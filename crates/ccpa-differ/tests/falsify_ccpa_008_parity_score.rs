//! FALSIFY-CCPA-008 — `parity_score_bound` (per-trace, not yet aggregate).
//!
//! Asserts the `parity_score` reduction defined in
//! `contracts/claude-code-parity-apr-v1.yaml § parity_score`.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods, // serde_json::json! expands to internal unwrap
    clippy::float_cmp           // exact-equal score comparisons are intentional
)]

use ccpa_differ::{compute_parity_score, DriftCategory};
use ccpa_trace::{Actor, Block, Record, StopReason, SCHEMA_VERSION};
use serde_json::json;

fn session_start() -> Record {
    Record::SessionStart {
        v: SCHEMA_VERSION,
        session_id: "s".into(),
        ts: "t".into(),
        actor: Actor::ClaudeCode,
        model: "m".into(),
        cwd_sha256: "0".repeat(64),
    }
}

fn assistant_turn(turn: u32, blocks: Vec<Block>) -> Record {
    let stop_reason = if blocks.iter().any(|b| matches!(b, Block::ToolUse { .. })) {
        StopReason::ToolUse
    } else {
        StopReason::EndTurn
    };
    Record::AssistantTurn {
        v: SCHEMA_VERSION,
        turn,
        blocks,
        stop_reason,
    }
}

fn tool_use(id: &str, name: &str, input: serde_json::Value) -> Block {
    Block::ToolUse {
        id: id.to_owned(),
        name: name.to_owned(),
        input,
    }
}

#[test]
fn empty_traces_yield_perfect_score() {
    let report = compute_parity_score(&[], &[]);
    assert_eq!(report.score, 1.0);
    assert_eq!(report.teacher_count, 0);
    assert_eq!(report.student_count, 0);
    assert!(report.drifts.is_empty());
}

#[test]
fn teacher_empty_student_calls_yields_zero_score() {
    let student = vec![assistant_turn(
        1,
        vec![tool_use("t1", "Bash", json!({ "command": "ls" }))],
    )];
    let report = compute_parity_score(&[], &student);
    assert_eq!(report.score, 0.0);
    assert_eq!(report.teacher_count, 0);
    assert_eq!(report.student_count, 1);
    assert_eq!(report.drifts.len(), 1);
    assert_eq!(report.drifts[0].category, DriftCategory::ExtraToolCall);
}

#[test]
fn teacher_calls_student_empty_yields_zero_with_missing_drifts() {
    let teacher = vec![assistant_turn(
        1,
        vec![tool_use("t1", "Bash", json!({ "command": "ls" }))],
    )];
    let report = compute_parity_score(&teacher, &[]);
    assert_eq!(report.score, 0.0);
    assert_eq!(report.teacher_count, 1);
    assert_eq!(report.student_count, 0);
    assert_eq!(report.drifts.len(), 1);
    assert_eq!(report.drifts[0].category, DriftCategory::MissingToolCall);
    assert_eq!(report.drifts[0].tool_name, "Bash");
}

#[test]
fn identical_single_call_traces_score_one() {
    let trace = vec![assistant_turn(
        1,
        vec![tool_use("t1", "Bash", json!({ "command": "ls" }))],
    )];
    let report = compute_parity_score(&trace, &trace);
    assert_eq!(report.score, 1.0);
    assert_eq!(report.matched_count, 1);
    assert!(report.drifts.is_empty());
}

#[test]
fn mismatched_tool_input_drops_score() {
    let teacher = vec![assistant_turn(
        1,
        vec![tool_use("t1", "Bash", json!({ "command": "ls" }))],
    )];
    let student = vec![assistant_turn(
        1,
        vec![tool_use("s1", "Bash", json!({ "command": "cat /tmp" }))],
    )];
    let report = compute_parity_score(&teacher, &student);
    assert_eq!(report.score, 0.0);
    assert_eq!(report.matched_count, 0);
    assert_eq!(report.drifts.len(), 1);
    assert_eq!(
        report.drifts[0].category,
        DriftCategory::MismatchedToolInput
    );
}

#[test]
fn mismatched_tool_name_drops_score() {
    let teacher = vec![assistant_turn(
        1,
        vec![tool_use("t1", "Bash", json!({ "command": "ls" }))],
    )];
    let student = vec![assistant_turn(
        1,
        vec![tool_use("s1", "Read", json!({ "path": "/tmp" }))],
    )];
    let report = compute_parity_score(&teacher, &student);
    assert_eq!(report.score, 0.0);
    assert_eq!(report.drifts[0].category, DriftCategory::MismatchedToolName);
}

#[test]
fn three_calls_two_match_score_is_two_thirds() {
    let teacher = vec![assistant_turn(
        1,
        vec![
            tool_use("t1", "Bash", json!({ "command": "ls" })),
            tool_use("t2", "Read", json!({ "path": "/x" })),
            tool_use("t3", "Glob", json!({ "pattern": "*.rs" })),
        ],
    )];
    let student = vec![assistant_turn(
        1,
        vec![
            tool_use("s1", "Bash", json!({ "command": "ls" })),
            tool_use("s2", "Read", json!({ "path": "/different" })), // mismatch
            tool_use("s3", "Glob", json!({ "pattern": "*.rs" })),
        ],
    )];
    let report = compute_parity_score(&teacher, &student);
    assert!((report.score - 2.0_f64 / 3.0).abs() < 1e-9);
    assert_eq!(report.matched_count, 2);
    assert_eq!(report.teacher_count, 3);
    assert_eq!(report.drifts.len(), 1);
    assert_eq!(
        report.drifts[0].category,
        DriftCategory::MismatchedToolInput
    );
    assert_eq!(report.drifts[0].position, 1);
}

#[test]
fn student_extra_calls_emit_extra_drifts() {
    let teacher = vec![assistant_turn(
        1,
        vec![tool_use("t1", "Bash", json!({ "command": "ls" }))],
    )];
    let student = vec![assistant_turn(
        1,
        vec![
            tool_use("s1", "Bash", json!({ "command": "ls" })),
            tool_use("s2", "Read", json!({ "path": "/x" })),
        ],
    )];
    let report = compute_parity_score(&teacher, &student);
    assert_eq!(report.score, 1.0); // still 1/1 matched on teacher's denominator
    assert_eq!(report.student_count, 2);
    assert_eq!(report.drifts.len(), 1);
    assert_eq!(report.drifts[0].category, DriftCategory::ExtraToolCall);
    assert_eq!(report.drifts[0].position, 1);
    assert_eq!(report.drifts[0].tool_name, "Read");
}

#[test]
fn student_missing_calls_emit_missing_drifts() {
    let teacher = vec![assistant_turn(
        1,
        vec![
            tool_use("t1", "Bash", json!({ "command": "ls" })),
            tool_use("t2", "Read", json!({ "path": "/x" })),
        ],
    )];
    let student = vec![assistant_turn(
        1,
        vec![tool_use("s1", "Bash", json!({ "command": "ls" }))],
    )];
    let report = compute_parity_score(&teacher, &student);
    assert_eq!(report.score, 0.5);
    assert_eq!(report.matched_count, 1);
    assert_eq!(report.drifts.len(), 1);
    assert_eq!(report.drifts[0].category, DriftCategory::MissingToolCall);
    assert_eq!(report.drifts[0].position, 1);
    assert_eq!(report.drifts[0].tool_name, "Read");
}

#[test]
fn calls_across_multiple_assistant_turns_aggregate() {
    let teacher = vec![
        session_start(),
        assistant_turn(1, vec![tool_use("t1", "Bash", json!({ "command": "ls" }))]),
        assistant_turn(3, vec![tool_use("t2", "Read", json!({ "path": "/x" }))]),
    ];
    let student = teacher.clone();
    let report = compute_parity_score(&teacher, &student);
    assert_eq!(report.teacher_count, 2);
    assert_eq!(report.student_count, 2);
    assert_eq!(report.score, 1.0);
}

#[test]
fn non_assistant_records_are_ignored() {
    let teacher = vec![
        session_start(),
        Record::UserPrompt {
            v: SCHEMA_VERSION,
            turn: 0,
            text: "noise".into(),
        },
        assistant_turn(1, vec![tool_use("t1", "Bash", json!({ "command": "ls" }))]),
        Record::ToolResult {
            v: SCHEMA_VERSION,
            turn: 2,
            tool_use_id: "t1".into(),
            ok: true,
            content: "ok".into(),
            side_effects: None,
        },
    ];
    let student = vec![assistant_turn(
        1,
        vec![tool_use("s1", "Bash", json!({ "command": "ls" }))],
    )];
    let report = compute_parity_score(&teacher, &student);
    assert_eq!(report.teacher_count, 1, "only tool_use blocks count");
    assert_eq!(report.score, 1.0);
}

#[test]
fn assistant_turn_with_only_text_blocks_contributes_zero_calls() {
    let teacher = vec![assistant_turn(
        1,
        vec![Block::Text {
            text: "hello".into(),
        }],
    )];
    let report = compute_parity_score(&teacher, &[]);
    assert_eq!(report.teacher_count, 0);
    assert_eq!(report.score, 1.0);
}

#[test]
fn drift_struct_is_clone_and_debug() {
    let teacher = vec![assistant_turn(
        1,
        vec![tool_use("t1", "Bash", json!({ "command": "ls" }))],
    )];
    let report = compute_parity_score(&teacher, &[]);
    let drift = report.drifts[0].clone();
    let _ = format!("{drift:?}");
    assert_eq!(drift.category, DriftCategory::MissingToolCall);
}
