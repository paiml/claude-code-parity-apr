//! `ccpa diff` — parity-score two traces.

#![allow(clippy::disallowed_methods)] // serde_json::json! expands to internal unwrap

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use ccpa_differ::compute_parity_score;
use ccpa_trace::Trace;
use clap::Parser;
use thiserror::Error;

/// Failures during `ccpa diff`.
#[derive(Debug, Error)]
pub enum DiffError {
    /// Reading either teacher or student file failed.
    #[error("io reading {path}: {source}")]
    Io {
        /// Path that failed to read.
        path: PathBuf,
        /// Underlying IO error.
        #[source]
        source: std::io::Error,
    },
    /// JSON in one of the trace files did not match schema.
    #[error("schema in {path}: {source}")]
    Schema {
        /// Path with the bad JSON.
        path: PathBuf,
        /// Underlying serde error.
        #[source]
        source: serde_json::Error,
    },
}

/// Args for `ccpa diff`.
#[derive(Parser, Debug)]
pub struct Args {
    /// Teacher (recorded) trace.
    pub teacher: PathBuf,
    /// Student (replayed) trace.
    pub student: PathBuf,
    /// Emit machine-readable JSON instead of human prose.
    #[arg(long)]
    pub json: bool,
    /// Per-fixture floor (default 0.80).
    #[arg(long, default_value_t = 0.80)]
    pub individual_min: f64,
}

pub(crate) fn run(args: &Args) -> Result<ExitCode, DiffError> {
    let teacher = read_trace(&args.teacher)?;
    let student = read_trace(&args.student)?;
    let report = compute_parity_score(&teacher.records, &student.records);

    if args.json {
        // Hand-roll a small JSON shape rather than serde-derive the
        // ParityReport (avoids leaking internal field names that could
        // change). Stable surface for piping into pv.
        let drifts: Vec<serde_json::Value> = report
            .drifts
            .iter()
            .map(|d| {
                serde_json::json!({
                    "category": format!("{:?}", d.category),
                    "position": d.position,
                    "tool_name": d.tool_name,
                })
            })
            .collect();
        let out = serde_json::json!({
            "score": report.score,
            "matched_count": report.matched_count,
            "teacher_count": report.teacher_count,
            "student_count": report.student_count,
            "drifts": drifts,
            "passes_individual_floor": report.score >= args.individual_min,
        });
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
    } else {
        println!(
            "ccpa diff: parity_score = {:.4}  ({}/{} matched, {} student calls, {} drift(s))",
            report.score,
            report.matched_count,
            report.teacher_count,
            report.student_count,
            report.drifts.len()
        );
        for d in &report.drifts {
            println!(
                "  drift @ pos {}  {:?}  {}",
                d.position, d.category, d.tool_name
            );
        }
    }

    if report.score >= args.individual_min {
        Ok(ExitCode::from(0))
    } else {
        Ok(ExitCode::from(1))
    }
}

fn read_trace(path: &PathBuf) -> Result<Trace, DiffError> {
    let body = fs::read_to_string(path).map_err(|e| DiffError::Io {
        path: path.clone(),
        source: e,
    })?;
    Trace::from_jsonl(&body).map_err(|e| DiffError::Schema {
        path: path.clone(),
        source: e,
    })
}
