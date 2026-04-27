//! `ccpa` CLI library — separated from `main.rs` so the entry-point logic
//! is testable without spawning the binary.
//!
//! Subcommands:
//!   - `validate <trace.jsonl>` — schema-roundtrip a fixture
//!   - `diff <teacher.jsonl> <student.jsonl>` — parity-score two traces
//!   - `coverage --required <ids> <fixtures-dir>` — corpus-coverage walk
//!
//! Every subcommand is a thin wrapper over the four `ccpa-*` library
//! crates. Exit codes:
//!   - 0 — gate passed
//!   - 1 — gate failed (drift / missing coverage / schema error)
//!   - 2 — usage / IO error

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// `ccpa coverage` subcommand.
pub mod cmd_coverage;
/// `ccpa diff` subcommand.
pub mod cmd_diff;
/// `ccpa validate` subcommand.
pub mod cmd_validate;

use std::ffi::OsString;
use std::fmt;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// Unified error type so subcommand dispatch can return a single
/// `Result<ExitCode, CcpaError>`. Each variant wraps the per-subcommand
/// error.
#[derive(Debug)]
pub enum CcpaError {
    /// `validate` failure.
    Validate(cmd_validate::ValidateError),
    /// `diff` failure.
    Diff(cmd_diff::DiffError),
    /// `coverage` failure.
    Coverage(cmd_coverage::CoverageError),
}

impl fmt::Display for CcpaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validate(e) => write!(f, "{e}"),
            Self::Diff(e) => write!(f, "{e}"),
            Self::Coverage(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for CcpaError {}

impl From<cmd_validate::ValidateError> for CcpaError {
    fn from(e: cmd_validate::ValidateError) -> Self {
        Self::Validate(e)
    }
}
impl From<cmd_diff::DiffError> for CcpaError {
    fn from(e: cmd_diff::DiffError) -> Self {
        Self::Diff(e)
    }
}
impl From<cmd_coverage::CoverageError> for CcpaError {
    fn from(e: cmd_coverage::CoverageError) -> Self {
        Self::Coverage(e)
    }
}

/// Parse + dispatch the CLI args. Returns the process exit code.
pub fn run<I, T>(args: I) -> ExitCode
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = match Cli::try_parse_from(args) {
        Ok(c) => c,
        Err(e) => {
            // clap prints help/version to stdout (exit 0) and errors to
            // stderr (exit 2). Mirror that behaviour.
            let kind = e.kind();
            let _ = e.print();
            return match kind {
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion => {
                    ExitCode::from(0)
                }
                _ => ExitCode::from(2),
            };
        }
    };

    let result: Result<ExitCode, CcpaError> = match cli.command {
        Command::Validate(args) => cmd_validate::run(&args).map_err(Into::into),
        Command::Diff(args) => cmd_diff::run(&args).map_err(Into::into),
        Command::Coverage(args) => cmd_coverage::run(&args).map_err(Into::into),
    };

    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("ccpa: {e}");
            ExitCode::from(2)
        }
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "ccpa",
    version,
    about = "claude-code-parity-apr CLI — measure Claude Code ↔ apr code drift",
    long_about = "Pure CLI wrapper over the four ccpa-* libraries. \
                  Reads .ccpa-trace.jsonl files, applies the contract's \
                  per-tool equivalence rules, emits parity scores."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Validate a `.ccpa-trace.jsonl` against the schema (FALSIFY-CCPA-001).
    Validate(cmd_validate::Args),
    /// Diff a teacher trace against a student trace and print the parity score (FALSIFY-CCPA-008).
    Diff(cmd_diff::Args),
    /// Cross-check fixture corpus against required parity-matrix rows (FALSIFY-CCPA-007).
    Coverage(cmd_coverage::Args),
}
