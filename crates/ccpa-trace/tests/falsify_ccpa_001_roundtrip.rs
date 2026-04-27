//! FALSIFY-CCPA-001 — `trace_schema_roundtrip`.
//!
//! Asserts that every record kind round-trips through
//! `Trace::from_jsonl` / `Trace::to_jsonl` byte-identical (modulo
//! lexicographic field ordering produced by `serde_json`).
//!
//! Source-of-truth schema: `contracts/claude-code-parity-apr-v1.yaml
//! § trace_schema`. Adding a new record kind requires updating both this
//! test and the contract YAML; CI will fail otherwise.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods // serde_json::json! expands to internal unwrap
)]

use ccpa_trace::{
    Actor, Block, HookDecision, Record, SideEffects, SkillSource, StopReason, Trace,
    SCHEMA_VERSION,
};
use serde_json::json;

fn fixture_session() -> Trace {
    Trace {
        records: vec![
            Record::SessionStart {
                v: 1,
                session_id: "0192f0e0-0000-7000-8000-000000000001".to_owned(),
                ts: "2026-04-26T01:23:45Z".to_owned(),
                actor: Actor::ClaudeCode,
                model: "claude-sonnet-4-6".to_owned(),
                cwd_sha256: "0".repeat(64),
            },
            Record::HookEvent {
                v: 1,
                turn: 0,
                event: "SessionStart".to_owned(),
                matcher: None,
                decision: HookDecision::Allow,
                exit_code: 0,
                output: String::new(),
            },
            Record::UserPrompt {
                v: 1,
                turn: 0,
                text: "fix the failing test".to_owned(),
            },
            Record::SkillInvocation {
                v: 1,
                turn: 0,
                name: "rust-debug".to_owned(),
                source: SkillSource::AutoMatched,
                instructions_injected: true,
            },
            Record::AssistantTurn {
                v: 1,
                turn: 1,
                blocks: vec![
                    Block::Thinking {
                        thinking: "Plan: read tests, run cargo, patch.".to_owned(),
                    },
                    Block::Text {
                        text: "I'll start by checking the test output.".to_owned(),
                    },
                    Block::ToolUse {
                        id: "toolu_01abc".to_owned(),
                        name: "Bash".to_owned(),
                        input: json!({ "command": "cargo test --lib" }),
                    },
                ],
                stop_reason: StopReason::ToolUse,
            },
            Record::HookEvent {
                v: 1,
                turn: 1,
                event: "PreToolUse".to_owned(),
                matcher: Some("Bash".to_owned()),
                decision: HookDecision::Warn,
                exit_code: 1,
                output: "warning: long-running cargo test".to_owned(),
            },
            Record::ToolResult {
                v: 1,
                turn: 2,
                tool_use_id: "toolu_01abc".to_owned(),
                ok: false,
                content: "test result: FAILED. 1 failed".to_owned(),
                side_effects: Some(SideEffects {
                    files_read: vec!["tests/example.rs".to_owned()],
                    files_written: vec![],
                    exit_code: Some(101),
                }),
            },
            Record::SessionEnd {
                v: 1,
                turn: 2,
                stop_reason: StopReason::EndTurn,
                elapsed_ms: 12_340,
                tokens_in: 4_521,
                tokens_out: 891,
            },
        ],
    }
}

#[test]
fn record_kinds_round_trip_byte_identical() {
    let original = fixture_session();
    let jsonl = original
        .to_jsonl()
        .expect("to_jsonl never fails on bundled types");
    let parsed = Trace::from_jsonl(&jsonl).expect("schema-valid JSONL must parse");
    assert_eq!(
        original, parsed,
        "trace must round-trip via Trace::from_jsonl/to_jsonl"
    );
    let twice = parsed.to_jsonl().expect("re-serialize");
    assert_eq!(jsonl, twice, "JSONL must be byte-identical on re-serialize");
}

#[test]
fn empty_trace_round_trips() {
    let trace = Trace::new();
    let jsonl = trace.to_jsonl().expect("empty serializes");
    assert!(jsonl.is_empty(), "empty trace serializes to empty string");
    let parsed = Trace::from_jsonl("").expect("empty parses");
    assert_eq!(trace, parsed);
}

#[test]
fn empty_lines_are_tolerated_in_parser() {
    let jsonl = "\n\n\n";
    let parsed = Trace::from_jsonl(jsonl).expect("blank lines OK");
    assert!(parsed.records.is_empty());
}

#[test]
fn unknown_record_kind_is_rejected() {
    let jsonl = r#"{"kind":"telepathy","v":1}"#;
    let result = Trace::from_jsonl(jsonl);
    assert!(result.is_err(), "unknown kind must fail to parse");
}

#[test]
fn missing_required_field_is_rejected() {
    let jsonl = r#"{"kind":"user_prompt","v":1}"#; // missing `turn` and `text`
    let result = Trace::from_jsonl(jsonl);
    assert!(result.is_err(), "missing required field must fail to parse");
}

#[test]
fn actor_round_trips_via_serde() {
    for actor in [Actor::ClaudeCode, Actor::AprCode] {
        let s = serde_json::to_string(&actor).expect("serialize Actor");
        let back: Actor = serde_json::from_str(&s).expect("deserialize Actor");
        assert_eq!(actor, back);
    }
}

#[test]
fn stop_reason_round_trips_via_serde() {
    for sr in [
        StopReason::EndTurn,
        StopReason::MaxTokens,
        StopReason::StopSequence,
        StopReason::ToolUse,
        StopReason::Error,
    ] {
        let s = serde_json::to_string(&sr).expect("serialize StopReason");
        let back: StopReason = serde_json::from_str(&s).expect("deserialize StopReason");
        assert_eq!(sr, back);
    }
}

#[test]
fn block_round_trips_via_serde() {
    let blocks = vec![
        Block::Text {
            text: "hello".to_owned(),
        },
        Block::Thinking {
            thinking: "ponder".to_owned(),
        },
        Block::ToolUse {
            id: "toolu_x".to_owned(),
            name: "Read".to_owned(),
            input: json!({ "path": "src/lib.rs" }),
        },
    ];
    for block in blocks {
        let s = serde_json::to_string(&block).expect("serialize Block");
        let back: Block = serde_json::from_str(&s).expect("deserialize Block");
        assert_eq!(block, back);
    }
}

#[test]
fn side_effects_skips_empty_optional_fields() {
    let se = SideEffects::default();
    let s = serde_json::to_string(&se).expect("serialize");
    // Default elides all fields (empty vecs + None exit_code).
    assert_eq!(s, "{}", "empty SideEffects serializes as {{}}");
    let back: SideEffects = serde_json::from_str("{}").expect("deserialize empty");
    assert_eq!(se, back);
}

#[test]
fn schema_version_constant_is_two() {
    assert_eq!(SCHEMA_VERSION, 2);
}

#[test]
fn hook_decision_round_trips_via_serde() {
    for d in [HookDecision::Allow, HookDecision::Warn, HookDecision::Block] {
        let s = serde_json::to_string(&d).expect("serialize HookDecision");
        let back: HookDecision = serde_json::from_str(&s).expect("deserialize HookDecision");
        assert_eq!(d, back);
    }
}

#[test]
fn skill_source_round_trips_via_serde() {
    for s in [SkillSource::UserInvoked, SkillSource::AutoMatched] {
        let json_s = serde_json::to_string(&s).expect("serialize SkillSource");
        let back: SkillSource = serde_json::from_str(&json_s).expect("deserialize SkillSource");
        assert_eq!(s, back);
    }
}

#[test]
fn hook_event_record_round_trips() {
    let r = Record::HookEvent {
        v: 1,
        turn: 1,
        event: "PreToolUse".to_owned(),
        matcher: Some("Bash".to_owned()),
        decision: HookDecision::Block,
        exit_code: 2,
        output: "blocked: dangerous command".to_owned(),
    };
    let s = serde_json::to_string(&r).expect("serialize HookEvent");
    let back: Record = serde_json::from_str(&s).expect("deserialize HookEvent");
    assert_eq!(r, back);
}

#[test]
fn skill_invocation_record_round_trips() {
    let r = Record::SkillInvocation {
        v: 1,
        turn: 1,
        name: "rust-debug".to_owned(),
        source: SkillSource::UserInvoked,
        instructions_injected: true,
    };
    let s = serde_json::to_string(&r).expect("serialize SkillInvocation");
    let back: Record = serde_json::from_str(&s).expect("deserialize SkillInvocation");
    assert_eq!(r, back);
}

#[test]
fn hook_event_optional_matcher_omitted_when_none() {
    let r = Record::HookEvent {
        v: 1,
        turn: 0,
        event: "SessionStart".to_owned(),
        matcher: None,
        decision: HookDecision::Allow,
        exit_code: 0,
        output: String::new(),
    };
    let s = serde_json::to_string(&r).expect("serialize");
    assert!(!s.contains("matcher"), "None matcher must not be emitted");
    assert!(!s.contains("output"), "empty output must not be emitted");
}

#[test]
fn skill_invocation_default_instructions_injected_is_false() {
    // explicit false is omitted by serde default-skip OR included; either is fine,
    // what matters is that deserialize tolerates absence.
    let jsonl = r#"{"kind":"skill_invocation","v":1,"turn":1,"name":"x","source":"user_invoked"}"#;
    let r: Record = serde_json::from_str(jsonl).expect("parse without instructions_injected");
    if let Record::SkillInvocation {
        instructions_injected,
        ..
    } = r
    {
        assert!(
            !instructions_injected,
            "absent instructions_injected defaults to false"
        );
    } else {
        panic!("expected SkillInvocation");
    }
}

#[test]
fn trace_default_is_empty() {
    let trace = Trace::default();
    assert!(trace.records.is_empty());
}
