//! `ccpa corpus` — walk a directory of paired teacher/student fixtures
//! and aggregate parity scores per FALSIFY-CCPA-008 corpus rules.
//!
//! Layout convention:
//!
//! ```text
//! fixtures/canonical/
//!   0001-edit-readme/
//!     teacher.ccpa-trace.jsonl
//!     student.ccpa-trace.jsonl
//!   0002-fix-test/
//!     teacher.ccpa-trace.jsonl
//!     student.ccpa-trace.jsonl
//!   ...
//! ```
//!
//! Each subdirectory of `fixtures/canonical/` is one corpus entry.
//! The fixture id is the subdirectory name; the teacher and student
//! traces are the two `.ccpa-trace.jsonl` files inside.
//!
//! Discharges FALSIFY-CCPA-013 when piped into a `measured_parity`
//! status_history block.

#![allow(clippy::disallowed_methods)] // serde_json::json! expands to internal unwrap

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use ccpa_differ::{evaluate_corpus, CorpusEntry, Thresholds};
use ccpa_trace::{Record, Trace};
use clap::Parser;
use thiserror::Error;

/// Failures during `ccpa corpus`.
#[derive(Debug, Error)]
pub enum CorpusError {
    /// Reading the fixtures directory or its contents failed.
    #[error("io reading {path}: {source}")]
    Io {
        /// Path that triggered the IO error.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
    /// JSON in a trace file did not match schema.
    #[error("schema in {path}: {source}")]
    Schema {
        /// Path with the bad JSON.
        path: PathBuf,
        /// Underlying serde error.
        #[source]
        source: serde_json::Error,
    },
    /// Fixture directory layout violated convention.
    #[error("fixture {fixture}: {message}")]
    Layout {
        /// Fixture id.
        fixture: String,
        /// What was wrong.
        message: String,
    },
}

/// Args for `ccpa corpus`.
#[derive(Parser, Debug)]
pub struct Args {
    /// Directory containing one subdir per fixture (e.g. `fixtures/canonical/`).
    pub corpus_dir: PathBuf,
    /// Emit machine-readable JSON instead of human prose.
    #[arg(long)]
    pub json: bool,
    /// Aggregate floor (default 0.95 — matches contract).
    #[arg(long, default_value_t = 0.95)]
    pub aggregate_min: f64,
    /// Per-fixture floor (default 0.80 — matches contract).
    #[arg(long, default_value_t = 0.80)]
    pub individual_min: f64,
}

pub(crate) fn run(args: &Args) -> Result<ExitCode, CorpusError> {
    let entries = collect_fixtures(&args.corpus_dir)?;
    if entries.is_empty() {
        eprintln!(
            "ccpa corpus: no fixtures found under {}",
            args.corpus_dir.display()
        );
        return Ok(ExitCode::from(1));
    }

    // Read traces eagerly so we can hand &[Record] slices to the differ.
    let mut loaded: Vec<(String, Vec<Record>, Vec<Record>)> = Vec::with_capacity(entries.len());
    for (id, teacher_path, student_path) in entries {
        let teacher = read_trace(&teacher_path)?.records;
        let student = read_trace(&student_path)?.records;
        loaded.push((id, teacher, student));
    }

    let corpus_entries: Vec<CorpusEntry<'_>> = loaded
        .iter()
        .map(|(id, t, s)| CorpusEntry {
            fixture_id: id.clone(),
            teacher: t,
            student: s,
        })
        .collect();

    let thresholds = Thresholds {
        aggregate_min: args.aggregate_min,
        individual_min: args.individual_min,
    };
    let report = evaluate_corpus(&corpus_entries, &thresholds);

    if args.json {
        let per_fixture: Vec<serde_json::Value> = report
            .fixtures
            .iter()
            .map(|f| {
                serde_json::json!({
                    "id": f.fixture_id,
                    "score": f.parity.score,
                    "drift_count": f.parity.drifts.len(),
                    "passes_individual": f.passes_individual,
                })
            })
            .collect();
        let out = serde_json::json!({
            "aggregate_score": report.aggregate_score,
            "fixture_count": report.fixtures.len(),
            "per_fixture": per_fixture,
            "passes_gate": report.passes_gate,
            "thresholds": {
                "aggregate_min": thresholds.aggregate_min,
                "individual_min": thresholds.individual_min,
            },
        });
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
    } else {
        println!(
            "ccpa corpus: aggregate_score = {:.4}  ({} fixture(s), {} passes_gate)",
            report.aggregate_score,
            report.fixtures.len(),
            if report.passes_gate {
                "PASSES"
            } else {
                "FAILS"
            },
        );
        for f in &report.fixtures {
            let mark = if f.passes_individual { "✓" } else { "✗" };
            println!(
                "  {}  {:.4}  {}  ({} drift(s))",
                mark,
                f.parity.score,
                f.fixture_id,
                f.parity.drifts.len()
            );
        }
    }

    if report.passes_gate {
        Ok(ExitCode::from(0))
    } else {
        Ok(ExitCode::from(1))
    }
}

fn io_err(path: &Path, source: std::io::Error) -> CorpusError {
    CorpusError::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn schema_err(path: &Path, source: serde_json::Error) -> CorpusError {
    CorpusError::Schema {
        path: path.to_path_buf(),
        source,
    }
}

fn collect_fixtures(dir: &Path) -> Result<Vec<(String, PathBuf, PathBuf)>, CorpusError> {
    // Fold both initial-read_dir failure AND per-entry iteration failure
    // into a single Err path so coverage doesn't see two distinct
    // unreachable-in-tests closures.
    let dir_entries: Vec<fs::DirEntry> =
        match fs::read_dir(dir).and_then(Iterator::collect::<Result<Vec<_>, _>>) {
            Ok(es) => es,
            Err(e) => return Err(io_err(dir, e)),
        };
    let mut entries = Vec::new();
    for entry in dir_entries {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let id = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_owned)
            .unwrap_or_default();
        let teacher = path.join("teacher.ccpa-trace.jsonl");
        let student = path.join("student.ccpa-trace.jsonl");
        if !teacher.exists() {
            return Err(CorpusError::Layout {
                fixture: id,
                message: "missing teacher.ccpa-trace.jsonl".to_owned(),
            });
        }
        if !student.exists() {
            return Err(CorpusError::Layout {
                fixture: id,
                message: "missing student.ccpa-trace.jsonl".to_owned(),
            });
        }
        entries.push((id, teacher, student));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(entries)
}

fn read_trace(path: &Path) -> Result<Trace, CorpusError> {
    let body = match fs::read_to_string(path) {
        Ok(b) => b,
        Err(e) => return Err(io_err(path, e)),
    };
    match Trace::from_jsonl(&body) {
        Ok(t) => Ok(t),
        Err(e) => Err(schema_err(path, e)),
    }
}
