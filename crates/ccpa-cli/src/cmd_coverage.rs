//! `ccpa coverage` — corpus-coverage walk against required parity-matrix rows.
//!
//! Two modes:
//!
//! 1. **Manual** — caller supplies `--required <a,b,c>` and one
//!    `--fixture <id=row1,row2>` flag per fixture.
//! 2. **Auto** — caller supplies `--apr-code-parity-yaml <path>` (the
//!    contract enumerating SHIPPED+PARTIAL rows) and
//!    `--fixtures-dir <path>` (a directory of `<id>/meta.toml` files
//!    each declaring `covers = [...]`). The CLI reads both and runs
//!    the same [`ccpa_differ::corpus_coverage`] reduction.
//!
//! Both modes ship `FALSIFY-CCPA-007` as a CI step.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use ccpa_differ::{corpus_coverage, FixtureCoverage};
use clap::Parser;
use serde::Deserialize;
use thiserror::Error;

/// Failures during `ccpa coverage`.
#[derive(Debug, Error)]
pub enum CoverageError {
    /// `--required` resolved to an empty list after splitting on `,`.
    #[error("--required must be a comma-separated list of row ids; got empty string")]
    EmptyRequired,
    /// Reading a yaml/toml/dir entry failed.
    #[error("io reading {path}: {source}")]
    Io {
        /// Offending path.
        path: PathBuf,
        /// Wrapped IO error.
        #[source]
        source: std::io::Error,
    },
    /// Caller asked for auto-mode but provided neither a yaml + fixtures-dir
    /// pair nor required-flag pairs.
    #[error("either (--required + --fixture) OR (--apr-code-parity-yaml + --fixtures-dir) must be supplied")]
    MissingMode,
    /// `meta.toml` failed to deserialize.
    #[error("malformed meta.toml at {path}: {source}")]
    BadMeta {
        /// Offending path.
        path: PathBuf,
        /// TOML parse error.
        #[source]
        source: toml::de::Error,
    },
}

/// Args for `ccpa coverage`.
#[derive(Parser, Debug)]
pub struct Args {
    /// (Manual mode) Comma-separated list of required parity-matrix row ids.
    #[arg(long)]
    pub required: Option<String>,
    /// (Manual mode) Fixture metadata in the form `<id>=<row1,row2,...>`.
    #[arg(long = "fixture", value_parser = parse_fixture_arg)]
    pub fixtures: Vec<(String, Vec<String>)>,
    /// (Auto mode) Path to `apr-code-parity-v1.yaml` whose SHIPPED+PARTIAL
    /// rows are the required set.
    #[arg(long = "apr-code-parity-yaml")]
    pub apr_yaml: Option<PathBuf>,
    /// (Auto mode) Fixtures directory; each subdir's `meta.toml` declares
    /// the rows that fixture covers.
    #[arg(long = "fixtures-dir")]
    pub fixtures_dir: Option<PathBuf>,
    /// Comma-separated list of row ids classified as **out-of-scope** at
    /// the trace boundary (REPL render artifacts / keystroke events that
    /// never cross a trace event). These rows are excluded from
    /// `passes_gate`. Source-of-truth: contract `status_history` M15
    /// `remaining_uncovered_classification`.
    #[arg(long = "oos-rows")]
    pub oos_rows: Option<String>,
}

pub(crate) fn run(args: &Args) -> Result<ExitCode, CoverageError> {
    let (required, fixtures) =
        if let (Some(yaml), Some(dir)) = (args.apr_yaml.as_ref(), args.fixtures_dir.as_ref()) {
            let req = required_from_yaml(yaml)?;
            let fix = fixtures_from_dir(dir)?;
            (req, fix)
        } else if args.required.is_some() {
            let req = required_from_flag(args.required.as_deref().unwrap_or(""))?;
            let fix = args
                .fixtures
                .iter()
                .map(|(id, covers)| FixtureCoverage {
                    fixture_id: id.clone(),
                    covers: covers.clone(),
                })
                .collect();
            (req, fix)
        } else {
            return Err(CoverageError::MissingMode);
        };

    if required.is_empty() {
        return Err(CoverageError::EmptyRequired);
    }

    let oos: Vec<String> = args.oos_rows.as_deref().map(parse_csv).unwrap_or_default();

    let report = corpus_coverage(&fixtures, &required, &oos);
    let reachable = required.len().saturating_sub(report.oos.len());

    println!(
        "ccpa coverage: {}/{} reachable rows covered  ({} required total, {} OOS, {} uncovered)",
        report.covered.len(),
        reachable,
        required.len(),
        report.oos.len(),
        report.uncovered.len()
    );
    if !report.uncovered.is_empty() {
        println!("  uncovered:");
        for row in &report.uncovered {
            println!("    - {row}");
        }
    }
    if !report.oos.is_empty() {
        println!("  out-of-scope (excluded from gate):");
        for row in &report.oos {
            println!("    - {row}");
        }
    }

    if report.passes_gate {
        Ok(ExitCode::from(0))
    } else {
        Ok(ExitCode::from(1))
    }
}

fn parse_csv(s: &str) -> Vec<String> {
    s.split(',')
        .map(|x| x.trim().to_owned())
        .filter(|x| !x.is_empty())
        .collect()
}

fn required_from_flag(s: &str) -> Result<Vec<String>, CoverageError> {
    let req: Vec<String> = s
        .split(',')
        .map(|x| x.trim().to_owned())
        .filter(|x| !x.is_empty())
        .collect();
    if req.is_empty() {
        return Err(CoverageError::EmptyRequired);
    }
    Ok(req)
}

/// Parse `apr-code-parity-v1.yaml` and return the row ids whose status is
/// `SHIPPED` or `PARTIAL`. Uses a deliberately tiny line scanner instead
/// of pulling in a heavyweight YAML lib — the schema we care about is
/// strictly `categories: - id: <s>\n  ... status: <s>`.
fn required_from_yaml(path: &Path) -> Result<Vec<String>, CoverageError> {
    let body = match fs::read_to_string(path) {
        Ok(b) => b,
        Err(e) => {
            return Err(CoverageError::Io {
                path: path.to_path_buf(),
                source: e,
            })
        }
    };
    let mut required = Vec::new();
    let mut current_id: Option<String> = None;
    for raw_line in body.lines() {
        let line = raw_line.trim_start();
        if let Some(rest) = line.strip_prefix("- id: ") {
            current_id = Some(rest.trim().to_owned());
        } else if let Some(rest) = line.strip_prefix("status: ") {
            let status = rest.split_whitespace().next().unwrap_or("");
            if matches!(status, "SHIPPED" | "PARTIAL") {
                if let Some(ref id) = current_id {
                    required.push(id.clone());
                }
            }
            current_id = None;
        }
    }
    Ok(required)
}

#[derive(Deserialize)]
struct MetaToml {
    fixture: MetaFixture,
}

#[derive(Deserialize)]
struct MetaFixture {
    id: String,
    #[serde(default)]
    covers: Vec<String>,
}

fn fixtures_from_dir(dir: &Path) -> Result<Vec<FixtureCoverage>, CoverageError> {
    // Fold open + iter errors into one path (mirrors cmd_corpus.rs pattern
    // for the same coverage reason).
    let dir_entries: Vec<fs::DirEntry> =
        match fs::read_dir(dir).and_then(Iterator::collect::<Result<Vec<_>, _>>) {
            Ok(es) => es,
            Err(e) => {
                return Err(CoverageError::Io {
                    path: dir.to_path_buf(),
                    source: e,
                })
            }
        };
    let mut out = Vec::new();
    for entry in dir_entries {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let meta_path = path.join("meta.toml");
        if !meta_path.exists() {
            continue;
        }
        let text = match fs::read_to_string(&meta_path) {
            Ok(t) => t,
            Err(e) => {
                return Err(CoverageError::Io {
                    path: meta_path,
                    source: e,
                })
            }
        };
        let meta: MetaToml = match toml::from_str(&text) {
            Ok(m) => m,
            Err(e) => {
                return Err(CoverageError::BadMeta {
                    path: meta_path,
                    source: e,
                })
            }
        };
        out.push(FixtureCoverage {
            fixture_id: meta.fixture.id,
            covers: meta.fixture.covers,
        });
    }
    out.sort_by(|a, b| a.fixture_id.cmp(&b.fixture_id));
    Ok(out)
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
