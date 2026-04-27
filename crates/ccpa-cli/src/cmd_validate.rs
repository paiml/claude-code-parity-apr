//! `ccpa validate` — schema-roundtrip a `.ccpa-trace.jsonl` fixture.

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use ccpa_trace::Trace;
use clap::Parser;
use thiserror::Error;

/// Failures during `ccpa validate`.
#[derive(Debug, Error)]
pub enum ValidateError {
    /// Reading the trace file failed.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// JSON did not match the trace schema.
    #[error("schema: {0}")]
    Schema(#[from] serde_json::Error),
}

/// Args for `ccpa validate`.
#[derive(Parser, Debug)]
pub struct Args {
    /// Path to the `.ccpa-trace.jsonl` fixture.
    pub trace: PathBuf,
}

pub(crate) fn run(args: &Args) -> Result<ExitCode, ValidateError> {
    let body = fs::read_to_string(&args.trace)?;
    let trace = Trace::from_jsonl(&body)?;
    let serialized = trace.to_jsonl()?;
    let reparsed = Trace::from_jsonl(&serialized)?;
    if reparsed != trace {
        eprintln!("ccpa: schema-roundtrip drift in {}", args.trace.display());
        return Ok(ExitCode::from(1));
    }
    println!(
        "ccpa validate: {} record(s) — schema-roundtrip OK",
        trace.records.len()
    );
    Ok(ExitCode::from(0))
}
