//! FALSIFY-CCPA-007 — `corpus_coverage` (algorithm-level).

#![allow(clippy::expect_used, clippy::panic)]

use ccpa_differ::{corpus_coverage, FixtureCoverage};

fn fc(id: &str, covers: &[&str]) -> FixtureCoverage {
    FixtureCoverage {
        fixture_id: id.to_owned(),
        covers: covers.iter().map(|&s| s.to_owned()).collect(),
    }
}

#[test]
fn empty_corpus_against_empty_required_passes() {
    let report = corpus_coverage(&[], &[], &[]);
    assert!(report.passes_gate);
    assert!(report.covered.is_empty());
    assert!(report.uncovered.is_empty());
}

#[test]
fn empty_corpus_against_required_rows_fails_with_all_uncovered() {
    let required = vec!["hooks".into(), "skills".into()];
    let report = corpus_coverage(&[], &required, &[]);
    assert!(!report.passes_gate);
    assert_eq!(report.uncovered, required);
    assert!(report.covered.is_empty());
}

#[test]
fn single_fixture_covering_single_row_passes() {
    let fixtures = [fc("0001-hooks-test", &["hooks"])];
    let required = vec!["hooks".into()];
    let report = corpus_coverage(&fixtures, &required, &[]);
    assert!(report.passes_gate);
    assert_eq!(report.covered, vec!["hooks".to_owned()]);
}

#[test]
fn fixture_with_multiple_covers_satisfies_multiple_rows() {
    let fixtures = [fc("0001", &["hooks", "skills", "subagent-spawn"])];
    let required = vec!["hooks".into(), "skills".into()];
    let report = corpus_coverage(&fixtures, &required, &[]);
    assert!(report.passes_gate);
    assert_eq!(report.covered.len(), 2);
}

#[test]
fn missing_row_is_reported_in_uncovered() {
    let fixtures = [fc("0001", &["hooks"])];
    let required = vec!["hooks".into(), "skills".into()];
    let report = corpus_coverage(&fixtures, &required, &[]);
    assert!(!report.passes_gate);
    assert_eq!(report.uncovered, vec!["skills".to_owned()]);
    assert_eq!(report.covered, vec!["hooks".to_owned()]);
}

#[test]
fn duplicate_coverage_across_fixtures_is_idempotent() {
    let fixtures = [
        fc("0001", &["hooks"]),
        fc("0002", &["hooks"]),
        fc("0003", &["hooks"]),
    ];
    let required = vec!["hooks".into()];
    let report = corpus_coverage(&fixtures, &required, &[]);
    assert!(report.passes_gate);
    assert_eq!(report.covered.len(), 1);
}

#[test]
fn fixtures_can_cover_rows_not_required() {
    let fixtures = [fc("0001", &["hooks", "extra-row"])];
    let required = vec!["hooks".into()];
    let report = corpus_coverage(&fixtures, &required, &[]);
    assert!(report.passes_gate);
    // extra-row not in required → not in covered (we only report against required)
    assert_eq!(report.covered, vec!["hooks".to_owned()]);
}

#[test]
fn order_of_required_rows_is_preserved_in_report() {
    let fixtures = [fc("0001", &["b", "a", "c"])];
    let required: Vec<String> = vec!["a".into(), "b".into(), "c".into()];
    let report = corpus_coverage(&fixtures, &required, &[]);
    assert_eq!(report.covered, required);
}

#[test]
fn realistic_apr_code_parity_subset_passes() {
    let fixtures = [
        fc("0001-edit-readme", &["builtin-tools-edit"]),
        fc("0002-fix-test", &["builtin-tools-bash", "subagent-spawn"]),
        fc("0003-hooks-fire", &["hooks"]),
        fc("0004-skills", &["skills"]),
        fc("0005-mcp-roundtrip", &["mcp-client"]),
    ];
    let required: Vec<String> = vec![
        "builtin-tools-edit".into(),
        "builtin-tools-bash".into(),
        "subagent-spawn".into(),
        "hooks".into(),
        "skills".into(),
        "mcp-client".into(),
    ];
    let report = corpus_coverage(&fixtures, &required, &[]);
    assert!(report.passes_gate);
    assert_eq!(report.covered.len(), 6);
}

#[test]
fn report_struct_clone_eq_debug() {
    let r1 = corpus_coverage(&[fc("x", &["a"])], &["a".into()], &[]);
    let r2 = r1.clone();
    assert_eq!(r1, r2);
    let _ = format!("{r1:?}");
}

#[test]
fn fixture_coverage_struct_clone_eq_debug() {
    let f = fc("x", &["a", "b"]);
    let g = f.clone();
    assert_eq!(f, g);
    let _ = format!("{f:?}");
}

// ── M16: out-of-scope row exclusion ────────────────────────────────────

#[test]
fn oos_row_is_excluded_from_uncovered_and_gate_passes() {
    // status-line is required but classified as OOS at trace boundary;
    // gate must still pass when it's the only uncovered row.
    let fixtures = [fc("0001", &["builtin-tools-rwegs"])];
    let required = vec!["builtin-tools-rwegs".into(), "status-line".into()];
    let oos = vec!["status-line".into()];
    let report = corpus_coverage(&fixtures, &required, &oos);
    assert!(
        report.passes_gate,
        "gate must pass: only uncovered row is OOS"
    );
    assert_eq!(report.covered, vec!["builtin-tools-rwegs".to_owned()]);
    assert!(report.uncovered.is_empty());
    assert_eq!(report.oos, vec!["status-line".to_owned()]);
}

#[test]
fn oos_row_already_covered_is_still_classified_as_oos() {
    // If a fixture happens to declare it covers an OOS row, the row
    // is classified as OOS regardless. The OOS list is authoritative.
    let fixtures = [fc("0001", &["status-line"])];
    let required = vec!["status-line".into()];
    let oos = vec!["status-line".into()];
    let report = corpus_coverage(&fixtures, &required, &oos);
    assert!(report.passes_gate);
    assert!(
        report.covered.is_empty(),
        "OOS row should not appear in covered"
    );
    assert_eq!(report.oos, vec!["status-line".to_owned()]);
}

#[test]
fn oos_row_not_in_required_is_silently_ignored() {
    let fixtures = [fc("0001", &["a"])];
    let required: Vec<String> = vec!["a".into()];
    let oos: Vec<String> = vec!["not-in-required".into()];
    let report = corpus_coverage(&fixtures, &required, &oos);
    assert!(report.passes_gate);
    assert_eq!(report.covered, vec!["a".to_owned()]);
    assert!(report.oos.is_empty());
}

#[test]
fn uncovered_required_row_outside_oos_still_fails_gate() {
    let fixtures = [fc("0001", &["a"])];
    let required = vec!["a".into(), "b".into(), "status-line".into()];
    let oos = vec!["status-line".into()];
    let report = corpus_coverage(&fixtures, &required, &oos);
    assert!(!report.passes_gate, "gate must fail: b is uncovered");
    assert_eq!(report.uncovered, vec!["b".to_owned()]);
    assert_eq!(report.oos, vec!["status-line".to_owned()]);
}
