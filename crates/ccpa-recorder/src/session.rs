//! Session-stateful recorder that streams [`Record`]s to a JSONL file.
//!
//! Wraps the three pure parsers ([`parse_messages_request`],
//! [`parse_messages_response`], [`reconstruct_sse_stream`]) with the
//! per-session bookkeeping needed to produce a clean
//! `.ccpa-trace.jsonl` file:
//!
//!   - emits one [`Record::SessionStart`] on `open`
//!   - tracks how many records have been written so subsequent
//!     `record_exchange` calls only emit the *new* tail of the
//!     conversation history (Anthropic re-sends the whole history
//!     each request, so naive forwarding would duplicate every record)
//!   - emits one [`Record::SessionEnd`] on `close` with elapsed
//!     wall-clock + cumulative usage
//!
//! The session writes JSONL with [`Trace::to_jsonl`] line shape — same
//! format that `FALSIFY-CCPA-001` round-trips against.
//!
//! [`parse_messages_request`]: crate::parse_messages_request
//! [`parse_messages_response`]: crate::parse_messages_response
//! [`reconstruct_sse_stream`]: crate::reconstruct_sse_stream
//! [`Trace::to_jsonl`]: ccpa_trace::Trace::to_jsonl

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use ccpa_trace::{Actor, Record, StopReason, SCHEMA_VERSION};
use thiserror::Error;

use crate::{
    parse_messages_request, parse_messages_response, parse_sse_wire_format, reconstruct_sse_stream,
    ParseError, SseError,
};

/// Errors that can arise during a recording session.
#[derive(Debug, Error)]
pub enum SessionError {
    /// Filesystem failure (open, write, flush).
    #[error("session IO error: {0}")]
    Io(#[from] std::io::Error),
    /// Wraps [`ParseError`] from request/response parsing.
    #[error(transparent)]
    Parse(#[from] ParseError),
    /// Wraps [`SseError`] from SSE reconstruction.
    #[error(transparent)]
    Sse(#[from] SseError),
    /// JSONL serialization failure (should be unreachable for bundled types).
    #[error("session serialization error: {0}")]
    Serialize(#[from] serde_json::Error),
}

/// Recorder configuration handed to [`RecorderSession::open`].
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// Output `.ccpa-trace.jsonl` path. Will be created if missing.
    pub output_path: PathBuf,
    /// Stable session id (`UUIDv7` recommended).
    pub session_id: String,
    /// ISO-8601 UTC timestamp at session start.
    pub ts_start: String,
    /// Producer of this trace.
    pub actor: Actor,
    /// Model id (assistant model the proxy is recording).
    pub model: String,
    /// Git tree hash of CWD at session start.
    pub cwd_sha256: String,
}

/// A recording session writing JSONL records to any [`Write`] sink as the
/// proxy observes Anthropic Messages-API exchanges. Generic over the
/// writer for testability — production callers use [`Self::open`] which
/// defaults the writer to `BufWriter<File>`.
pub struct RecorderSession<W: Write = BufWriter<File>> {
    writer: W,
    /// Number of conversation records (`UserPrompt` / `AssistantTurn` / `ToolResult`)
    /// already written from prior `record_exchange` calls. Subsequent
    /// exchanges only emit records past this index from the parsed history.
    records_written: usize,
    /// Cumulative input tokens billed.
    tokens_in: u64,
    /// Cumulative output tokens billed.
    tokens_out: u64,
    /// Last assistant turn number used (for response.parse turn assignment).
    last_assistant_turn: u32,
}

impl RecorderSession<BufWriter<File>> {
    /// Open a fresh recording session backed by a file, writing the
    /// `SessionStart` record.
    ///
    /// # Errors
    ///
    /// [`SessionError::Io`] if the output file cannot be created or the
    /// initial write fails.
    pub fn open(config: SessionConfig) -> Result<Self, SessionError> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&config.output_path)?;
        Self::new_with_writer(BufWriter::new(file), config)
    }
}

impl<W: Write> RecorderSession<W> {
    /// Construct a session writing to an arbitrary [`Write`] sink. Used
    /// internally by [`Self::open`] and externally by tests that inject
    /// failing writers to exercise IO error paths.
    ///
    /// # Errors
    ///
    /// Any error from the writer's first `SessionStart` write.
    pub fn new_with_writer(mut writer: W, config: SessionConfig) -> Result<Self, SessionError> {
        write_record_line(
            &mut writer,
            &Record::SessionStart {
                v: SCHEMA_VERSION,
                session_id: config.session_id,
                ts: config.ts_start,
                actor: config.actor,
                model: config.model,
                cwd_sha256: config.cwd_sha256,
            },
        )?;
        Ok(Self {
            writer,
            records_written: 0,
            tokens_in: 0,
            tokens_out: 0,
            last_assistant_turn: 0,
        })
    }

    /// Record one HTTP exchange. Parses the request body for the full
    /// conversation history (skipping records already written), then
    /// parses the response body and appends the assistant turn.
    ///
    /// `tokens_in_delta` / `tokens_out_delta` are added to the running
    /// session total written on [`close`](Self::close).
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] on parsing or IO failures.
    pub fn record_exchange(
        &mut self,
        request_body: &str,
        response_body: ResponseBody<'_>,
        tokens_in_delta: u64,
        tokens_out_delta: u64,
    ) -> Result<(), SessionError> {
        let history = parse_messages_request(request_body)?;
        for record in history.into_iter().skip(self.records_written) {
            write_record_line(&mut self.writer, &record)?;
            self.records_written = self.records_written.saturating_add(1);
        }

        let response_turn = self.last_assistant_turn.saturating_add(2).max(1);
        let response_record = match response_body {
            ResponseBody::Json(body) => parse_messages_response(body, response_turn)?,
            ResponseBody::Sse(wire) => {
                let events = parse_sse_wire_format(wire)?;
                reconstruct_sse_stream(events, response_turn)?
            }
        };
        write_record_line(&mut self.writer, &response_record)?;
        self.records_written = self.records_written.saturating_add(1);
        self.last_assistant_turn = response_turn;

        self.tokens_in = self.tokens_in.saturating_add(tokens_in_delta);
        self.tokens_out = self.tokens_out.saturating_add(tokens_out_delta);
        Ok(())
    }

    /// Close the session, writing the final `SessionEnd` record and
    /// flushing the writer.
    ///
    /// # Errors
    ///
    /// IO or serialization failures.
    pub fn close(mut self, stop_reason: StopReason, elapsed_ms: u64) -> Result<(), SessionError> {
        write_record_line(
            &mut self.writer,
            &Record::SessionEnd {
                v: SCHEMA_VERSION,
                turn: self.last_assistant_turn,
                stop_reason,
                elapsed_ms,
                tokens_in: self.tokens_in,
                tokens_out: self.tokens_out,
            },
        )?;
        self.writer.flush()?;
        Ok(())
    }
}

/// Discriminator for the response body shape of one exchange.
#[derive(Debug, Clone, Copy)]
pub enum ResponseBody<'a> {
    /// Non-streaming JSON body.
    Json(&'a str),
    /// Streaming SSE wire-format body.
    Sse(&'a str),
}

fn write_record_line<W: Write>(writer: &mut W, record: &Record) -> Result<(), SessionError> {
    let line = serde_json::to_string(record)?;
    writer.write_all(line.as_bytes())?;
    writer.write_all(b"\n")?;
    Ok(())
}
