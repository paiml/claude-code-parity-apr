//! FALSIFY-CCPA-002 (`replay_determinism`) +
//! FALSIFY-CCPA-003 (`mock_completeness`).
//!
//! Mock-side gates: the orchestrator and `RecordedDriver` are
//! deterministic by construction; we assert that.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods // serde_json::json! expands to internal unwrap
)]

use ccpa_replayer::{replay, LlmDriver, RecordedDriver, ReplayError};
use ccpa_trace::{Actor, Block, Record, StopReason, SCHEMA_VERSION};
use serde_json::json;

fn user_prompt(turn: u32, text: &str) -> Record {
    Record::UserPrompt {
        v: SCHEMA_VERSION,
        turn,
        text: text.to_owned(),
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

fn tool_result(turn: u32) -> Record {
    Record::ToolResult {
        v: SCHEMA_VERSION,
        turn,
        tool_use_id: "x".into(),
        ok: true,
        content: "ok".into(),
        side_effects: None,
    }
}

fn next_turn(blocks: Vec<Block>, sr: StopReason) -> ccpa_replayer::NextTurn {
    ccpa_replayer::NextTurn {
        blocks,
        stop_reason: sr,
    }
}

fn text_block(s: &str) -> Block {
    Block::Text { text: s.to_owned() }
}

fn tool_use_block(id: &str, name: &str, input: serde_json::Value) -> Block {
    Block::ToolUse {
        id: id.to_owned(),
        name: name.to_owned(),
        input,
    }
}

fn driver_for(turns: &[(Vec<Block>, StopReason)]) -> RecordedDriver {
    RecordedDriver::new(
        turns
            .iter()
            .map(|(b, s)| ccpa_replayer::NextTurn {
                blocks: b.clone(),
                stop_reason: *s,
            })
            .collect(),
    )
}

#[test]
fn empty_teacher_with_empty_driver_yields_empty_student() {
    let mut driver = driver_for(&[]);
    let student = replay(&[], &mut driver).expect("ok");
    assert!(student.is_empty());
}

#[test]
fn user_prompt_passes_through_unchanged() {
    let teacher = vec![user_prompt(0, "hi")];
    let mut driver = driver_for(&[]);
    let student = replay(&teacher, &mut driver).expect("ok");
    assert_eq!(student, teacher);
}

#[test]
fn tool_result_passes_through_unchanged() {
    let teacher = vec![tool_result(2)];
    let mut driver = driver_for(&[]);
    let student = replay(&teacher, &mut driver).expect("ok");
    assert_eq!(student, teacher);
}

#[test]
fn single_assistant_turn_consumed_from_driver() {
    let teacher = vec![assistant_turn(1, vec![text_block("hi")])];
    let mut driver = driver_for(&[(vec![text_block("hi")], StopReason::EndTurn)]);
    let student = replay(&teacher, &mut driver).expect("ok");
    assert_eq!(student.len(), 1);
    matches!(student[0], Record::AssistantTurn { .. });
}

#[test]
fn full_4_turn_session_replays_byte_identical_when_driver_matches() {
    let teacher = vec![
        user_prompt(0, "fix the test"),
        assistant_turn(
            1,
            vec![tool_use_block(
                "t1",
                "Bash",
                json!({ "command": "cargo test" }),
            )],
        ),
        tool_result(2),
        assistant_turn(3, vec![text_block("done")]),
    ];
    let mut driver = driver_for(&[
        (
            vec![tool_use_block(
                "t1",
                "Bash",
                json!({ "command": "cargo test" }),
            )],
            StopReason::ToolUse,
        ),
        (vec![text_block("done")], StopReason::EndTurn),
    ]);
    let student = replay(&teacher, &mut driver).expect("ok");
    assert_eq!(
        student, teacher,
        "matching driver yields byte-identical replay"
    );
}

#[test]
fn determinism_two_replays_byte_identical() {
    let teacher = vec![
        user_prompt(0, "x"),
        assistant_turn(1, vec![text_block("y")]),
    ];
    let make_driver = || driver_for(&[(vec![text_block("y")], StopReason::EndTurn)]);
    let s1 = replay(&teacher, &mut make_driver()).expect("ok");
    let s2 = replay(&teacher, &mut make_driver()).expect("ok");
    assert_eq!(s1, s2, "FALSIFY-CCPA-002: deterministic by construction");
}

#[test]
fn driver_exhausted_when_teacher_has_more_assistant_turns() {
    let teacher = vec![
        assistant_turn(1, vec![text_block("a")]),
        assistant_turn(3, vec![text_block("b")]),
    ];
    let mut driver = driver_for(&[(vec![text_block("a")], StopReason::EndTurn)]); // missing 2nd
    let result = replay(&teacher, &mut driver);
    assert!(matches!(
        result,
        Err(ReplayError::DriverExhausted {
            position: 1,
            total: 1
        })
    ));
}

#[test]
fn driver_has_remaining_when_teacher_has_fewer_assistant_turns() {
    let teacher = vec![assistant_turn(1, vec![text_block("a")])];
    let mut driver = driver_for(&[
        (vec![text_block("a")], StopReason::EndTurn),
        (vec![text_block("EXTRA")], StopReason::EndTurn), // never consumed
    ]);
    let result = replay(&teacher, &mut driver);
    assert!(matches!(
        result,
        Err(ReplayError::DriverHasRemaining { remaining: 1 })
    ));
}

#[test]
fn recorded_driver_remaining_decrements_on_consume() {
    let mut driver = driver_for(&[
        (vec![text_block("a")], StopReason::EndTurn),
        (vec![text_block("b")], StopReason::EndTurn),
    ]);
    assert_eq!(driver.remaining(), 2);
    let _ = driver.next_turn().expect("ok");
    assert_eq!(driver.remaining(), 1);
    let _ = driver.next_turn().expect("ok");
    assert_eq!(driver.remaining(), 0);
}

#[test]
fn recorded_driver_returns_exhausted_after_running_out() {
    let mut driver = driver_for(&[(vec![text_block("a")], StopReason::EndTurn)]);
    let _ = driver.next_turn().expect("ok");
    let result = driver.next_turn();
    assert!(matches!(
        result,
        Err(ReplayError::DriverExhausted {
            position: 1,
            total: 1
        })
    ));
}

#[test]
fn session_start_and_end_pass_through() {
    let teacher = vec![
        Record::SessionStart {
            v: SCHEMA_VERSION,
            session_id: "x".into(),
            ts: "t".into(),
            actor: Actor::ClaudeCode,
            model: "m".into(),
            cwd_sha256: "0".repeat(64),
        },
        Record::SessionEnd {
            v: SCHEMA_VERSION,
            turn: 0,
            stop_reason: StopReason::EndTurn,
            elapsed_ms: 0,
            tokens_in: 0,
            tokens_out: 0,
        },
    ];
    let mut driver = driver_for(&[]);
    let student = replay(&teacher, &mut driver).expect("ok");
    assert_eq!(student, teacher);
}

#[test]
fn recorded_driver_clone_eq_debug() {
    let d = driver_for(&[(vec![text_block("a")], StopReason::EndTurn)]);
    let d2 = d.clone();
    assert_eq!(d, d2);
    let _ = format!("{d:?}");
}

#[test]
fn next_turn_struct_clone_eq_debug() {
    let nt = next_turn(vec![text_block("hi")], StopReason::EndTurn);
    let nt2 = nt.clone();
    assert_eq!(nt, nt2);
    let _ = format!("{nt:?}");
}

#[test]
fn replay_error_display_exhausted() {
    let err = ReplayError::DriverExhausted {
        position: 5,
        total: 3,
    };
    let s = format!("{err}");
    assert!(s.contains("exhausted"));
}

#[test]
fn replay_error_display_remaining() {
    let err = ReplayError::DriverHasRemaining { remaining: 2 };
    let s = format!("{err}");
    assert!(s.contains("unconsumed"));
}

#[test]
fn replay_error_clone_eq() {
    let a = ReplayError::DriverExhausted {
        position: 1,
        total: 0,
    };
    let b = a.clone();
    assert_eq!(a, b);
}
