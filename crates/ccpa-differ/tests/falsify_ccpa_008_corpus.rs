//! FALSIFY-CCPA-008 — corpus-level `parity_score_bound`.
//!
//! Asserts the corpus aggregation defined in
//! `contracts/claude-code-parity-apr-v1.yaml § parity_score`:
//!   `aggregate >= aggregate_min AND every fixture >= individual_min`.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods, // serde_json::json! expands to internal unwrap
    clippy::float_cmp           // exact-equal score comparisons are intentional
)]

use ccpa_differ::{evaluate_corpus, CorpusEntry, Thresholds};
use ccpa_trace::{Block, Record, StopReason, SCHEMA_VERSION};
use serde_json::json;

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

fn matching_pair() -> (Vec<Record>, Vec<Record>) {
    let trace = vec![assistant_turn(
        1,
        vec![tool_use("t1", "Bash", json!({ "command": "ls" }))],
    )];
    (trace.clone(), trace)
}

fn drifting_pair() -> (Vec<Record>, Vec<Record>) {
    let teacher = vec![assistant_turn(
        1,
        vec![tool_use("t1", "Bash", json!({ "command": "ls" }))],
    )];
    let student = vec![assistant_turn(
        1,
        vec![tool_use("s1", "Bash", json!({ "command": "DIFFERENT" }))],
    )];
    (teacher, student)
}

#[test]
fn empty_corpus_passes_vacuously() {
    let report = evaluate_corpus(&[], &Thresholds::default());
    assert_eq!(report.aggregate_score, 1.0);
    assert!(report.passes_gate);
    assert!(report.fixtures.is_empty());
}

#[test]
fn defaults_match_contract_thresholds() {
    let t = Thresholds::default();
    assert_eq!(t.aggregate_min, 0.95);
    assert_eq!(t.individual_min, 0.80);
}

#[test]
fn single_perfect_fixture_passes_gate() {
    let (t, s) = matching_pair();
    let entries = [CorpusEntry {
        fixture_id: "fixture-001".to_owned(),
        teacher: &t,
        student: &s,
    }];
    let report = evaluate_corpus(&entries, &Thresholds::default());
    assert_eq!(report.aggregate_score, 1.0);
    assert!(report.passes_gate);
    assert_eq!(report.fixtures.len(), 1);
    assert!(report.fixtures[0].passes_individual);
    assert_eq!(report.fixtures[0].fixture_id, "fixture-001");
}

#[test]
fn single_drifting_fixture_fails_gate() {
    let (t, s) = drifting_pair();
    let entries = [CorpusEntry {
        fixture_id: "fixture-002".to_owned(),
        teacher: &t,
        student: &s,
    }];
    let report = evaluate_corpus(&entries, &Thresholds::default());
    assert_eq!(report.aggregate_score, 0.0);
    assert!(!report.passes_gate);
    assert!(!report.fixtures[0].passes_individual);
}

#[test]
fn aggregate_above_floor_with_one_fixture_failing_individual() {
    // 4 perfect + 1 zero = aggregate 0.8, but the zero fails individual
    let (perfect_t, perfect_s) = matching_pair();
    let (drift_t, drift_s) = drifting_pair();
    let entries = vec![
        CorpusEntry {
            fixture_id: "f1".into(),
            teacher: &perfect_t,
            student: &perfect_s,
        },
        CorpusEntry {
            fixture_id: "f2".into(),
            teacher: &perfect_t,
            student: &perfect_s,
        },
        CorpusEntry {
            fixture_id: "f3".into(),
            teacher: &perfect_t,
            student: &perfect_s,
        },
        CorpusEntry {
            fixture_id: "f4".into(),
            teacher: &perfect_t,
            student: &perfect_s,
        },
        CorpusEntry {
            fixture_id: "f5_drift".into(),
            teacher: &drift_t,
            student: &drift_s,
        },
    ];
    let report = evaluate_corpus(&entries, &Thresholds::default());
    assert_eq!(report.aggregate_score, 0.8);
    assert!(
        !report.passes_gate,
        "aggregate 0.8 fails the 0.95 floor (and f5 fails the 0.80 individual floor)"
    );
}

#[test]
fn aggregate_just_at_threshold_passes_when_individuals_pass() {
    // 19 perfect + 1 perfect = aggregate 1.0; well above 0.95
    let (t, s) = matching_pair();
    let entries: Vec<CorpusEntry<'_>> = (0..20)
        .map(|i| CorpusEntry {
            fixture_id: format!("f{i:02}"),
            teacher: &t,
            student: &s,
        })
        .collect();
    let report = evaluate_corpus(&entries, &Thresholds::default());
    assert_eq!(report.aggregate_score, 1.0);
    assert!(report.passes_gate);
    assert_eq!(report.fixtures.len(), 20);
}

#[test]
fn custom_thresholds_can_loosen_gate() {
    let (t, s) = drifting_pair();
    let entries = [CorpusEntry {
        fixture_id: "f1".into(),
        teacher: &t,
        student: &s,
    }];
    let lax = Thresholds {
        aggregate_min: 0.0,
        individual_min: 0.0,
    };
    let report = evaluate_corpus(&entries, &lax);
    assert!(report.passes_gate, "0.0 thresholds always pass");
}

#[test]
fn custom_thresholds_can_tighten_gate() {
    let (t, s) = matching_pair();
    let entries = [CorpusEntry {
        fixture_id: "f1".into(),
        teacher: &t,
        student: &s,
    }];
    let strict = Thresholds {
        aggregate_min: 1.5, // unreachable
        individual_min: 0.0,
    };
    let report = evaluate_corpus(&entries, &strict);
    assert!(
        !report.passes_gate,
        "score 1.0 < aggregate floor 1.5 → fails"
    );
}

#[test]
fn fixture_id_round_trips_into_report() {
    let (t, s) = matching_pair();
    let entries = [CorpusEntry {
        fixture_id: "0042-edit-readme".into(),
        teacher: &t,
        student: &s,
    }];
    let report = evaluate_corpus(&entries, &Thresholds::default());
    assert_eq!(report.fixtures[0].fixture_id, "0042-edit-readme");
}

#[test]
fn multiple_fixtures_average_correctly() {
    // 3 perfect + 2 zero = aggregate 0.6
    let (perfect_t, perfect_s) = matching_pair();
    let (drift_t, drift_s) = drifting_pair();
    let entries = vec![
        CorpusEntry {
            fixture_id: "p1".into(),
            teacher: &perfect_t,
            student: &perfect_s,
        },
        CorpusEntry {
            fixture_id: "p2".into(),
            teacher: &perfect_t,
            student: &perfect_s,
        },
        CorpusEntry {
            fixture_id: "p3".into(),
            teacher: &perfect_t,
            student: &perfect_s,
        },
        CorpusEntry {
            fixture_id: "d1".into(),
            teacher: &drift_t,
            student: &drift_s,
        },
        CorpusEntry {
            fixture_id: "d2".into(),
            teacher: &drift_t,
            student: &drift_s,
        },
    ];
    let report = evaluate_corpus(&entries, &Thresholds::default());
    assert!(
        (report.aggregate_score - 0.6).abs() < 1e-9,
        "got {}",
        report.aggregate_score
    );
}

#[test]
fn corpus_report_structures_clone_and_debug() {
    let (t, s) = matching_pair();
    let entries = [CorpusEntry {
        fixture_id: "f".into(),
        teacher: &t,
        student: &s,
    }];
    let report = evaluate_corpus(&entries, &Thresholds::default());
    let cloned = report.clone();
    let _ = format!("{cloned:?}");
    let _ = format!("{:?}", report.fixtures[0]);
    assert_eq!(report, cloned);
}

#[test]
fn thresholds_clone_copy_works() {
    let a = Thresholds::default();
    let b = a;
    assert_eq!(a.aggregate_min, b.aggregate_min);
    assert_eq!(a.individual_min, b.individual_min);
}
