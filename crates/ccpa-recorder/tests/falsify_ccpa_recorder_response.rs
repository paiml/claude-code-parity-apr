//! Falsification harness for `parse_messages_response` (non-streaming
//! Anthropic Messages-API response).

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods // serde_json::json! expands to internal unwrap
)]

use ccpa_recorder::{parse_messages_response, ParseError};
use ccpa_trace::{Block, Record, StopReason};
use serde_json::json;

#[test]
fn malformed_response_is_rejected() {
    let body = "{not json";
    assert!(matches!(
        parse_messages_response(body, 1),
        Err(ParseError::BadRequest(_))
    ));
}

#[test]
fn empty_content_emits_assistant_turn_with_no_blocks() {
    let body = json!({ "content": [], "stop_reason": "end_turn" }).to_string();
    let record = parse_messages_response(&body, 1).expect("parses");
    match record {
        Record::AssistantTurn {
            blocks,
            stop_reason,
            turn,
            ..
        } => {
            assert_eq!(turn, 1);
            assert!(blocks.is_empty());
            assert_eq!(stop_reason, StopReason::EndTurn);
        }
        other => panic!("expected AssistantTurn, got {other:?}"),
    }
}

#[test]
fn text_block_round_trips() {
    let body = json!({
        "content": [{ "type": "text", "text": "hello" }],
        "stop_reason": "end_turn"
    })
    .to_string();
    let record = parse_messages_response(&body, 1).expect("parses");
    match record {
        Record::AssistantTurn { blocks, .. } => {
            assert_eq!(blocks.len(), 1);
            assert!(matches!(blocks[0], Block::Text { ref text } if text == "hello"));
        }
        other => panic!("expected AssistantTurn, got {other:?}"),
    }
}

#[test]
fn thinking_block_round_trips() {
    let body = json!({
        "content": [{ "type": "thinking", "thinking": "ponder" }],
        "stop_reason": "end_turn"
    })
    .to_string();
    let record = parse_messages_response(&body, 1).expect("parses");
    match record {
        Record::AssistantTurn { blocks, .. } => {
            assert!(matches!(blocks[0], Block::Thinking { .. }));
        }
        other => panic!("expected AssistantTurn, got {other:?}"),
    }
}

#[test]
fn tool_use_block_implies_tool_use_stop_reason_when_unspecified() {
    let body = json!({
        "content": [{
            "type": "tool_use",
            "id": "toolu_01",
            "name": "Bash",
            "input": { "command": "ls" }
        }]
        // stop_reason intentionally absent
    })
    .to_string();
    let record = parse_messages_response(&body, 3).expect("parses");
    match record {
        Record::AssistantTurn {
            blocks,
            stop_reason,
            turn,
            ..
        } => {
            assert_eq!(turn, 3);
            assert!(matches!(blocks[0], Block::ToolUse { .. }));
            assert_eq!(stop_reason, StopReason::ToolUse);
        }
        other => panic!("expected AssistantTurn, got {other:?}"),
    }
}

#[test]
fn explicit_max_tokens_stop_reason_round_trips() {
    let body = json!({
        "content": [{ "type": "text", "text": "..." }],
        "stop_reason": "max_tokens"
    })
    .to_string();
    let record = parse_messages_response(&body, 1).expect("parses");
    match record {
        Record::AssistantTurn { stop_reason, .. } => {
            assert_eq!(stop_reason, StopReason::MaxTokens);
        }
        other => panic!("expected AssistantTurn, got {other:?}"),
    }
}

#[test]
fn explicit_stop_sequence_stop_reason_round_trips() {
    let body = json!({
        "content": [{ "type": "text", "text": "x" }],
        "stop_reason": "stop_sequence"
    })
    .to_string();
    let record = parse_messages_response(&body, 1).expect("parses");
    match record {
        Record::AssistantTurn { stop_reason, .. } => {
            assert_eq!(stop_reason, StopReason::StopSequence);
        }
        other => panic!("expected AssistantTurn, got {other:?}"),
    }
}

#[test]
fn explicit_tool_use_stop_reason_overrides_inference() {
    let body = json!({
        "content": [{ "type": "text", "text": "x" }],   // text only
        "stop_reason": "tool_use"                       // but explicitly tool_use
    })
    .to_string();
    let record = parse_messages_response(&body, 1).expect("parses");
    match record {
        Record::AssistantTurn { stop_reason, .. } => {
            assert_eq!(stop_reason, StopReason::ToolUse);
        }
        other => panic!("expected AssistantTurn, got {other:?}"),
    }
}

#[test]
fn unknown_stop_reason_defaults_to_end_turn() {
    let body = json!({
        "content": [{ "type": "text", "text": "x" }],
        "stop_reason": "future_unknown_reason"
    })
    .to_string();
    let record = parse_messages_response(&body, 1).expect("parses");
    match record {
        Record::AssistantTurn { stop_reason, .. } => {
            assert_eq!(stop_reason, StopReason::EndTurn);
        }
        other => panic!("expected AssistantTurn, got {other:?}"),
    }
}

#[test]
fn missing_stop_reason_falls_back_to_inference() {
    let body = json!({
        "content": [{ "type": "text", "text": "x" }]
        // no stop_reason
    })
    .to_string();
    let record = parse_messages_response(&body, 1).expect("parses");
    match record {
        Record::AssistantTurn { stop_reason, .. } => {
            assert_eq!(stop_reason, StopReason::EndTurn);
        }
        other => panic!("expected AssistantTurn, got {other:?}"),
    }
}
