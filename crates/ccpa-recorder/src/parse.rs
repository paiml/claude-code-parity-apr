//! Parse Anthropic Messages-API request bodies into trace records.
//!
//! A single `POST /v1/messages` request body carries the FULL conversation
//! history — every prior user prompt, assistant turn, and tool result.
//! [`parse_messages_request`] walks that history once and emits one
//! [`Record`] per surface.
//!
//! Response-body parsing (and SSE streaming) is a separate module added in
//! a follow-up PR.

use ccpa_trace::{Block, Record, StopReason, SCHEMA_VERSION};
use serde::Deserialize;
use thiserror::Error;

/// Parse failures.
#[derive(Debug, Error)]
pub enum ParseError {
    /// JSON did not match the expected request shape.
    #[error("malformed Messages-API request body: {0}")]
    BadRequest(#[from] serde_json::Error),
    /// `messages[]` was empty — every Anthropic request must carry at
    /// least one message.
    #[error("Messages-API request had empty messages[] array")]
    EmptyMessages,
    /// A `tool_result` block referenced an unknown role or shape.
    #[error("unexpected role in messages[]: {0}")]
    UnexpectedRole(String),
}

#[derive(Deserialize)]
struct MessagesRequest {
    messages: Vec<AnthropicMessage>,
}

#[derive(Deserialize)]
struct AnthropicMessage {
    role: String,
    content: AnthropicContent,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum AnthropicContent {
    Text(String),
    Blocks(Vec<AnthropicBlock>),
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicBlock {
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
    ToolResult {
        tool_use_id: String,
        content: serde_json::Value,
        #[serde(default)]
        is_error: bool,
    },
}

/// Parse one Messages-API request body into a sequence of trace records.
///
/// The returned `Vec<Record>` reconstructs the conversation history as
/// observed by the proxy at this request: one [`Record::UserPrompt`] per
/// user-text message, one [`Record::AssistantTurn`] per prior assistant
/// turn, and one [`Record::ToolResult`] per `tool_result` block. Caller
/// is responsible for adding [`Record::SessionStart`] / [`Record::SessionEnd`]
/// (which carry runtime context — `session_id`, `cwd_sha256`, timestamps)
/// and for de-duplicating against records already written from prior
/// requests in the same session.
///
/// `tool_use` blocks emitted by the assistant are nested inside
/// [`Block::ToolUse`] within the appropriate `AssistantTurn`.
///
/// # Errors
///
/// Returns [`ParseError`] if the body fails JSON-schema validation
/// against the Anthropic Messages-API request shape, if the `messages[]`
/// array is empty, or if any message has an unrecognized `role`.
pub fn parse_messages_request(body: &str) -> Result<Vec<Record>, ParseError> {
    let req: MessagesRequest = serde_json::from_str(body)?;

    if req.messages.is_empty() {
        return Err(ParseError::EmptyMessages);
    }

    let mut out = Vec::with_capacity(req.messages.len());
    let mut user_turn: u32 = 0;
    let mut assistant_turn: u32 = 1;

    for msg in req.messages {
        match msg.role.as_str() {
            "user" => match msg.content {
                AnthropicContent::Text(text) => {
                    out.push(Record::UserPrompt {
                        v: SCHEMA_VERSION,
                        turn: user_turn,
                        text,
                    });
                    user_turn = user_turn.saturating_add(2);
                }
                AnthropicContent::Blocks(blocks) => {
                    for block in blocks {
                        match block {
                            AnthropicBlock::Text { text } => {
                                out.push(Record::UserPrompt {
                                    v: SCHEMA_VERSION,
                                    turn: user_turn,
                                    text,
                                });
                                user_turn = user_turn.saturating_add(2);
                            }
                            AnthropicBlock::ToolResult {
                                tool_use_id,
                                content,
                                is_error,
                            } => {
                                let content_str = stringify_tool_result_content(&content);
                                out.push(Record::ToolResult {
                                    v: SCHEMA_VERSION,
                                    turn: assistant_turn.saturating_add(1),
                                    tool_use_id,
                                    ok: !is_error,
                                    content: content_str,
                                    side_effects: None,
                                });
                            }
                            AnthropicBlock::Thinking { .. } | AnthropicBlock::ToolUse { .. } => {
                                // user-role messages do not carry these blocks per spec;
                                // ignore for forward-compat.
                            }
                        }
                    }
                }
            },
            "assistant" => {
                let blocks = match msg.content {
                    AnthropicContent::Text(text) => vec![Block::Text { text }],
                    AnthropicContent::Blocks(blocks) => blocks
                        .into_iter()
                        .filter_map(convert_assistant_block)
                        .collect(),
                };
                let stop_reason = if blocks.iter().any(|b| matches!(b, Block::ToolUse { .. })) {
                    StopReason::ToolUse
                } else {
                    StopReason::EndTurn
                };
                out.push(Record::AssistantTurn {
                    v: SCHEMA_VERSION,
                    turn: assistant_turn,
                    blocks,
                    stop_reason,
                });
                assistant_turn = assistant_turn.saturating_add(2);
            }
            other => return Err(ParseError::UnexpectedRole(other.to_owned())),
        }
    }

    Ok(out)
}

fn convert_assistant_block(b: AnthropicBlock) -> Option<Block> {
    match b {
        AnthropicBlock::Text { text } => Some(Block::Text { text }),
        AnthropicBlock::Thinking { thinking } => Some(Block::Thinking { thinking }),
        AnthropicBlock::ToolUse { id, name, input } => Some(Block::ToolUse { id, name, input }),
        AnthropicBlock::ToolResult { .. } => None, // assistant-role never emits tool_result
    }
}

fn stringify_tool_result_content(content: &serde_json::Value) -> String {
    match content {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}
