//! Reconstruct an Anthropic SSE event stream into a single
//! [`Record::AssistantTurn`].
//!
//! Anthropic streams responses as a series of SSE events:
//!   - `message_start`        — opens the response, carries the empty `Message`
//!   - `content_block_start`  — opens block N with an empty placeholder
//!   - `content_block_delta`  — appends `text_delta` / `input_json_delta` /
//!                               `thinking_delta` to block N
//!   - `content_block_stop`   — closes block N
//!   - `message_delta`        — carries `stop_reason` + final `usage`
//!   - `message_stop`         — terminator
//!   - `ping`                 — heartbeat (ignored)
//!   - `error`                — terminal error
//!
//! [`reconstruct_sse_stream`] walks an iterator of events, accumulates
//! block deltas, and emits the same [`Record::AssistantTurn`] shape that
//! [`crate::response::parse_messages_response`] would produce for the
//! equivalent non-streaming body. This lets the rest of the pipeline
//! treat streaming and non-streaming uniformly.

use ccpa_trace::{Block, Record, StopReason, SCHEMA_VERSION};
use serde::Deserialize;
use thiserror::Error;

/// Errors specific to SSE reconstruction.
#[derive(Debug, Error)]
pub enum SseError {
    /// `data:` payload failed to parse against the SSE event schema.
    #[error("malformed SSE event payload: {0}")]
    BadEvent(#[from] serde_json::Error),
    /// `content_block_delta` arrived for a block index that was never
    /// opened by `content_block_start`.
    #[error("delta for unknown block index {0}")]
    UnknownBlockIndex(usize),
    /// `content_block_start` opened a block with an unrecognized type.
    #[error("unsupported content_block type: {0}")]
    UnsupportedBlockType(String),
    /// `input_json_delta` carried a partial-JSON fragment that we could
    /// not append (e.g. `ToolUse` block was never opened).
    #[error("input_json_delta on a non-tool_use block")]
    DeltaOnWrongBlock,
}

/// One parsed SSE event: discriminator on `type`.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SseEvent {
    /// Opens the response.
    MessageStart,
    /// Opens block at `index` with the given placeholder.
    ContentBlockStart {
        /// Block index (0-based).
        index: usize,
        /// Placeholder block (empty `text` / `thinking` / `tool_use`).
        content_block: SseBlockPlaceholder,
    },
    /// Appends a delta to the open block at `index`.
    ContentBlockDelta {
        /// Block index (0-based).
        index: usize,
        /// Delta payload.
        delta: SseDelta,
    },
    /// Closes block at `index`.
    ContentBlockStop {
        /// Block index (0-based).
        index: usize,
    },
    /// Carries the final `stop_reason`.
    MessageDelta {
        /// Inner delta carrying `stop_reason`.
        delta: MessageDeltaPayload,
    },
    /// Terminator.
    MessageStop,
    /// Heartbeat — ignored.
    Ping,
    /// Terminal error.
    Error {
        /// Error envelope (opaque).
        error: serde_json::Value,
    },
}

/// Placeholder block as emitted in `content_block_start`.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SseBlockPlaceholder {
    /// Text block — `text` always starts empty.
    Text {
        /// Initial empty text (ignored — deltas drive the content).
        #[serde(default)]
        text: String,
    },
    /// Thinking block — `thinking` always starts empty.
    Thinking {
        /// Initial empty thinking text (ignored — deltas drive content).
        #[serde(default)]
        thinking: String,
    },
    /// Tool-use block — `input` accumulates partial-JSON deltas.
    ToolUse {
        /// Tool-call id (final).
        id: String,
        /// Tool name (final).
        name: String,
    },
}

/// Delta payload variants.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SseDelta {
    /// Append text to a [`SseBlockPlaceholder::Text`] block.
    TextDelta {
        /// Text fragment to append.
        text: String,
    },
    /// Append text to a [`SseBlockPlaceholder::Thinking`] block.
    ThinkingDelta {
        /// Thinking fragment to append.
        thinking: String,
    },
    /// Append a partial-JSON fragment to a [`SseBlockPlaceholder::ToolUse`]
    /// block's accumulating `input`.
    InputJsonDelta {
        /// Partial-JSON fragment.
        partial_json: String,
    },
}

/// Inner payload of `message_delta`.
#[derive(Debug, Deserialize)]
pub struct MessageDeltaPayload {
    /// Final stop reason for the assistant turn.
    pub stop_reason: Option<String>,
}

/// Walk a sequence of [`SseEvent`]s and reconstruct a single
/// [`Record::AssistantTurn`]. Mirrors what
/// [`crate::response::parse_messages_response`] would produce for the
/// equivalent non-streaming body.
///
/// # Errors
///
/// Returns [`SseError`] on malformed event sequences (delta with no
/// matching start, unknown block kind, partial-json on non-tool_use).
pub fn reconstruct_sse_stream(
    events: impl IntoIterator<Item = SseEvent>,
    turn: u32,
) -> Result<Record, SseError> {
    let mut blocks: Vec<Option<MutableBlock>> = Vec::new();
    let mut explicit_stop_reason: Option<String> = None;

    for event in events {
        match event {
            // No-op events: message_start/stop, ping, content_block_stop,
            // and error all leave accumulated state untouched. error is
            // terminal but we still return the partial turn built so far.
            SseEvent::MessageStart
            | SseEvent::MessageStop
            | SseEvent::Ping
            | SseEvent::ContentBlockStop { .. }
            | SseEvent::Error { .. } => {}
            SseEvent::ContentBlockStart {
                index,
                content_block,
            } => {
                while blocks.len() <= index {
                    blocks.push(None);
                }
                blocks[index] = Some(MutableBlock::from_placeholder(content_block));
            }
            SseEvent::ContentBlockDelta { index, delta } => {
                let slot = blocks
                    .get_mut(index)
                    .ok_or(SseError::UnknownBlockIndex(index))?;
                let block = slot.as_mut().ok_or(SseError::UnknownBlockIndex(index))?;
                block.apply_delta(delta)?;
            }
            SseEvent::MessageDelta { delta } => {
                if let Some(reason) = delta.stop_reason {
                    explicit_stop_reason = Some(reason);
                }
            }
        }
    }

    let final_blocks: Vec<Block> = blocks
        .into_iter()
        .flatten()
        .map(MutableBlock::finalize)
        .collect();
    let stop_reason = explicit_stop_reason
        .as_deref()
        .map_or_else(|| infer_stop_reason(&final_blocks), parse_stop_reason);

    Ok(Record::AssistantTurn {
        v: SCHEMA_VERSION,
        turn,
        blocks: final_blocks,
        stop_reason,
    })
}

/// Parse an SSE wire-format string (one or more `event:` / `data:` blocks
/// separated by blank lines) into a Vec of [`SseEvent`]s. Pure function;
/// no IO.
///
/// # Errors
///
/// Returns [`SseError::BadEvent`] if any `data:` line fails to parse as
/// JSON against the [`SseEvent`] schema.
pub fn parse_sse_wire_format(wire: &str) -> Result<Vec<SseEvent>, SseError> {
    let mut events = Vec::new();
    for chunk in wire.split("\n\n") {
        for line in chunk.lines() {
            if let Some(payload) = line.strip_prefix("data: ") {
                let event: SseEvent = serde_json::from_str(payload)?;
                events.push(event);
            }
            // `event:` lines are advisory; the `type` field inside
            // `data:` JSON is what we dispatch on. Ignore other fields
            // (id:, retry:, comments).
        }
    }
    Ok(events)
}

enum MutableBlock {
    Text(String),
    Thinking(String),
    ToolUse {
        id: String,
        name: String,
        input_json: String,
    },
}

impl MutableBlock {
    fn from_placeholder(p: SseBlockPlaceholder) -> Self {
        match p {
            SseBlockPlaceholder::Text { text } => Self::Text(text),
            SseBlockPlaceholder::Thinking { thinking } => Self::Thinking(thinking),
            SseBlockPlaceholder::ToolUse { id, name } => Self::ToolUse {
                id,
                name,
                input_json: String::new(),
            },
        }
    }

    fn apply_delta(&mut self, delta: SseDelta) -> Result<(), SseError> {
        match (self, delta) {
            (Self::Text(buf), SseDelta::TextDelta { text }) => {
                buf.push_str(&text);
                Ok(())
            }
            (Self::Thinking(buf), SseDelta::ThinkingDelta { thinking }) => {
                buf.push_str(&thinking);
                Ok(())
            }
            (Self::ToolUse { input_json, .. }, SseDelta::InputJsonDelta { partial_json }) => {
                input_json.push_str(&partial_json);
                Ok(())
            }
            _ => Err(SseError::DeltaOnWrongBlock),
        }
    }

    fn finalize(self) -> Block {
        match self {
            Self::Text(text) => Block::Text { text },
            Self::Thinking(thinking) => Block::Thinking { thinking },
            Self::ToolUse {
                id,
                name,
                input_json,
            } => {
                let input = if input_json.is_empty() {
                    serde_json::Value::Object(serde_json::Map::new())
                } else {
                    serde_json::from_str(&input_json)
                        .unwrap_or(serde_json::Value::String(input_json))
                };
                Block::ToolUse { id, name, input }
            }
        }
    }
}

fn infer_stop_reason(blocks: &[Block]) -> StopReason {
    if blocks.iter().any(|b| matches!(b, Block::ToolUse { .. })) {
        StopReason::ToolUse
    } else {
        StopReason::EndTurn
    }
}

fn parse_stop_reason(s: &str) -> StopReason {
    match s {
        "max_tokens" => StopReason::MaxTokens,
        "stop_sequence" => StopReason::StopSequence,
        "tool_use" => StopReason::ToolUse,
        _ => StopReason::EndTurn,
    }
}
