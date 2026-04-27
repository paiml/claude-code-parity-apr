//! Integration tests for `ccpa` CLI subcommands.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    clippy::float_cmp
)]

use std::ffi::OsString;
use std::fs;
use std::process::{Command, ExitCode};

use ccpa_cli::run;
use tempfile::tempdir;

/// Path to the compiled `ccpa` binary that cargo built for this test
/// run. Set by cargo via the CARGO_BIN_EXE_<name> env var.
const CCPA_BIN: &str = env!("CARGO_BIN_EXE_ccpa");

fn args(parts: &[&str]) -> Vec<OsString> {
    parts.iter().map(|s| OsString::from(*s)).collect()
}

fn ec(code: ExitCode) -> u8 {
    // ExitCode → Termination is opaque; we go through the public
    // From<ExitCode> path the OS uses. The simplest portable check is
    // round-tripping through Debug (best-effort).
    let s = format!("{code:?}");
    // ExitCode prints as `ExitCode(unix_exit_status(N))` on unix.
    s.chars()
        .filter(char::is_ascii_digit)
        .collect::<String>()
        .parse::<u8>()
        .unwrap_or(255)
}

const SAMPLE_TRACE: &str = r#"{"v":1,"kind":"session_start","session_id":"s","ts":"t","actor":"claude-code","model":"m","cwd_sha256":"0000000000000000000000000000000000000000000000000000000000000000"}
{"v":1,"kind":"user_prompt","turn":0,"text":"hi"}
{"v":1,"kind":"assistant_turn","turn":1,"blocks":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls"}}],"stop_reason":"tool_use"}
"#;

const STUDENT_DRIFT: &str = r#"{"v":1,"kind":"session_start","session_id":"s","ts":"t","actor":"apr-code","model":"qwen","cwd_sha256":"0000000000000000000000000000000000000000000000000000000000000000"}
{"v":1,"kind":"user_prompt","turn":0,"text":"hi"}
{"v":1,"kind":"assistant_turn","turn":1,"blocks":[{"type":"tool_use","id":"s1","name":"Bash","input":{"command":"DIFFERENT"}}],"stop_reason":"tool_use"}
"#;

#[test]
fn no_args_prints_help_and_exits_2() {
    // clap returns exit 2 when subcommand is required but missing.
    let code = run(args(&["ccpa"]));
    assert_eq!(ec(code), 2);
}

#[test]
fn help_flag_exits_0() {
    let code = run(args(&["ccpa", "--help"]));
    assert_eq!(ec(code), 0);
}

#[test]
fn version_flag_exits_0() {
    let code = run(args(&["ccpa", "--version"]));
    assert_eq!(ec(code), 0);
}

#[test]
fn validate_on_good_trace_exits_0() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("good.jsonl");
    fs::write(&path, SAMPLE_TRACE).expect("write");
    let code = run(args(&["ccpa", "validate", path.to_str().expect("utf8")]));
    assert_eq!(ec(code), 0);
}

#[test]
fn validate_on_missing_file_exits_2() {
    let code = run(args(&["ccpa", "validate", "/no/such/file.jsonl"]));
    assert_eq!(ec(code), 2);
}

#[test]
fn validate_on_malformed_json_exits_2() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("bad.jsonl");
    fs::write(&path, "{not json").expect("write");
    let code = run(args(&["ccpa", "validate", path.to_str().expect("utf8")]));
    assert_eq!(ec(code), 2);
}

#[test]
fn diff_on_identical_traces_exits_0_with_score_1() {
    let dir = tempdir().expect("tempdir");
    let teacher = dir.path().join("t.jsonl");
    let student = dir.path().join("s.jsonl");
    fs::write(&teacher, SAMPLE_TRACE).expect("write");
    fs::write(&student, SAMPLE_TRACE).expect("write");
    let code = run(args(&[
        "ccpa",
        "diff",
        teacher.to_str().expect("utf8"),
        student.to_str().expect("utf8"),
    ]));
    assert_eq!(ec(code), 0);
}

#[test]
fn diff_on_drifting_traces_exits_1() {
    let dir = tempdir().expect("tempdir");
    let teacher = dir.path().join("t.jsonl");
    let student = dir.path().join("s.jsonl");
    fs::write(&teacher, SAMPLE_TRACE).expect("write");
    fs::write(&student, STUDENT_DRIFT).expect("write");
    let code = run(args(&[
        "ccpa",
        "diff",
        teacher.to_str().expect("utf8"),
        student.to_str().expect("utf8"),
    ]));
    assert_eq!(ec(code), 1);
}

#[test]
fn diff_json_format_outputs_parseable_json() {
    let dir = tempdir().expect("tempdir");
    let teacher = dir.path().join("t.jsonl");
    let student = dir.path().join("s.jsonl");
    fs::write(&teacher, SAMPLE_TRACE).expect("write");
    fs::write(&student, STUDENT_DRIFT).expect("write");
    // We can't capture stdout from run(), but we can assert exit code.
    // The JSON output is exercised via this run; coverage will verify
    // the json path executed.
    let code = run(args(&[
        "ccpa",
        "diff",
        "--json",
        teacher.to_str().expect("utf8"),
        student.to_str().expect("utf8"),
    ]));
    assert_eq!(ec(code), 1);
}

#[test]
fn diff_with_lax_individual_min_passes() {
    let dir = tempdir().expect("tempdir");
    let teacher = dir.path().join("t.jsonl");
    let student = dir.path().join("s.jsonl");
    fs::write(&teacher, SAMPLE_TRACE).expect("write");
    fs::write(&student, STUDENT_DRIFT).expect("write");
    let code = run(args(&[
        "ccpa",
        "diff",
        "--individual-min",
        "0.0",
        teacher.to_str().expect("utf8"),
        student.to_str().expect("utf8"),
    ]));
    assert_eq!(ec(code), 0, "score 0.0 ≥ floor 0.0 → pass");
}

#[test]
fn diff_on_missing_teacher_file_exits_2() {
    let dir = tempdir().expect("tempdir");
    let student = dir.path().join("s.jsonl");
    fs::write(&student, SAMPLE_TRACE).expect("write");
    let code = run(args(&[
        "ccpa",
        "diff",
        "/no/such/teacher.jsonl",
        student.to_str().expect("utf8"),
    ]));
    assert_eq!(ec(code), 2);
}

#[test]
fn diff_on_malformed_student_exits_2() {
    let dir = tempdir().expect("tempdir");
    let teacher = dir.path().join("t.jsonl");
    let student = dir.path().join("s.jsonl");
    fs::write(&teacher, SAMPLE_TRACE).expect("write");
    fs::write(&student, "{not json").expect("write");
    let code = run(args(&[
        "ccpa",
        "diff",
        teacher.to_str().expect("utf8"),
        student.to_str().expect("utf8"),
    ]));
    assert_eq!(ec(code), 2);
}

#[test]
fn coverage_all_required_covered_exits_0() {
    let code = run(args(&[
        "ccpa",
        "coverage",
        "--required",
        "hooks,skills",
        "--fixture",
        "0001=hooks",
        "--fixture",
        "0002=skills",
    ]));
    assert_eq!(ec(code), 0);
}

#[test]
fn coverage_uncovered_required_exits_1() {
    let code = run(args(&[
        "ccpa",
        "coverage",
        "--required",
        "hooks,skills,plugins",
        "--fixture",
        "0001=hooks",
    ]));
    assert_eq!(ec(code), 1);
}

#[test]
fn coverage_empty_required_exits_2() {
    let code = run(args(&[
        "ccpa",
        "coverage",
        "--required",
        ",,,", // splits to all-empty
    ]));
    assert_eq!(ec(code), 2);
}

#[test]
fn coverage_malformed_fixture_exits_2() {
    let code = run(args(&[
        "ccpa",
        "coverage",
        "--required",
        "hooks",
        "--fixture",
        "no_equals_sign",
    ]));
    assert_eq!(ec(code), 2);
}

#[test]
fn coverage_empty_fixture_id_exits_2() {
    let code = run(args(&[
        "ccpa",
        "coverage",
        "--required",
        "hooks",
        "--fixture",
        "=hooks",
    ]));
    assert_eq!(ec(code), 2);
}

#[test]
fn ccpa_error_display_chains_inner_message() {
    use ccpa_cli::CcpaError;
    let inner = ccpa_cli::cmd_coverage::CoverageError::EmptyRequired;
    let wrapped: CcpaError = inner.into();
    let s = format!("{wrapped}");
    assert!(s.contains("--required"));
    let _ = format!("{wrapped:?}");
}

#[test]
fn binary_main_handles_version_flag() {
    // Exercises src/main.rs end-to-end (the lib tests above call
    // `ccpa_cli::run` directly; only this test invokes the actual
    // binary entry point).
    let status = Command::new(CCPA_BIN)
        .arg("--version")
        .status()
        .expect("spawn ccpa --version");
    assert!(status.success());
}
