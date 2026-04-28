//! `ccpa measure` — bridge AUTHORED canonical fixtures into MEASURED ones.
//!
//! Spec problem: every canonical fixture is paired teacher + student
//! BOTH AUTHORED. Score 1.0 over identical inputs is tautological — it
//! proves the meter doesn't false-positive, not that real `apr code`
//! actually matches Claude Code.
//!
//! M26 closes the gap for the zero-tool-call subset of the corpus
//! (text-only assistant turns). The flow:
//!
//! 1. Load the teacher fixture (AUTHORED canonical).
//! 2. Extract the user prompt from `Record::UserPrompt`.
//! 3. Refuse the run if the teacher contains any `Block::ToolUse` —
//!    apr-code's stdout doesn't faithfully serialize tool dispatch
//!    today, so we'd be mis-measuring. Tool-dispatch measurement waits
//!    on `apr code --emit-trace` (M27 follow-up).
//! 4. Spawn `<apr-bin> code -p '<prompt>'` and capture stdout.
//! 5. Build a synthetic student trace: `SessionStart` + `UserPrompt` +
//!    `AssistantTurn` (one `Block::Text` block carrying stdout) +
//!    `SessionEnd`.
//! 6. Run `compute_parity_score(&teacher, &student)` and print a
//!    drift report.
//!
//! Exit codes mirror `ccpa diff`:
//!   0 — measured score ≥ `--individual-min` (default 0.80)
//!   1 — measured score < `--individual-min`
//!   2 — usage / IO / spawn failure / teacher contains `tool_use`

#![allow(clippy::disallowed_methods)] // serde_json::json! expands to internal unwrap

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use ccpa_differ::compute_parity_score;
use ccpa_trace::{Actor, Block, Record, StopReason, Trace};
use clap::Parser;
use thiserror::Error;

/// Failures during `ccpa measure`.
#[derive(Debug, Error)]
pub enum MeasureError {
    /// Reading the teacher fixture failed.
    #[error("io reading teacher {path}: {source}")]
    Io {
        /// Path that failed to read.
        path: PathBuf,
        /// Underlying IO error.
        #[source]
        source: std::io::Error,
    },
    /// Teacher trace did not parse against the schema.
    #[error("schema in teacher {path}: {source}")]
    Schema {
        /// Path with the bad JSON.
        path: PathBuf,
        /// Underlying serde error.
        #[source]
        source: serde_json::Error,
    },
    /// Teacher had no `UserPrompt` record at turn 0.
    #[error("teacher {path} has no user_prompt at turn 0; cannot extract prompt")]
    NoPrompt {
        /// Offending teacher path.
        path: PathBuf,
    },
    /// Teacher contains tool dispatch — out of scope for stdout-based measurement.
    #[error("teacher {path} contains tool_use blocks; M26 measure path is text-only — author --emit-trace support in apr-cli (M27) before measuring tool-dispatch fixtures")]
    HasToolUse {
        /// Offending teacher path.
        path: PathBuf,
    },
    /// Spawning the apr binary failed (binary missing, exec error, etc).
    #[error("spawn {bin}: {source}")]
    Spawn {
        /// The binary path that failed to spawn.
        bin: String,
        /// Underlying IO error.
        #[source]
        source: std::io::Error,
    },
    /// The apr binary exited non-zero before producing usable output.
    #[error("{bin} exited with non-zero status {status}: {stderr}")]
    AprFailed {
        /// The binary path that exited.
        bin: String,
        /// Exit status (as printed; may include signal info).
        status: String,
        /// Captured stderr (truncated for display).
        stderr: String,
    },
}

/// Args for `ccpa measure`.
#[derive(Parser, Debug)]
pub struct Args {
    /// Teacher (canonical) trace whose `user_prompt` will drive the apr-code run.
    #[arg(long)]
    pub teacher: PathBuf,
    /// Path to the apr binary. Defaults to `apr` on PATH.
    #[arg(long, default_value = "apr")]
    pub apr_bin: String,
    /// Apr-code `--max-turns` (passed through verbatim).
    #[arg(long, default_value_t = 1)]
    pub max_turns: u32,
    /// Per-fixture floor (default 0.80; mirrors `ccpa diff`).
    #[arg(long, default_value_t = 0.80)]
    pub individual_min: f64,
    /// Optional: write the synthesized student trace to this path
    /// alongside printing the report (so it can be inspected later).
    #[arg(long)]
    pub emit_student: Option<PathBuf>,
    /// Emit machine-readable JSON instead of human prose.
    #[arg(long)]
    pub json: bool,
}

pub(crate) fn run(args: &Args) -> Result<ExitCode, MeasureError> {
    let teacher = read_teacher(&args.teacher)?;
    refuse_if_tool_use(&teacher, &args.teacher)?;
    let prompt = extract_prompt(&teacher, &args.teacher)?;

    let stdout = run_apr_code(&args.apr_bin, &prompt, args.max_turns)?;

    let student = build_student_trace(&teacher, &prompt, &stdout);

    if let Some(path) = args.emit_student.as_ref() {
        let body = student.to_jsonl().unwrap_or_default();
        fs::write(path, &body).map_err(|e| MeasureError::Io {
            path: path.clone(),
            source: e,
        })?;
    }

    let report = compute_parity_score(&teacher.records, &student.records);
    print_report(
        &report,
        &args.apr_bin,
        &prompt,
        args.json,
        args.individual_min,
    );
    Ok(exit_for(&report, args.individual_min))
}

/// Print the parity report in either human-readable or JSON form.
/// Factored out of `run` so the formatting branch is unit-testable
/// without spawning a subprocess.
fn print_report(
    report: &ccpa_differ::ParityReport,
    apr_bin: &str,
    prompt: &str,
    json: bool,
    individual_min: f64,
) {
    if json {
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
            "passes_individual_floor": report.score >= individual_min,
            "apr_bin": apr_bin,
            "prompt": prompt,
            "measured": true,
        });
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
    } else {
        println!(
            "ccpa measure: parity_score = {:.4}  ({}/{} matched, measured against {})",
            report.score, report.matched_count, report.teacher_count, apr_bin
        );
        for d in &report.drifts {
            println!(
                "  drift @ pos {}  {:?}  {}",
                d.position, d.category, d.tool_name
            );
        }
    }
}

/// Decide the process exit code given a parity report and the
/// individual-minimum floor. Unit-testable independent of spawning.
fn exit_for(report: &ccpa_differ::ParityReport, individual_min: f64) -> ExitCode {
    if report.score >= individual_min {
        ExitCode::from(0)
    } else {
        ExitCode::from(1)
    }
}

fn read_teacher(path: &Path) -> Result<Trace, MeasureError> {
    let body = fs::read_to_string(path).map_err(|e| MeasureError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    Trace::from_jsonl(&body).map_err(|e| MeasureError::Schema {
        path: path.to_path_buf(),
        source: e,
    })
}

fn refuse_if_tool_use(trace: &Trace, path: &Path) -> Result<(), MeasureError> {
    for record in &trace.records {
        if let Record::AssistantTurn { blocks, .. } = record {
            if blocks.iter().any(|b| matches!(b, Block::ToolUse { .. })) {
                return Err(MeasureError::HasToolUse {
                    path: path.to_path_buf(),
                });
            }
        }
    }
    Ok(())
}

fn extract_prompt(trace: &Trace, path: &Path) -> Result<String, MeasureError> {
    for record in &trace.records {
        if let Record::UserPrompt { turn: 0, text, .. } = record {
            return Ok(text.clone());
        }
    }
    Err(MeasureError::NoPrompt {
        path: path.to_path_buf(),
    })
}

fn run_apr_code(bin: &str, prompt: &str, max_turns: u32) -> Result<String, MeasureError> {
    let max_turns_str = max_turns.to_string();
    let output = Command::new(bin)
        .args(["code", "-p", prompt, "--max-turns", &max_turns_str])
        .output()
        .map_err(|e| MeasureError::Spawn {
            bin: bin.to_owned(),
            source: e,
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let truncated: String = stderr.chars().take(400).collect();
        return Err(MeasureError::AprFailed {
            bin: bin.to_owned(),
            status: format!("{:?}", output.status),
            stderr: truncated,
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn build_student_trace(teacher: &Trace, prompt: &str, stdout: &str) -> Trace {
    let session_id = teacher
        .records
        .iter()
        .find_map(|r| match r {
            Record::SessionStart { session_id, .. } => Some(session_id.clone()),
            _ => None,
        })
        .unwrap_or_else(|| "00000000-0000-0000-0000-000000000000".to_owned());
    let cwd_sha256 = teacher
        .records
        .iter()
        .find_map(|r| match r {
            Record::SessionStart { cwd_sha256, .. } => Some(cwd_sha256.clone()),
            _ => None,
        })
        .unwrap_or_else(|| "0".repeat(64));
    let ts = teacher
        .records
        .iter()
        .find_map(|r| match r {
            Record::SessionStart { ts, .. } => Some(ts.clone()),
            _ => None,
        })
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_owned());

    Trace {
        records: vec![
            Record::SessionStart {
                v: 1,
                session_id,
                ts,
                actor: Actor::AprCode,
                model: "apr-code-measured".to_owned(),
                cwd_sha256,
            },
            Record::UserPrompt {
                v: 1,
                turn: 0,
                text: prompt.to_owned(),
            },
            Record::AssistantTurn {
                v: 1,
                turn: 1,
                blocks: vec![Block::Text {
                    text: stdout.trim().to_owned(),
                }],
                stop_reason: StopReason::EndTurn,
            },
            Record::SessionEnd {
                v: 1,
                turn: 1,
                stop_reason: StopReason::EndTurn,
                elapsed_ms: 0,
                tokens_in: 0,
                tokens_out: 0,
            },
        ],
    }
}

#[cfg(test)]
#[allow(clippy::panic)] // tests panic on schema-shape mismatch — expected
mod tests {
    //! Unit tests for the unit-testable helpers in this module
    //! (the integration-test layer for the full subprocess path
    //! lives in `tests/cli.rs`).
    use super::*;
    use ccpa_differ::{Drift, DriftCategory, ParityReport};

    #[test]
    fn build_student_trace_uses_teacher_session_metadata_when_present() {
        let teacher = Trace {
            records: vec![Record::SessionStart {
                v: 1,
                session_id: "real-id".to_owned(),
                ts: "2026-01-02T03:04:05Z".to_owned(),
                actor: Actor::ClaudeCode,
                model: "claude".to_owned(),
                cwd_sha256: "a".repeat(64),
            }],
        };
        let student = build_student_trace(&teacher, "p", "r");
        match &student.records[0] {
            Record::SessionStart {
                session_id,
                ts,
                cwd_sha256,
                ..
            } => {
                assert_eq!(session_id, "real-id");
                assert_eq!(ts, "2026-01-02T03:04:05Z");
                assert_eq!(cwd_sha256, &"a".repeat(64));
            }
            _ => panic!("expected SessionStart"),
        }
    }

    #[test]
    fn build_student_trace_falls_back_when_teacher_has_no_session_start() {
        // Teacher with NO SessionStart — exercises the unwrap_or_else
        // fallback paths for session_id / cwd_sha256 / ts.
        let teacher = Trace {
            records: vec![Record::UserPrompt {
                v: 1,
                turn: 0,
                text: "p".to_owned(),
            }],
        };
        let student = build_student_trace(&teacher, "p", "r");
        match &student.records[0] {
            Record::SessionStart {
                session_id,
                ts,
                cwd_sha256,
                ..
            } => {
                assert_eq!(session_id, "00000000-0000-0000-0000-000000000000");
                assert_eq!(ts, "1970-01-01T00:00:00Z");
                assert_eq!(cwd_sha256, &"0".repeat(64));
            }
            _ => panic!("expected SessionStart"),
        }
    }

    #[test]
    fn build_student_trace_trims_stdout_whitespace() {
        let teacher = Trace { records: vec![] };
        let student = build_student_trace(&teacher, "p", "  hello\n\n");
        match &student.records[2] {
            Record::AssistantTurn { blocks, .. } => match &blocks[0] {
                Block::Text { text } => assert_eq!(text, "hello"),
                _ => panic!("expected Text"),
            },
            _ => panic!("expected AssistantTurn"),
        }
    }

    fn synth_report(score: f64, drifts: Vec<Drift>) -> ParityReport {
        ParityReport {
            score,
            matched_count: usize::from(score >= 1.0),
            teacher_count: 1,
            student_count: 1,
            drifts,
        }
    }

    #[test]
    fn exit_for_above_floor_is_zero() {
        let r = synth_report(0.95, vec![]);
        let code = exit_for(&r, 0.80);
        assert_eq!(format!("{code:?}"), format!("{:?}", ExitCode::from(0)));
    }

    #[test]
    fn exit_for_below_floor_is_one() {
        let r = synth_report(0.50, vec![]);
        let code = exit_for(&r, 0.80);
        assert_eq!(format!("{code:?}"), format!("{:?}", ExitCode::from(1)));
    }

    #[test]
    fn exit_for_at_floor_is_zero_inclusive() {
        let r = synth_report(0.80, vec![]);
        let code = exit_for(&r, 0.80);
        assert_eq!(format!("{code:?}"), format!("{:?}", ExitCode::from(0)));
    }

    #[test]
    fn print_report_human_with_drifts_does_not_panic() {
        let r = synth_report(
            0.50,
            vec![Drift {
                category: DriftCategory::MismatchedToolInput,
                position: 0,
                tool_name: "Bash".to_owned(),
            }],
        );
        // smoke-test: just ensure no panic / no error path in the
        // formatting branch.
        print_report(&r, "/bin/apr", "what?", false, 0.80);
    }

    #[test]
    fn print_report_json_with_drifts_does_not_panic() {
        let r = synth_report(
            0.50,
            vec![Drift {
                category: DriftCategory::MissingToolCall,
                position: 1,
                tool_name: "Edit".to_owned(),
            }],
        );
        print_report(&r, "/bin/apr", "what?", true, 0.80);
    }
}
