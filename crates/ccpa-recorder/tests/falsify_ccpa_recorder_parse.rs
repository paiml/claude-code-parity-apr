//! `parse_messages_request` falsification harness.
//!
//! Asserts the contract `trace_schema § Record` shape is the byte-stable
//! output of feeding it canonical Anthropic Messages-API request bodies.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods // serde_json::json! expands to internal unwrap
)]

use ccpa_recorder::{parse_messages_request, ParseError};
use ccpa_trace::{Block, Record, StopReason, SCHEMA_VERSION};
use serde_json::json;

#[test]
fn empty_messages_array_is_rejected() {
    let body = json!({ "messages": [] }).to_string();
    let result = parse_messages_request(&body);
    assert!(matches!(result, Err(ParseError::EmptyMessages)));
}

#[test]
fn malformed_json_is_rejected() {
    let body = "{not json";
    let result = parse_messages_request(body);
    assert!(matches!(result, Err(ParseError::BadRequest(_))));
}

#[test]
fn unknown_role_is_rejected() {
    let body = json!({
        "messages": [{ "role": "telepath", "content": "x" }]
    })
    .to_string();
    let result = parse_messages_request(&body);
    assert!(matches!(result, Err(ParseError::UnexpectedRole(_))));
}

#[test]
fn single_user_text_message_emits_user_prompt() {
    let body = json!({
        "messages": [{ "role": "user", "content": "fix the failing test" }]
    })
    .to_string();
    let records = parse_messages_request(&body).expect("parses");
    assert_eq!(records.len(), 1);
    match &records[0] {
        Record::UserPrompt { v, turn, text } => {
            assert_eq!(*v, SCHEMA_VERSION);
            assert_eq!(*turn, 0);
            assert_eq!(text, "fix the failing test");
        }
        other => panic!("expected UserPrompt, got {other:?}"),
    }
}

#[test]
fn user_blocks_with_text_emit_user_prompt() {
    let body = json!({
        "messages": [{
            "role": "user",
            "content": [{ "type": "text", "text": "hello" }]
        }]
    })
    .to_string();
    let records = parse_messages_request(&body).expect("parses");
    assert_eq!(records.len(), 1);
    matches!(records[0], Record::UserPrompt { .. });
}

#[test]
fn assistant_text_message_emits_end_turn() {
    let body = json!({
        "messages": [
            { "role": "user", "content": "hi" },
            { "role": "assistant", "content": "hello back" }
        ]
    })
    .to_string();
    let records = parse_messages_request(&body).expect("parses");
    assert_eq!(records.len(), 2);
    match &records[1] {
        Record::AssistantTurn {
            v,
            turn,
            blocks,
            stop_reason,
        } => {
            assert_eq!(*v, SCHEMA_VERSION);
            assert_eq!(*turn, 1);
            assert_eq!(blocks.len(), 1);
            assert!(matches!(blocks[0], Block::Text { .. }));
            assert_eq!(*stop_reason, StopReason::EndTurn);
        }
        other => panic!("expected AssistantTurn, got {other:?}"),
    }
}

#[test]
fn assistant_with_tool_use_block_emits_tool_use_stop_reason() {
    let body = json!({
        "messages": [
            { "role": "user", "content": "list files" },
            { "role": "assistant", "content": [
                { "type": "thinking", "thinking": "I should call ls" },
                { "type": "text", "text": "I'll list the files." },
                { "type": "tool_use", "id": "toolu_01x", "name": "Bash", "input": { "command": "ls" } }
            ]}
        ]
    })
    .to_string();
    let records = parse_messages_request(&body).expect("parses");
    assert_eq!(records.len(), 2);
    match &records[1] {
        Record::AssistantTurn {
            blocks,
            stop_reason,
            ..
        } => {
            assert_eq!(blocks.len(), 3);
            assert!(matches!(blocks[0], Block::Thinking { .. }));
            assert!(matches!(blocks[1], Block::Text { .. }));
            assert!(matches!(blocks[2], Block::ToolUse { .. }));
            assert_eq!(*stop_reason, StopReason::ToolUse);
        }
        other => panic!("expected AssistantTurn, got {other:?}"),
    }
}

#[test]
fn user_tool_result_block_emits_tool_result_record() {
    let body = json!({
        "messages": [
            { "role": "user", "content": [{
                "type": "tool_result",
                "tool_use_id": "toolu_01x",
                "content": "file1.txt\nfile2.txt"
            }]}
        ]
    })
    .to_string();
    let records = parse_messages_request(&body).expect("parses");
    assert_eq!(records.len(), 1);
    match &records[0] {
        Record::ToolResult {
            v,
            tool_use_id,
            ok,
            content,
            ..
        } => {
            assert_eq!(*v, SCHEMA_VERSION);
            assert_eq!(tool_use_id, "toolu_01x");
            assert!(*ok);
            assert_eq!(content, "file1.txt\nfile2.txt");
        }
        other => panic!("expected ToolResult, got {other:?}"),
    }
}

#[test]
fn tool_result_with_is_error_sets_ok_false() {
    let body = json!({
        "messages": [
            { "role": "user", "content": [{
                "type": "tool_result",
                "tool_use_id": "toolu_01x",
                "content": "command not found",
                "is_error": true
            }]}
        ]
    })
    .to_string();
    let records = parse_messages_request(&body).expect("parses");
    match &records[0] {
        Record::ToolResult { ok, .. } => assert!(!*ok),
        other => panic!("expected ToolResult, got {other:?}"),
    }
}

#[test]
fn tool_result_with_json_content_is_stringified() {
    let body = json!({
        "messages": [
            { "role": "user", "content": [{
                "type": "tool_result",
                "tool_use_id": "toolu_01x",
                "content": [{ "type": "text", "text": "structured" }]
            }]}
        ]
    })
    .to_string();
    let records = parse_messages_request(&body).expect("parses");
    match &records[0] {
        Record::ToolResult { content, .. } => assert!(content.contains("structured")),
        other => panic!("expected ToolResult, got {other:?}"),
    }
}

#[test]
fn user_blocks_with_thinking_or_tool_use_are_silently_skipped() {
    // forward-compat: user-role messages shouldn't carry these per spec,
    // but if they appear we ignore them rather than fail.
    let body = json!({
        "messages": [{
            "role": "user",
            "content": [
                { "type": "thinking", "thinking": "user thinking?" },
                { "type": "tool_use", "id": "x", "name": "y", "input": {} }
            ]
        }]
    })
    .to_string();
    let records = parse_messages_request(&body).expect("parses");
    assert!(records.is_empty(), "user thinking/tool_use blocks ignored");
}

#[test]
fn assistant_blocks_with_tool_result_are_silently_skipped() {
    // forward-compat: assistant-role messages shouldn't carry tool_result
    // blocks per spec, but if they do we drop them.
    let body = json!({
        "messages": [{
            "role": "assistant",
            "content": [
                { "type": "text", "text": "ok" },
                { "type": "tool_result", "tool_use_id": "x", "content": "spurious" }
            ]
        }]
    })
    .to_string();
    let records = parse_messages_request(&body).expect("parses");
    match &records[0] {
        Record::AssistantTurn { blocks, .. } => {
            assert_eq!(blocks.len(), 1, "spurious tool_result dropped");
            assert!(matches!(blocks[0], Block::Text { .. }));
        }
        other => panic!("expected AssistantTurn, got {other:?}"),
    }
}

#[test]
fn full_4_turn_session_round_trips_cleanly() {
    let body = json!({
        "messages": [
            { "role": "user", "content": "fix the failing test" },
            { "role": "assistant", "content": [
                { "type": "tool_use", "id": "toolu_01a", "name": "Bash", "input": { "command": "cargo test" } }
            ]},
            { "role": "user", "content": [
                { "type": "tool_result", "tool_use_id": "toolu_01a", "content": "1 failed" }
            ]},
            { "role": "assistant", "content": "I see — the test is failing." }
        ]
    })
    .to_string();
    let records = parse_messages_request(&body).expect("parses");
    assert_eq!(records.len(), 4);
    assert!(matches!(records[0], Record::UserPrompt { .. }));
    assert!(matches!(records[1], Record::AssistantTurn { .. }));
    assert!(matches!(records[2], Record::ToolResult { .. }));
    assert!(matches!(records[3], Record::AssistantTurn { .. }));
}
