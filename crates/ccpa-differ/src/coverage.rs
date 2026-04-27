//! `corpus_coverage` — FALSIFY-CCPA-007 (algorithm-level).
//!
//! Asserts that each non-MISSING row of `apr-code-parity-v1.yaml` is
//! exercised by at least one fixture in the corpus. Pure function over
//! pre-loaded fixture metadata.
//!
//! Per the contract:
//!   "for every row in apr-code-parity-v1.yaml § categories[*] whose
//!    status is in {SHIPPED, PARTIAL}, at least one fixture in
//!    fixtures/ exercises that capability (fixture is annotated with
//!    `covers: [<row.id>, ...]` in a header comment, machine-readable).
//!    MISSING rows are exempt."
//!
//! M16 (Schema-v2 + reachable-only): some rows describe REPL render
//! artifacts or keystroke handling that never crosses the trace
//! boundary (e.g. `status-line`, `keyboard-shortcuts`). The contract
//! classifies these as **out-of-scope (OOS)** at the trace boundary;
//! the gate's hard-blocking form is "all REACHABLE required rows
//! covered" where REACHABLE = required \ oos.

use std::collections::BTreeSet;

/// Per-fixture metadata extracted from the fixture's `covers:` header
/// (or wherever the IO layer reads it from).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureCoverage {
    /// Stable fixture id (filename, sha, etc).
    pub fixture_id: String,
    /// Row ids from `apr-code-parity-v1.yaml` that this fixture covers.
    pub covers: Vec<String>,
}

/// Result of cross-checking a fixture corpus against required rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageReport {
    /// Required row ids that no fixture covers (excluding `oos` rows).
    pub uncovered: Vec<String>,
    /// Row ids that at least one fixture covers (in input order).
    pub covered: Vec<String>,
    /// Required row ids classified as out-of-scope at the trace boundary.
    /// These are excluded from `uncovered` and from `passes_gate`.
    pub oos: Vec<String>,
    /// Whether every REACHABLE required row is covered (i.e.
    /// `uncovered.is_empty()` after subtracting `oos`).
    pub passes_gate: bool,
}

/// Compare a fixture corpus against the set of required rows
/// (typically the `SHIPPED` ∪ `PARTIAL` row ids of
/// `apr-code-parity-v1.yaml`).
///
/// `oos_rows` is the contract-declared set of out-of-scope rows
/// (REPL render artifacts / keystroke events that never reach the
/// trace boundary). Rows listed there are excluded from the gate.
#[must_use]
pub fn corpus_coverage(
    fixtures: &[FixtureCoverage],
    required_rows: &[String],
    oos_rows: &[String],
) -> CoverageReport {
    let covered_set: BTreeSet<&str> = fixtures
        .iter()
        .flat_map(|f| f.covers.iter().map(String::as_str))
        .collect();
    let oos_set: BTreeSet<&str> = oos_rows.iter().map(String::as_str).collect();

    let mut covered = Vec::new();
    let mut uncovered = Vec::new();
    let mut oos = Vec::new();
    for row in required_rows {
        if oos_set.contains(row.as_str()) {
            oos.push(row.clone());
        } else if covered_set.contains(row.as_str()) {
            covered.push(row.clone());
        } else {
            uncovered.push(row.clone());
        }
    }

    CoverageReport {
        passes_gate: uncovered.is_empty(),
        covered,
        uncovered,
        oos,
    }
}
