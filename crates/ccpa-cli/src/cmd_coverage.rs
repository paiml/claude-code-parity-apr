//! `ccpa coverage` — corpus-coverage walk against required parity-matrix rows.

use std::process::ExitCode;

use ccpa_differ::{corpus_coverage, FixtureCoverage};
use clap::Parser;
use thiserror::Error;

/// Failures during `ccpa coverage`.
#[derive(Debug, Error)]
pub enum CoverageError {
    /// `--required` resolved to an empty list after splitting on `,`.
    #[error("--required must be a comma-separated list of row ids; got empty string")]
    EmptyRequired,
}

/// Args for `ccpa coverage`.
#[derive(Parser, Debug)]
pub struct Args {
    /// Comma-separated list of required parity-matrix row ids
    /// (typically the SHIPPED ∪ PARTIAL rows of `apr-code-parity-v1.yaml`).
    #[arg(long, required = true)]
    pub required: String,
    /// Fixture metadata in the form `<id>=<row1,row2,...>` (repeatable).
    /// Each occurrence declares which rows that fixture covers.
    #[arg(long = "fixture", value_parser = parse_fixture_arg)]
    pub fixtures: Vec<(String, Vec<String>)>,
}

pub(crate) fn run(args: &Args) -> Result<ExitCode, CoverageError> {
    let required: Vec<String> = args
        .required
        .split(',')
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect();
    if required.is_empty() {
        return Err(CoverageError::EmptyRequired);
    }

    let fixtures: Vec<FixtureCoverage> = args
        .fixtures
        .iter()
        .map(|(id, covers)| FixtureCoverage {
            fixture_id: id.clone(),
            covers: covers.clone(),
        })
        .collect();

    let report = corpus_coverage(&fixtures, &required);

    println!(
        "ccpa coverage: {}/{} required rows covered  ({} uncovered)",
        report.covered.len(),
        required.len(),
        report.uncovered.len()
    );
    if !report.uncovered.is_empty() {
        println!("  uncovered:");
        for row in &report.uncovered {
            println!("    - {row}");
        }
    }

    if report.passes_gate {
        Ok(ExitCode::from(0))
    } else {
        Ok(ExitCode::from(1))
    }
}

fn parse_fixture_arg(s: &str) -> Result<(String, Vec<String>), String> {
    let (id, rows) = s
        .split_once('=')
        .ok_or_else(|| format!("expected `<id>=<row1,row2,...>`, got `{s}`"))?;
    if id.is_empty() {
        return Err("empty fixture id".to_owned());
    }
    let covers: Vec<String> = rows
        .split(',')
        .map(|r| r.trim().to_owned())
        .filter(|r| !r.is_empty())
        .collect();
    Ok((id.to_owned(), covers))
}
