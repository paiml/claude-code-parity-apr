//! Parse Anthropic Messages-API response bodies.
//!
//! Two response shapes:
//!   - non-streaming: a single JSON `Message` body
//!   - streaming: a sequence of SSE events ([`crate::sse`])
//!
//! [`parse_messages_response`] handles the non-streaming form. SSE event
//! reconstruction lives in [`crate::sse`] but produces the same
//! [`Record::AssistantTurn`] shape, so callers can treat both uniformly.

use ccpa_trace::{Block, Record, StopReason, SCHEMA_VERSION};
use serde::Deserialize;

use crate::ParseError;

#[derive(Deserialize)]
struct MessagesResponse {
    content: Vec<ResponseBlock>,
    #[serde(default)]
    stop_reason: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ResponseBlock {
    Text {
        text: String,
    },
    Thinking {
        thinking: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

/// Parse one Messages-API non-streaming response body into a single
/// [`Record::AssistantTurn`].
///
/// `turn` is the 1-based turn number caller assigns (typically computed
/// from the request that produced this response).
///
/// # Errors
///
/// Returns [`ParseError::BadRequest`] (re-used name; same JSON-shape
/// failure) if the body fails to deserialize against the response schema.
pub fn parse_messages_response(body: &str, turn: u32) -> Result<Record, ParseError> {
    let resp: MessagesResponse = serde_json::from_str(body)?;
    let blocks: Vec<Block> = resp
        .content
        .into_iter()
        .map(convert_response_block)
        .collect();
    let stop_reason = resp
        .stop_reason
        .as_deref()
        .map_or_else(|| infer_stop_reason(&blocks), parse_stop_reason);
    Ok(Record::AssistantTurn {
        v: SCHEMA_VERSION,
        turn,
        blocks,
        stop_reason,
    })
}

fn convert_response_block(b: ResponseBlock) -> Block {
    match b {
        ResponseBlock::Text { text } => Block::Text { text },
        ResponseBlock::Thinking { thinking } => Block::Thinking { thinking },
        ResponseBlock::ToolUse { id, name, input } => Block::ToolUse { id, name, input },
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
        // "end_turn" and any unknown value default to EndTurn — Anthropic
        // is allowed to add new reasons; treating them as end_turn is the
        // forward-compat-safe default.
        _ => StopReason::EndTurn,
    }
}
