//! Falsification harness for SSE reconstruction
//! ([`ccpa_recorder::reconstruct_sse_stream`] +
//! [`ccpa_recorder::parse_sse_wire_format`]).

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods // serde_json::json! expands to internal unwrap
)]

use ccpa_recorder::{parse_sse_wire_format, reconstruct_sse_stream, SseError, SseEvent};
use ccpa_trace::{Block, Record, StopReason};

fn ev(json: &str) -> SseEvent {
    parse_sse_wire_format(&format!("data: {json}\n\n"))
        .expect("parses")
        .into_iter()
        .next()
        .expect("one event")
}

#[test]
fn message_start_and_stop_alone_produce_empty_turn() {
    let events = vec![
        ev(r#"{"type":"message_start"}"#),
        ev(r#"{"type":"message_stop"}"#),
    ];
    let record = reconstruct_sse_stream(events, 1).expect("reconstructs");
    match record {
        Record::AssistantTurn { blocks, .. } => assert!(blocks.is_empty()),
        other => panic!("expected AssistantTurn, got {other:?}"),
    }
}

#[test]
fn ping_events_are_ignored() {
    let events = vec![ev(r#"{"type":"ping"}"#)];
    let record = reconstruct_sse_stream(events, 1).expect("reconstructs");
    match record {
        Record::AssistantTurn { blocks, .. } => assert!(blocks.is_empty()),
        other => panic!("expected AssistantTurn, got {other:?}"),
    }
}

#[test]
fn text_block_accumulates_deltas() {
    let events = vec![
        ev(r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#),
        ev(
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hel"}}"#,
        ),
        ev(r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"lo"}}"#),
        ev(r#"{"type":"content_block_stop","index":0}"#),
    ];
    let record = reconstruct_sse_stream(events, 1).expect("reconstructs");
    match record {
        Record::AssistantTurn { blocks, .. } => match &blocks[0] {
            Block::Text { text } => assert_eq!(text, "hello"),
            other => panic!("expected Text, got {other:?}"),
        },
        other => panic!("expected AssistantTurn, got {other:?}"),
    }
}

#[test]
fn thinking_block_accumulates_deltas() {
    let events = vec![
        ev(
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}"#,
        ),
        ev(
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"pon"}}"#,
        ),
        ev(
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"der"}}"#,
        ),
        ev(r#"{"type":"content_block_stop","index":0}"#),
    ];
    let record = reconstruct_sse_stream(events, 1).expect("reconstructs");
    match record {
        Record::AssistantTurn { blocks, .. } => match &blocks[0] {
            Block::Thinking { thinking } => assert_eq!(thinking, "ponder"),
            other => panic!("expected Thinking, got {other:?}"),
        },
        other => panic!("expected AssistantTurn, got {other:?}"),
    }
}

#[test]
fn tool_use_block_assembles_partial_json_into_object() {
    let events = vec![
        ev(
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_x","name":"Bash"}}"#,
        ),
        ev(
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"command\":\"l"}}"#,
        ),
        ev(
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"s\"}"}}"#,
        ),
        ev(r#"{"type":"content_block_stop","index":0}"#),
        ev(r#"{"type":"message_delta","delta":{"stop_reason":"tool_use"}}"#),
    ];
    let record = reconstruct_sse_stream(events, 1).expect("reconstructs");
    match record {
        Record::AssistantTurn {
            blocks,
            stop_reason,
            ..
        } => {
            match &blocks[0] {
                Block::ToolUse { id, name, input } => {
                    assert_eq!(id, "toolu_x");
                    assert_eq!(name, "Bash");
                    assert_eq!(input["command"], "ls");
                }
                other => panic!("expected ToolUse, got {other:?}"),
            }
            assert_eq!(stop_reason, StopReason::ToolUse);
        }
        other => panic!("expected AssistantTurn, got {other:?}"),
    }
}

#[test]
fn tool_use_with_invalid_partial_json_falls_back_to_string() {
    let events = vec![
        ev(
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"x","name":"y"}}"#,
        ),
        ev(
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"not json"}}"#,
        ),
    ];
    let record = reconstruct_sse_stream(events, 1).expect("reconstructs");
    match record {
        Record::AssistantTurn { blocks, .. } => match &blocks[0] {
            Block::ToolUse { input, .. } => {
                assert_eq!(input.as_str(), Some("not json"));
            }
            other => panic!("expected ToolUse, got {other:?}"),
        },
        other => panic!("expected AssistantTurn, got {other:?}"),
    }
}

#[test]
fn empty_tool_use_block_serializes_to_empty_object() {
    let events = vec![ev(
        r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"x","name":"y"}}"#,
    )];
    let record = reconstruct_sse_stream(events, 1).expect("reconstructs");
    match record {
        Record::AssistantTurn { blocks, .. } => match &blocks[0] {
            Block::ToolUse { input, .. } => assert!(input.is_object()),
            other => panic!("expected ToolUse, got {other:?}"),
        },
        other => panic!("expected AssistantTurn, got {other:?}"),
    }
}

#[test]
fn delta_for_unknown_index_is_rejected() {
    let events = vec![ev(
        r#"{"type":"content_block_delta","index":7,"delta":{"type":"text_delta","text":"x"}}"#,
    )];
    let result = reconstruct_sse_stream(events, 1);
    assert!(matches!(result, Err(SseError::UnknownBlockIndex(7))));
}

#[test]
fn text_delta_on_thinking_block_is_rejected() {
    let events = vec![
        ev(
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}"#,
        ),
        ev(r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"x"}}"#),
    ];
    let result = reconstruct_sse_stream(events, 1);
    assert!(matches!(result, Err(SseError::DeltaOnWrongBlock)));
}

#[test]
fn message_delta_with_max_tokens_sets_stop_reason() {
    let events = vec![
        ev(r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#),
        ev(r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"x"}}"#),
        ev(r#"{"type":"message_delta","delta":{"stop_reason":"max_tokens"}}"#),
    ];
    let record = reconstruct_sse_stream(events, 1).expect("reconstructs");
    match record {
        Record::AssistantTurn { stop_reason, .. } => {
            assert_eq!(stop_reason, StopReason::MaxTokens);
        }
        other => panic!("expected AssistantTurn, got {other:?}"),
    }
}

#[test]
fn message_delta_with_stop_sequence_round_trips() {
    let events = vec![ev(
        r#"{"type":"message_delta","delta":{"stop_reason":"stop_sequence"}}"#,
    )];
    let record = reconstruct_sse_stream(events, 1).expect("reconstructs");
    match record {
        Record::AssistantTurn { stop_reason, .. } => {
            assert_eq!(stop_reason, StopReason::StopSequence);
        }
        other => panic!("expected AssistantTurn, got {other:?}"),
    }
}

#[test]
fn message_delta_with_unknown_stop_reason_defaults_to_end_turn() {
    let events = vec![ev(
        r#"{"type":"message_delta","delta":{"stop_reason":"future_thing"}}"#,
    )];
    let record = reconstruct_sse_stream(events, 1).expect("reconstructs");
    match record {
        Record::AssistantTurn { stop_reason, .. } => {
            assert_eq!(stop_reason, StopReason::EndTurn);
        }
        other => panic!("expected AssistantTurn, got {other:?}"),
    }
}

#[test]
fn message_delta_without_stop_reason_falls_back_to_inference() {
    let events = vec![
        ev(r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#),
        ev(r#"{"type":"message_delta","delta":{}}"#),
    ];
    let record = reconstruct_sse_stream(events, 1).expect("reconstructs");
    match record {
        Record::AssistantTurn { stop_reason, .. } => {
            assert_eq!(stop_reason, StopReason::EndTurn);
        }
        other => panic!("expected AssistantTurn, got {other:?}"),
    }
}

#[test]
fn error_event_does_not_panic_and_returns_partial_turn() {
    let events = vec![
        ev(r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#),
        ev(r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hi"}}"#),
        ev(r#"{"type":"error","error":{"type":"overloaded_error","message":"sorry"}}"#),
    ];
    let record = reconstruct_sse_stream(events, 1).expect("reconstructs");
    match record {
        Record::AssistantTurn { blocks, .. } => match &blocks[0] {
            Block::Text { text } => assert_eq!(text, "hi"),
            other => panic!("expected Text, got {other:?}"),
        },
        other => panic!("expected AssistantTurn, got {other:?}"),
    }
}

#[test]
fn parse_sse_wire_format_round_trips_three_events() {
    let wire = concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\"}\n",
        "\n",
        "event: ping\n",
        "data: {\"type\":\"ping\"}\n",
        "\n",
        "event: message_stop\n",
        "data: {\"type\":\"message_stop\"}\n",
        "\n",
    );
    let events = parse_sse_wire_format(wire).expect("parses");
    assert_eq!(events.len(), 3);
}

#[test]
fn parse_sse_wire_format_rejects_malformed_payload() {
    let wire = "data: {bad json\n\n";
    let result = parse_sse_wire_format(wire);
    assert!(matches!(result, Err(SseError::BadEvent(_))));
}

#[test]
fn parse_sse_wire_format_ignores_non_data_lines() {
    // event:, id:, retry:, comments — all advisory; only data: drives state.
    let wire = concat!(
        "event: ping\n",
        ": this is a comment\n",
        "id: 42\n",
        "retry: 5000\n",
        "data: {\"type\":\"ping\"}\n",
        "\n",
    );
    let events = parse_sse_wire_format(wire).expect("parses");
    assert_eq!(events.len(), 1);
}
