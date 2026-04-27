//! Corpus-level parity-score aggregation — FALSIFY-CCPA-008.
//!
//! Composes [`crate::compute_parity_score`] over a fixture corpus and
//! decides whether the aggregate clears the contract's bound:
//!
//! ```text
//! aggregate parity_score ≥ thresholds.aggregate_min   (default 0.95)
//! AND per-fixture       ≥ thresholds.individual_min  (default 0.80)
//! ```
//!
//! Pure function. Caller supplies pre-loaded fixture records (one
//! `(teacher_records, student_records)` pair per fixture); this module
//! folds them into a single [`CorpusReport`].

use ccpa_trace::Record;

use crate::score::{compute_parity_score, ParityReport};

/// One fixture's contribution to the corpus-level parity gate.
#[derive(Debug, Clone, PartialEq)]
pub struct FixtureReport {
    /// Caller-supplied fixture identifier (filename, sha256, etc).
    pub fixture_id: String,
    /// Per-trace parity report from [`compute_parity_score`].
    pub parity: ParityReport,
    /// Whether this fixture's score >= `thresholds.individual_min`.
    pub passes_individual: bool,
}

/// Aggregate report over the whole fixture corpus.
#[derive(Debug, Clone, PartialEq)]
pub struct CorpusReport {
    /// Per-fixture rollups, in input order.
    pub fixtures: Vec<FixtureReport>,
    /// Mean of per-fixture scores. Equals `1.0` for an empty corpus
    /// (vacuously true — no fixtures means no failures).
    pub aggregate_score: f64,
    /// Whether the corpus passes both gates: aggregate ≥
    /// `thresholds.aggregate_min` AND every fixture ≥
    /// `thresholds.individual_min`.
    pub passes_gate: bool,
}

/// Thresholds for the corpus-level gate. Defaults match the contract.
#[derive(Debug, Clone, Copy)]
pub struct Thresholds {
    /// Aggregate floor. Default 0.95.
    pub aggregate_min: f64,
    /// Per-fixture floor. Default 0.80.
    pub individual_min: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            aggregate_min: 0.95,
            individual_min: 0.80,
        }
    }
}

/// One corpus entry handed to [`evaluate_corpus`]. The borrowed
/// `[Record]` slices avoid forcing callers to clone whole traces.
pub struct CorpusEntry<'a> {
    /// Identifier displayed in the report.
    pub fixture_id: String,
    /// Teacher trace records.
    pub teacher: &'a [Record],
    /// Student trace records.
    pub student: &'a [Record],
}

/// Evaluate the whole corpus and decide whether it passes the
/// FALSIFY-CCPA-008 gate.
#[must_use]
pub fn evaluate_corpus(corpus: &[CorpusEntry<'_>], thresholds: &Thresholds) -> CorpusReport {
    if corpus.is_empty() {
        return CorpusReport {
            fixtures: Vec::new(),
            aggregate_score: 1.0,
            passes_gate: true,
        };
    }

    let mut fixtures = Vec::with_capacity(corpus.len());
    let mut sum: f64 = 0.0;
    let mut individual_floor_held = true;

    for entry in corpus {
        let parity = compute_parity_score(entry.teacher, entry.student);
        let passes_individual = parity.score >= thresholds.individual_min;
        if !passes_individual {
            individual_floor_held = false;
        }
        sum += parity.score;
        fixtures.push(FixtureReport {
            fixture_id: entry.fixture_id.clone(),
            parity,
            passes_individual,
        });
    }

    #[allow(clippy::cast_precision_loss)]
    let aggregate_score = sum / corpus.len() as f64;
    let passes_gate = aggregate_score >= thresholds.aggregate_min && individual_floor_held;

    CorpusReport {
        fixtures,
        aggregate_score,
        passes_gate,
    }
}
