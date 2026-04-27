//! Falsification harness for [`RecorderSession`] — the stateful recorder
//! that wraps the three pure parsers and writes JSONL to a file.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods // serde_json::json! expands to internal unwrap
)]

use std::fs;
use std::io::{self, Write};

use ccpa_recorder::{RecorderSession, ResponseBody, SessionConfig, SessionError};
use ccpa_trace::{Actor, Record, StopReason, Trace};
use serde_json::json;
use tempfile::tempdir;

/// Test writer that swallows the first `n` `write` calls successfully,
/// then returns `WriteZero` on every subsequent call. Lets us deterministically
/// trigger the IO error paths in `new_with_writer`, `record_exchange`, `close`.
struct FailAfter(usize);

impl Write for FailAfter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.0 == 0 {
            return Err(io::Error::new(io::ErrorKind::WriteZero, "boom"));
        }
        self.0 -= 1;
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn cfg(path: std::path::PathBuf) -> SessionConfig {
    SessionConfig {
        output_path: path,
        session_id: "0192f0e0-0000-7000-8000-000000000001".to_owned(),
        ts_start: "2026-04-26T01:23:45Z".to_owned(),
        actor: Actor::ClaudeCode,
        model: "claude-sonnet-4-6".to_owned(),
        cwd_sha256: "0".repeat(64),
    }
}

fn read_trace(path: &std::path::Path) -> Trace {
    let body = fs::read_to_string(path).expect("read");
    Trace::from_jsonl(&body).expect("parse")
}

#[test]
fn open_writes_session_start_and_close_writes_session_end() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("0001.ccpa-trace.jsonl");
    let session = RecorderSession::open(cfg(path.clone())).expect("open");
    session.close(StopReason::EndTurn, 0).expect("close");
    let trace = read_trace(&path);
    assert_eq!(trace.records.len(), 2);
    assert!(matches!(trace.records[0], Record::SessionStart { .. }));
    assert!(matches!(trace.records[1], Record::SessionEnd { .. }));
}

#[test]
fn record_exchange_appends_history_records() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("0002.ccpa-trace.jsonl");
    let mut session = RecorderSession::open(cfg(path.clone())).expect("open");

    let req = json!({
        "messages": [{ "role": "user", "content": "hi" }]
    })
    .to_string();
    let resp = json!({
        "content": [{ "type": "text", "text": "hello back" }],
        "stop_reason": "end_turn"
    })
    .to_string();

    session
        .record_exchange(&req, ResponseBody::Json(&resp), 10, 5)
        .expect("record_exchange");
    session.close(StopReason::EndTurn, 1234).expect("close");

    let trace = read_trace(&path);
    // Expected: SessionStart + UserPrompt + AssistantTurn + SessionEnd
    assert_eq!(trace.records.len(), 4);
    assert!(matches!(trace.records[0], Record::SessionStart { .. }));
    assert!(matches!(trace.records[1], Record::UserPrompt { .. }));
    assert!(matches!(trace.records[2], Record::AssistantTurn { .. }));
    match &trace.records[3] {
        Record::SessionEnd {
            elapsed_ms,
            tokens_in,
            tokens_out,
            ..
        } => {
            assert_eq!(*elapsed_ms, 1234);
            assert_eq!(*tokens_in, 10);
            assert_eq!(*tokens_out, 5);
        }
        other => panic!("expected SessionEnd, got {other:?}"),
    }
}

#[test]
fn second_exchange_only_writes_new_records() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("0003.ccpa-trace.jsonl");
    let mut session = RecorderSession::open(cfg(path.clone())).expect("open");

    let req1 = json!({
        "messages": [{ "role": "user", "content": "first" }]
    })
    .to_string();
    let resp1 = json!({
        "content": [{ "type": "text", "text": "ack one" }],
        "stop_reason": "end_turn"
    })
    .to_string();
    session
        .record_exchange(&req1, ResponseBody::Json(&resp1), 0, 0)
        .expect("ex1");

    // Second exchange: request body re-sends the prior user+assistant
    // and adds a new user prompt. Recorder should ONLY append the new
    // user prompt + the new assistant response.
    let req2 = json!({
        "messages": [
            { "role": "user", "content": "first" },
            { "role": "assistant", "content": "ack one" },
            { "role": "user", "content": "second" }
        ]
    })
    .to_string();
    let resp2 = json!({
        "content": [{ "type": "text", "text": "ack two" }],
        "stop_reason": "end_turn"
    })
    .to_string();
    session
        .record_exchange(&req2, ResponseBody::Json(&resp2), 0, 0)
        .expect("ex2");
    session.close(StopReason::EndTurn, 1).expect("close");

    let trace = read_trace(&path);
    // Expected records: SessionStart, UserPrompt(first), AssistantTurn(ack one),
    //                   UserPrompt(second), AssistantTurn(ack two), SessionEnd
    assert_eq!(trace.records.len(), 6);
    assert!(matches!(trace.records[0], Record::SessionStart { .. }));
    assert!(matches!(trace.records[1], Record::UserPrompt { .. }));
    assert!(matches!(trace.records[2], Record::AssistantTurn { .. }));
    assert!(matches!(trace.records[3], Record::UserPrompt { .. }));
    assert!(matches!(trace.records[4], Record::AssistantTurn { .. }));
    assert!(matches!(trace.records[5], Record::SessionEnd { .. }));
}

#[test]
fn record_exchange_with_sse_response_uses_sse_path() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("0004.ccpa-trace.jsonl");
    let mut session = RecorderSession::open(cfg(path.clone())).expect("open");

    let req = json!({ "messages": [{ "role": "user", "content": "stream me" }] }).to_string();
    let wire = concat!(
        "data: {\"type\":\"message_start\"}\n",
        "\n",
        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n",
        "\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n",
        "\n",
        "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n",
        "\n",
        "data: {\"type\":\"message_stop\"}\n",
        "\n",
    );
    session
        .record_exchange(&req, ResponseBody::Sse(wire), 0, 0)
        .expect("record");
    session.close(StopReason::EndTurn, 0).expect("close");

    let trace = read_trace(&path);
    let assistant = trace
        .records
        .iter()
        .find(|r| matches!(r, Record::AssistantTurn { .. }))
        .expect("found assistant turn");
    if let Record::AssistantTurn { blocks, .. } = assistant {
        assert_eq!(blocks.len(), 1);
    }
}

#[test]
fn malformed_request_propagates_parse_error() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("0005.ccpa-trace.jsonl");
    let mut session = RecorderSession::open(cfg(path.clone())).expect("open");
    let result = session.record_exchange("not json", ResponseBody::Json("{}"), 0, 0);
    assert!(matches!(result, Err(SessionError::Parse(_))));
}

#[test]
fn malformed_response_propagates_parse_error() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("0006.ccpa-trace.jsonl");
    let mut session = RecorderSession::open(cfg(path.clone())).expect("open");
    let req = json!({ "messages": [{ "role": "user", "content": "hi" }] }).to_string();
    let result = session.record_exchange(&req, ResponseBody::Json("{not json"), 0, 0);
    assert!(matches!(result, Err(SessionError::Parse(_))));
}

#[test]
fn malformed_sse_propagates_sse_error() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("0007.ccpa-trace.jsonl");
    let mut session = RecorderSession::open(cfg(path.clone())).expect("open");
    let req = json!({ "messages": [{ "role": "user", "content": "hi" }] }).to_string();
    let bad_wire = "data: {bad json\n\n";
    let result = session.record_exchange(&req, ResponseBody::Sse(bad_wire), 0, 0);
    assert!(matches!(result, Err(SessionError::Sse(_))));
}

#[test]
fn open_with_unwritable_path_returns_io_error() {
    let cfg = SessionConfig {
        output_path: std::path::PathBuf::from("/this/path/does/not/exist/0008.jsonl"),
        session_id: "x".to_owned(),
        ts_start: "x".to_owned(),
        actor: Actor::AprCode,
        model: "x".to_owned(),
        cwd_sha256: "0".repeat(64),
    };
    let result = RecorderSession::open(cfg);
    assert!(matches!(result, Err(SessionError::Io(_))));
}

#[test]
fn token_counts_accumulate_across_exchanges() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("0009.ccpa-trace.jsonl");
    let mut session = RecorderSession::open(cfg(path.clone())).expect("open");

    let req = json!({ "messages": [{ "role": "user", "content": "x" }] }).to_string();
    let resp = json!({ "content": [], "stop_reason": "end_turn" }).to_string();

    session
        .record_exchange(&req, ResponseBody::Json(&resp), 100, 50)
        .expect("ex1");

    let req2 = json!({
        "messages": [
            { "role": "user", "content": "x" },
            { "role": "assistant", "content": [] },
            { "role": "user", "content": "y" }
        ]
    })
    .to_string();
    session
        .record_exchange(&req2, ResponseBody::Json(&resp), 25, 10)
        .expect("ex2");

    session.close(StopReason::EndTurn, 0).expect("close");

    let trace = read_trace(&path);
    let end = trace.records.last().expect("last");
    match end {
        Record::SessionEnd {
            tokens_in,
            tokens_out,
            ..
        } => {
            assert_eq!(*tokens_in, 125, "100 + 25");
            assert_eq!(*tokens_out, 60, "50 + 10");
        }
        other => panic!("expected SessionEnd, got {other:?}"),
    }
}

#[test]
fn new_with_writer_propagates_session_start_write_failure() {
    // FailAfter(0) → first write fails immediately → SessionStart write
    // surfaces as Err(SessionError::Io).
    let result = RecorderSession::new_with_writer(FailAfter(0), cfg("ignored".into()));
    assert!(matches!(result, Err(SessionError::Io(_))));
}

#[test]
fn close_propagates_session_end_write_failure() {
    // FailAfter(2) lets SessionStart's two write_all calls (line + newline)
    // through, then fails on close()'s SessionEnd write.
    let session = RecorderSession::new_with_writer(FailAfter(2), cfg("ignored".into()))
        .expect("SessionStart succeeds");
    let result = session.close(StopReason::EndTurn, 99_999);
    assert!(matches!(result, Err(SessionError::Io(_))));
}

#[test]
fn record_exchange_propagates_history_write_failure() {
    // FailAfter(2) lets SessionStart through, fails on the first
    // UserPrompt record write inside record_exchange.
    let mut session = RecorderSession::new_with_writer(FailAfter(2), cfg("ignored".into()))
        .expect("SessionStart succeeds");
    let req = json!({ "messages": [{ "role": "user", "content": "x" }] }).to_string();
    let resp = json!({ "content": [], "stop_reason": "end_turn" }).to_string();
    let result = session.record_exchange(&req, ResponseBody::Json(&resp), 0, 0);
    assert!(matches!(result, Err(SessionError::Io(_))));
}

#[test]
fn record_exchange_propagates_response_write_failure() {
    // FailAfter(4) lets SessionStart (2) + UserPrompt (2) through, fails
    // on the AssistantTurn (response) write.
    let mut session = RecorderSession::new_with_writer(FailAfter(4), cfg("ignored".into()))
        .expect("SessionStart succeeds");
    let req = json!({ "messages": [{ "role": "user", "content": "x" }] }).to_string();
    let resp = json!({ "content": [], "stop_reason": "end_turn" }).to_string();
    let result = session.record_exchange(&req, ResponseBody::Json(&resp), 0, 0);
    assert!(matches!(result, Err(SessionError::Io(_))));
}

#[test]
fn assistant_turn_includes_tool_use_block() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("0010.ccpa-trace.jsonl");
    let mut session = RecorderSession::open(cfg(path.clone())).expect("open");

    let req = json!({ "messages": [{ "role": "user", "content": "ls" }] }).to_string();
    let resp = json!({
        "content": [
            { "type": "tool_use", "id": "toolu_a", "name": "Bash", "input": { "command": "ls" } }
        ],
        "stop_reason": "tool_use"
    })
    .to_string();
    session
        .record_exchange(&req, ResponseBody::Json(&resp), 0, 0)
        .expect("record");
    session.close(StopReason::ToolUse, 0).expect("close");

    let trace = read_trace(&path);
    let assistant = trace
        .records
        .iter()
        .find(|r| matches!(r, Record::AssistantTurn { .. }))
        .expect("assistant");
    if let Record::AssistantTurn {
        stop_reason,
        blocks,
        ..
    } = assistant
    {
        assert_eq!(*stop_reason, StopReason::ToolUse);
        assert_eq!(blocks.len(), 1);
    }
}
