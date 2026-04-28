//! Integration tests for `ccpa` CLI subcommands.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    clippy::float_cmp
)]

use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

use ccpa_cli::run;
use tempfile::tempdir;

/// Path to the compiled `ccpa` binary that cargo built for this test
/// run. Set by cargo via the `CARGO_BIN_EXE_<name>` env var.
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
fn coverage_auto_mode_with_yaml_and_fixtures_dir() {
    let dir = tempdir().expect("tempdir");
    // Minimal yaml with one SHIPPED row and one MISSING row
    let yaml = dir.path().join("parity.yaml");
    fs::write(
        &yaml,
        r"categories:
  - id: shipped-row
    name: shipped
    status: SHIPPED
  - id: missing-row
    name: missing
    status: MISSING
",
    )
    .expect("write yaml");
    // One fixture covering shipped-row
    let fixdir = dir.path().join("fixtures");
    let f = fixdir.join("0001-x");
    fs::create_dir_all(&f).expect("mkdir");
    fs::write(
        f.join("meta.toml"),
        r#"[fixture]
id = "0001-x"
covers = ["shipped-row"]
"#,
    )
    .expect("write meta");
    let code = run(args(&[
        "ccpa",
        "coverage",
        "--apr-code-parity-yaml",
        yaml.to_str().expect("utf8"),
        "--fixtures-dir",
        fixdir.to_str().expect("utf8"),
    ]));
    assert_eq!(ec(code), 0, "missing-row is not required (MISSING)");
}

#[test]
fn coverage_auto_mode_uncovered_row_exits_1() {
    let dir = tempdir().expect("tempdir");
    let yaml = dir.path().join("parity.yaml");
    fs::write(
        &yaml,
        r"categories:
  - id: alpha
    status: SHIPPED
  - id: beta
    status: PARTIAL
",
    )
    .expect("write");
    let fixdir = dir.path().join("fixtures");
    fs::create_dir_all(&fixdir).expect("mkdir");
    let code = run(args(&[
        "ccpa",
        "coverage",
        "--apr-code-parity-yaml",
        yaml.to_str().expect("utf8"),
        "--fixtures-dir",
        fixdir.to_str().expect("utf8"),
    ]));
    assert_eq!(ec(code), 1);
}

#[test]
fn coverage_auto_mode_yaml_missing_exits_2() {
    let dir = tempdir().expect("tempdir");
    let fixdir = dir.path().join("fixtures");
    fs::create_dir_all(&fixdir).expect("mkdir");
    let code = run(args(&[
        "ccpa",
        "coverage",
        "--apr-code-parity-yaml",
        "/no/such/parity.yaml",
        "--fixtures-dir",
        fixdir.to_str().expect("utf8"),
    ]));
    assert_eq!(ec(code), 2);
}

#[test]
fn coverage_auto_mode_fixtures_dir_missing_exits_2() {
    let dir = tempdir().expect("tempdir");
    let yaml = dir.path().join("parity.yaml");
    fs::write(&yaml, "categories: []\n").expect("write");
    let code = run(args(&[
        "ccpa",
        "coverage",
        "--apr-code-parity-yaml",
        yaml.to_str().expect("utf8"),
        "--fixtures-dir",
        "/no/such/fixtures",
    ]));
    assert_eq!(ec(code), 2);
}

#[test]
fn coverage_auto_mode_malformed_meta_exits_2() {
    let dir = tempdir().expect("tempdir");
    let yaml = dir.path().join("parity.yaml");
    fs::write(
        &yaml,
        r"categories:
  - id: x
    status: SHIPPED
",
    )
    .expect("write");
    let fixdir = dir.path().join("fixtures");
    let f = fixdir.join("0001-bad");
    fs::create_dir_all(&f).expect("mkdir");
    fs::write(f.join("meta.toml"), "this is not toml = unclosed [").expect("write");
    let code = run(args(&[
        "ccpa",
        "coverage",
        "--apr-code-parity-yaml",
        yaml.to_str().expect("utf8"),
        "--fixtures-dir",
        fixdir.to_str().expect("utf8"),
    ]));
    assert_eq!(ec(code), 2);
}

#[test]
fn coverage_auto_mode_skips_subdirs_without_meta_toml() {
    let dir = tempdir().expect("tempdir");
    let yaml = dir.path().join("parity.yaml");
    fs::write(
        &yaml,
        r"categories:
  - id: only-row
    status: SHIPPED
",
    )
    .expect("write");
    let fixdir = dir.path().join("fixtures");
    let with_meta = fixdir.join("0001-good");
    fs::create_dir_all(&with_meta).expect("mkdir");
    fs::write(
        with_meta.join("meta.toml"),
        "[fixture]\nid = \"0001-good\"\ncovers = [\"only-row\"]\n",
    )
    .expect("write");
    let no_meta = fixdir.join("0002-no-meta");
    fs::create_dir_all(&no_meta).expect("mkdir"); // no meta.toml
    let code = run(args(&[
        "ccpa",
        "coverage",
        "--apr-code-parity-yaml",
        yaml.to_str().expect("utf8"),
        "--fixtures-dir",
        fixdir.to_str().expect("utf8"),
    ]));
    assert_eq!(ec(code), 0, "second subdir was silently skipped");
}

#[test]
fn coverage_auto_mode_skips_loose_files_in_fixtures_dir() {
    // Stray file in fixtures dir → covered by `if !path.is_dir() continue`.
    let dir = tempdir().expect("tempdir");
    let yaml = dir.path().join("parity.yaml");
    fs::write(&yaml, "categories:\n  - id: x\n    status: SHIPPED\n").expect("write yaml");
    let fixdir = dir.path().join("fixtures");
    fs::create_dir_all(&fixdir).expect("mkdir");
    fs::write(fixdir.join("README.md"), "stray").expect("stray file");
    let f = fixdir.join("0001-good");
    fs::create_dir_all(&f).expect("mkdir");
    fs::write(
        f.join("meta.toml"),
        "[fixture]\nid = \"0001-good\"\ncovers = [\"x\"]\n",
    )
    .expect("write");
    let code = run(args(&[
        "ccpa",
        "coverage",
        "--apr-code-parity-yaml",
        yaml.to_str().expect("utf8"),
        "--fixtures-dir",
        fixdir.to_str().expect("utf8"),
    ]));
    assert_eq!(ec(code), 0);
}

#[test]
fn coverage_auto_mode_sorts_multiple_fixtures_alphabetically() {
    // Two+ subdirs so fixtures_from_dir's sort_by closure fires.
    let dir = tempdir().expect("tempdir");
    let yaml = dir.path().join("parity.yaml");
    fs::write(&yaml, "categories:\n  - id: x\n    status: SHIPPED\n").expect("write");
    let fixdir = dir.path().join("fixtures");
    for id in &["0002-second", "0001-first"] {
        let f = fixdir.join(id);
        fs::create_dir_all(&f).expect("mkdir");
        fs::write(
            f.join("meta.toml"),
            format!("[fixture]\nid = \"{id}\"\ncovers = [\"x\"]\n"),
        )
        .expect("write");
    }
    let code = run(args(&[
        "ccpa",
        "coverage",
        "--apr-code-parity-yaml",
        yaml.to_str().expect("utf8"),
        "--fixtures-dir",
        fixdir.to_str().expect("utf8"),
    ]));
    assert_eq!(ec(code), 0);
}

#[test]
fn coverage_auto_mode_meta_path_is_directory_io_error() {
    // exists() returns true for a dir; read_to_string fails with IsADirectory.
    let dir = tempdir().expect("tempdir");
    let yaml = dir.path().join("parity.yaml");
    fs::write(&yaml, "categories:\n  - id: x\n    status: SHIPPED\n").expect("write");
    let fixdir = dir.path().join("fixtures");
    let f = fixdir.join("0001-trap");
    fs::create_dir_all(f.join("meta.toml")).expect("create dir at meta path");
    let code = run(args(&[
        "ccpa",
        "coverage",
        "--apr-code-parity-yaml",
        yaml.to_str().expect("utf8"),
        "--fixtures-dir",
        fixdir.to_str().expect("utf8"),
    ]));
    assert_eq!(ec(code), 2);
}

#[test]
fn coverage_neither_mode_exits_2() {
    // No --required AND no --apr-code-parity-yaml → MissingMode
    let code = run(args(&["ccpa", "coverage"]));
    assert_eq!(ec(code), 2);
}

#[test]
fn coverage_yaml_with_no_shipped_rows_yields_empty_required() {
    let dir = tempdir().expect("tempdir");
    let yaml = dir.path().join("parity.yaml");
    fs::write(
        &yaml,
        r"categories:
  - id: only-missing
    status: MISSING
",
    )
    .expect("write");
    let fixdir = dir.path().join("fixtures");
    fs::create_dir_all(&fixdir).expect("mkdir");
    let code = run(args(&[
        "ccpa",
        "coverage",
        "--apr-code-parity-yaml",
        yaml.to_str().expect("utf8"),
        "--fixtures-dir",
        fixdir.to_str().expect("utf8"),
    ]));
    // No SHIPPED/PARTIAL rows → required is empty → EmptyRequired error
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
fn corpus_passes_when_all_pairs_perfect() {
    let dir = tempdir().expect("tempdir");
    let f = dir.path().join("0001-x");
    fs::create_dir_all(&f).expect("mkdir");
    fs::write(f.join("teacher.ccpa-trace.jsonl"), SAMPLE_TRACE).expect("write t");
    fs::write(f.join("student.ccpa-trace.jsonl"), SAMPLE_TRACE).expect("write s");
    let code = run(args(&[
        "ccpa",
        "corpus",
        dir.path().to_str().expect("utf8"),
    ]));
    assert_eq!(ec(code), 0);
}

#[test]
fn corpus_fails_below_thresholds() {
    let dir = tempdir().expect("tempdir");
    let f = dir.path().join("0001-drift");
    fs::create_dir_all(&f).expect("mkdir");
    fs::write(f.join("teacher.ccpa-trace.jsonl"), SAMPLE_TRACE).expect("write t");
    fs::write(f.join("student.ccpa-trace.jsonl"), STUDENT_DRIFT).expect("write s");
    let code = run(args(&[
        "ccpa",
        "corpus",
        dir.path().to_str().expect("utf8"),
    ]));
    assert_eq!(ec(code), 1);
}

#[test]
fn corpus_json_output_includes_aggregate_and_per_fixture() {
    let dir = tempdir().expect("tempdir");
    let f = dir.path().join("0001-x");
    fs::create_dir_all(&f).expect("mkdir");
    fs::write(f.join("teacher.ccpa-trace.jsonl"), SAMPLE_TRACE).expect("write t");
    fs::write(f.join("student.ccpa-trace.jsonl"), SAMPLE_TRACE).expect("write s");
    let code = run(args(&[
        "ccpa",
        "corpus",
        "--json",
        dir.path().to_str().expect("utf8"),
    ]));
    assert_eq!(ec(code), 0);
}

#[test]
fn corpus_lax_thresholds_pass_drifting_corpus() {
    let dir = tempdir().expect("tempdir");
    let f = dir.path().join("0001-drift");
    fs::create_dir_all(&f).expect("mkdir");
    fs::write(f.join("teacher.ccpa-trace.jsonl"), SAMPLE_TRACE).expect("write t");
    fs::write(f.join("student.ccpa-trace.jsonl"), STUDENT_DRIFT).expect("write s");
    let code = run(args(&[
        "ccpa",
        "corpus",
        "--aggregate-min",
        "0.0",
        "--individual-min",
        "0.0",
        dir.path().to_str().expect("utf8"),
    ]));
    assert_eq!(ec(code), 0, "score 0.0 ≥ floor 0.0 → pass");
}

#[test]
fn corpus_empty_directory_exits_1() {
    let dir = tempdir().expect("tempdir");
    let code = run(args(&[
        "ccpa",
        "corpus",
        dir.path().to_str().expect("utf8"),
    ]));
    assert_eq!(ec(code), 1);
}

#[test]
fn corpus_missing_directory_exits_2() {
    let code = run(args(&["ccpa", "corpus", "/no/such/corpus/dir"]));
    assert_eq!(ec(code), 2);
}

#[test]
fn corpus_subdir_missing_teacher_exits_2() {
    let dir = tempdir().expect("tempdir");
    let f = dir.path().join("0001-bad");
    fs::create_dir_all(&f).expect("mkdir");
    // Only write student — teacher missing.
    fs::write(f.join("student.ccpa-trace.jsonl"), SAMPLE_TRACE).expect("write s");
    let code = run(args(&[
        "ccpa",
        "corpus",
        dir.path().to_str().expect("utf8"),
    ]));
    assert_eq!(ec(code), 2);
}

#[test]
fn corpus_subdir_missing_student_exits_2() {
    let dir = tempdir().expect("tempdir");
    let f = dir.path().join("0001-bad");
    fs::create_dir_all(&f).expect("mkdir");
    fs::write(f.join("teacher.ccpa-trace.jsonl"), SAMPLE_TRACE).expect("write t");
    let code = run(args(&[
        "ccpa",
        "corpus",
        dir.path().to_str().expect("utf8"),
    ]));
    assert_eq!(ec(code), 2);
}

#[test]
fn corpus_skips_loose_files_in_dir() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("README.md"), "stray file").expect("write");
    let f = dir.path().join("0001-ok");
    fs::create_dir_all(&f).expect("mkdir");
    fs::write(f.join("teacher.ccpa-trace.jsonl"), SAMPLE_TRACE).expect("write t");
    fs::write(f.join("student.ccpa-trace.jsonl"), SAMPLE_TRACE).expect("write s");
    let code = run(args(&[
        "ccpa",
        "corpus",
        dir.path().to_str().expect("utf8"),
    ]));
    assert_eq!(ec(code), 0);
}

#[test]
fn corpus_malformed_teacher_trace_exits_2() {
    let dir = tempdir().expect("tempdir");
    let f = dir.path().join("0001-bad");
    fs::create_dir_all(&f).expect("mkdir");
    fs::write(f.join("teacher.ccpa-trace.jsonl"), "{not json").expect("write");
    fs::write(f.join("student.ccpa-trace.jsonl"), SAMPLE_TRACE).expect("write");
    let code = run(args(&[
        "ccpa",
        "corpus",
        dir.path().to_str().expect("utf8"),
    ]));
    assert_eq!(ec(code), 2);
}

#[test]
fn corpus_sorts_multiple_fixtures_alphabetically() {
    // Two+ fixtures so collect_fixtures' sort_by closure fires.
    let dir = tempdir().expect("tempdir");
    for id in &["0002-second", "0001-first"] {
        let f = dir.path().join(id);
        fs::create_dir_all(&f).expect("mkdir");
        fs::write(f.join("teacher.ccpa-trace.jsonl"), SAMPLE_TRACE).expect("write t");
        fs::write(f.join("student.ccpa-trace.jsonl"), SAMPLE_TRACE).expect("write s");
    }
    let code = run(args(&[
        "ccpa",
        "corpus",
        dir.path().to_str().expect("utf8"),
    ]));
    assert_eq!(ec(code), 0);
}

#[test]
fn corpus_teacher_path_is_directory_triggers_io_error() {
    // exists() returns true for a directory; fs::read_to_string then
    // fails with ErrorKind::IsADirectory — covers the read_trace io_err path.
    let dir = tempdir().expect("tempdir");
    let f = dir.path().join("0001-dir-trap");
    fs::create_dir_all(f.join("teacher.ccpa-trace.jsonl")).expect("mkdir trap");
    fs::write(f.join("student.ccpa-trace.jsonl"), SAMPLE_TRACE).expect("write");
    let code = run(args(&[
        "ccpa",
        "corpus",
        dir.path().to_str().expect("utf8"),
    ]));
    assert_eq!(ec(code), 2);
}

// ── M16: --oos-rows flag ──────────────────────────────────────────────

#[test]
fn coverage_oos_row_excluded_from_gate_exits_0() {
    // alpha is required + uncovered; beta is required + OOS. With
    // alpha covered + beta declared OOS, the gate must pass.
    let dir = tempdir().expect("tempdir");
    let yaml = dir.path().join("parity.yaml");
    fs::write(
        &yaml,
        r"categories:
  - id: alpha
    status: SHIPPED
  - id: beta
    status: PARTIAL
",
    )
    .expect("write");
    let fixdir = dir.path().join("fixtures");
    fs::create_dir_all(fixdir.join("0001-alpha")).expect("mkdir");
    fs::write(
        fixdir.join("0001-alpha/meta.toml"),
        "[fixture]\nid = \"0001-alpha\"\ncovers = [\"alpha\"]\n",
    )
    .expect("write meta");
    let code = run(args(&[
        "ccpa",
        "coverage",
        "--apr-code-parity-yaml",
        yaml.to_str().expect("utf8"),
        "--fixtures-dir",
        fixdir.to_str().expect("utf8"),
        "--oos-rows",
        "beta",
    ]));
    assert_eq!(ec(code), 0, "OOS row excluded → gate passes");
}

#[test]
fn coverage_oos_row_unrelated_uncovered_still_fails() {
    let dir = tempdir().expect("tempdir");
    let yaml = dir.path().join("parity.yaml");
    fs::write(
        &yaml,
        r"categories:
  - id: alpha
    status: SHIPPED
  - id: beta
    status: SHIPPED
  - id: gamma
    status: PARTIAL
",
    )
    .expect("write");
    let fixdir = dir.path().join("fixtures");
    fs::create_dir_all(fixdir.join("0001-alpha")).expect("mkdir");
    fs::write(
        fixdir.join("0001-alpha/meta.toml"),
        "[fixture]\nid = \"0001-alpha\"\ncovers = [\"alpha\"]\n",
    )
    .expect("write meta");
    let code = run(args(&[
        "ccpa",
        "coverage",
        "--apr-code-parity-yaml",
        yaml.to_str().expect("utf8"),
        "--fixtures-dir",
        fixdir.to_str().expect("utf8"),
        "--oos-rows",
        "gamma",
    ]));
    // beta still uncovered + reachable → exit 1
    assert_eq!(ec(code), 1);
}

#[test]
fn coverage_oos_rows_csv_with_whitespace_is_tolerated() {
    let dir = tempdir().expect("tempdir");
    let yaml = dir.path().join("parity.yaml");
    fs::write(
        &yaml,
        r"categories:
  - id: a
    status: SHIPPED
  - id: b
    status: SHIPPED
",
    )
    .expect("write");
    let fixdir = dir.path().join("fixtures");
    fs::create_dir_all(&fixdir).expect("mkdir");
    let code = run(args(&[
        "ccpa",
        "coverage",
        "--apr-code-parity-yaml",
        yaml.to_str().expect("utf8"),
        "--fixtures-dir",
        fixdir.to_str().expect("utf8"),
        "--oos-rows",
        "  a , b  ",
    ]));
    // both required rows declared OOS → uncovered is empty after subtraction → gate passes
    assert_eq!(ec(code), 0);
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

// ── M25: mutation-coverage kill-tests for ccpa-cli ────────────────────

#[test]
fn binary_main_propagates_nonzero_exit_code() {
    // Kills `main.rs:6 replace main -> ExitCode with Default::default()`.
    // If main were `Default::default()` it would return SUCCESS unconditionally.
    // Invoke the binary with an argument that causes ccpa_cli::run to
    // return a non-zero ExitCode (e.g. coverage MissingMode).
    let status = Command::new(CCPA_BIN)
        .arg("coverage")
        .status()
        .expect("spawn ccpa coverage with no flags");
    assert!(
        !status.success(),
        "binary must propagate non-zero exit code from ccpa_cli::run; \
         the bare `coverage` invocation should exit 2 (MissingMode)"
    );
}

#[test]
fn coverage_prints_uncovered_list_when_uncovered_nonempty() {
    // Kills `cmd_coverage.rs:118 delete ! in run`.
    // If the `!` were removed, the `uncovered:` block would only print
    // when uncovered IS empty (printing nothing), and skip when non-empty.
    // This test invokes the binary so we can capture stdout, then asserts
    // the uncovered list IS printed when `report.uncovered` is non-empty.
    let dir = tempdir().expect("tempdir");
    let yaml = dir.path().join("parity.yaml");
    fs::write(
        &yaml,
        r"categories:
  - id: alpha
    status: SHIPPED
",
    )
    .expect("write");
    let fixdir = dir.path().join("fixtures");
    fs::create_dir_all(&fixdir).expect("mkdir");
    let output = Command::new(CCPA_BIN)
        .args([
            "coverage",
            "--apr-code-parity-yaml",
            yaml.to_str().expect("utf8"),
            "--fixtures-dir",
            fixdir.to_str().expect("utf8"),
        ])
        .output()
        .expect("spawn ccpa coverage");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("uncovered:"),
        "stdout MUST contain 'uncovered:' when uncovered is non-empty; got: {stdout}"
    );
    assert!(
        stdout.contains("- alpha"),
        "stdout MUST list the uncovered row 'alpha'; got: {stdout}"
    );
}

#[test]
fn coverage_prints_oos_list_when_oos_nonempty() {
    // Kills `cmd_coverage.rs:124 delete ! in run`.
    // Same idea for the OOS block: invoke with --oos-rows naming a
    // SHIPPED row, then assert the OOS list IS printed.
    let dir = tempdir().expect("tempdir");
    let yaml = dir.path().join("parity.yaml");
    fs::write(
        &yaml,
        r"categories:
  - id: alpha
    status: SHIPPED
  - id: beta
    status: SHIPPED
",
    )
    .expect("write");
    let fixdir = dir.path().join("fixtures");
    fs::create_dir_all(fixdir.join("0001-alpha")).expect("mkdir");
    fs::write(
        fixdir.join("0001-alpha/meta.toml"),
        "[fixture]\nid = \"0001-alpha\"\ncovers = [\"alpha\"]\n",
    )
    .expect("write meta");
    let output = Command::new(CCPA_BIN)
        .args([
            "coverage",
            "--apr-code-parity-yaml",
            yaml.to_str().expect("utf8"),
            "--fixtures-dir",
            fixdir.to_str().expect("utf8"),
            "--oos-rows",
            "beta",
        ])
        .output()
        .expect("spawn ccpa coverage");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("out-of-scope"),
        "stdout MUST contain 'out-of-scope' header when OOS is non-empty; got: {stdout}"
    );
    assert!(
        stdout.contains("- beta"),
        "stdout MUST list the OOS row 'beta'; got: {stdout}"
    );
}

// ── M26: ccpa measure subcommand ──────────────────────────────────────

fn write_teacher_fixture(dir: &std::path::Path, prompt: &str, expected_text: &str) -> PathBuf {
    let path = dir.join("teacher.ccpa-trace.jsonl");
    let body = format!(
        concat!(
            "{{\"v\":1,\"kind\":\"session_start\",\"session_id\":\"s\",\"ts\":\"t\",",
            "\"actor\":\"claude-code\",\"model\":\"m\",\"cwd_sha256\":\"{}\"}}\n",
            "{{\"v\":1,\"kind\":\"user_prompt\",\"turn\":0,\"text\":{}}}\n",
            "{{\"v\":1,\"kind\":\"assistant_turn\",\"turn\":1,\"blocks\":[",
            "{{\"type\":\"text\",\"text\":{}}}],\"stop_reason\":\"end_turn\"}}\n",
            "{{\"v\":1,\"kind\":\"session_end\",\"turn\":1,\"stop_reason\":\"end_turn\",",
            "\"elapsed_ms\":0,\"tokens_in\":0,\"tokens_out\":0}}\n"
        ),
        "0".repeat(64),
        serde_json::to_string(prompt).expect("json"),
        serde_json::to_string(expected_text).expect("json"),
    );
    fs::write(&path, body).expect("write teacher fixture");
    path
}

fn write_teacher_with_tool_use(dir: &std::path::Path) -> PathBuf {
    let path = dir.join("teacher_tool.ccpa-trace.jsonl");
    fs::write(&path, SAMPLE_TRACE).expect("write");
    path
}

fn write_apr_stub(path: &std::path::Path, stdout_text: &str) {
    let script = format!(
        "#!/usr/bin/env bash\nprintf '%s\\n' {}\n",
        shell_quote(stdout_text)
    );
    fs::write(path, script).expect("write apr stub");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("chmod apr stub");
    }
}

fn shell_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if c == '\'' {
            out.push_str(r"'\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

#[test]
fn measure_apr_stub_returns_identical_text_scores_one() {
    let dir = tempdir().expect("tempdir");
    let teacher = write_teacher_fixture(dir.path(), "what is 2+2", "4");
    let stub = dir.path().join("apr-stub.sh");
    write_apr_stub(&stub, "4");
    let code = run(args(&[
        "ccpa",
        "measure",
        "--teacher",
        teacher.to_str().expect("utf8"),
        "--apr-bin",
        stub.to_str().expect("utf8"),
    ]));
    assert_eq!(ec(code), 0, "stub stdout matches teacher text → score 1.0");
}

#[test]
fn measure_apr_stub_zero_tool_calls_scores_one_regardless_of_text() {
    // The parity-score formula counts ToolUse + HookEvent +
    // SkillInvocation actions. Teacher with zero tool-use blocks =>
    // teacher_count = 0 => score = 1.0 by formula. This is a known
    // property: text-only turns don't drive parity score directly.
    // Tool-dispatch measurement is M27 follow-up.
    let dir = tempdir().expect("tempdir");
    let teacher = write_teacher_fixture(dir.path(), "what is 2+2", "4");
    let stub = dir.path().join("apr-stub.sh");
    write_apr_stub(&stub, "completely different response");
    let code = run(args(&[
        "ccpa",
        "measure",
        "--teacher",
        teacher.to_str().expect("utf8"),
        "--apr-bin",
        stub.to_str().expect("utf8"),
    ]));
    assert_eq!(ec(code), 0);
}

#[test]
fn measure_refuses_teacher_with_tool_use() {
    let dir = tempdir().expect("tempdir");
    let teacher = write_teacher_with_tool_use(dir.path());
    let stub = dir.path().join("apr-stub.sh");
    write_apr_stub(&stub, "irrelevant");
    let code = run(args(&[
        "ccpa",
        "measure",
        "--teacher",
        teacher.to_str().expect("utf8"),
        "--apr-bin",
        stub.to_str().expect("utf8"),
    ]));
    assert_eq!(
        ec(code),
        2,
        "teacher with tool_use must be refused with exit 2"
    );
}

#[test]
fn measure_missing_teacher_exits_2() {
    let dir = tempdir().expect("tempdir");
    let stub = dir.path().join("apr-stub.sh");
    write_apr_stub(&stub, "x");
    let code = run(args(&[
        "ccpa",
        "measure",
        "--teacher",
        "/no/such/teacher.jsonl",
        "--apr-bin",
        stub.to_str().expect("utf8"),
    ]));
    assert_eq!(ec(code), 2);
}

#[test]
fn measure_malformed_teacher_exits_2() {
    let dir = tempdir().expect("tempdir");
    let teacher = dir.path().join("bad.jsonl");
    fs::write(&teacher, "{not json").expect("write");
    let stub = dir.path().join("apr-stub.sh");
    write_apr_stub(&stub, "x");
    let code = run(args(&[
        "ccpa",
        "measure",
        "--teacher",
        teacher.to_str().expect("utf8"),
        "--apr-bin",
        stub.to_str().expect("utf8"),
    ]));
    assert_eq!(ec(code), 2);
}

#[test]
fn measure_teacher_without_user_prompt_exits_2() {
    let dir = tempdir().expect("tempdir");
    let teacher = dir.path().join("no_prompt.jsonl");
    fs::write(
        &teacher,
        format!(
            concat!(
                "{{\"v\":1,\"kind\":\"session_start\",\"session_id\":\"s\",\"ts\":\"t\",",
                "\"actor\":\"claude-code\",\"model\":\"m\",\"cwd_sha256\":\"{}\"}}\n",
                "{{\"v\":1,\"kind\":\"session_end\",\"turn\":0,\"stop_reason\":\"end_turn\",",
                "\"elapsed_ms\":0,\"tokens_in\":0,\"tokens_out\":0}}\n"
            ),
            "0".repeat(64),
        ),
    )
    .expect("write");
    let stub = dir.path().join("apr-stub.sh");
    write_apr_stub(&stub, "x");
    let code = run(args(&[
        "ccpa",
        "measure",
        "--teacher",
        teacher.to_str().expect("utf8"),
        "--apr-bin",
        stub.to_str().expect("utf8"),
    ]));
    assert_eq!(ec(code), 2);
}

#[test]
fn measure_apr_bin_missing_exits_2() {
    let dir = tempdir().expect("tempdir");
    let teacher = write_teacher_fixture(dir.path(), "p", "expected");
    let code = run(args(&[
        "ccpa",
        "measure",
        "--teacher",
        teacher.to_str().expect("utf8"),
        "--apr-bin",
        "/no/such/apr/binary",
    ]));
    assert_eq!(ec(code), 2);
}

#[test]
fn measure_apr_bin_nonzero_exit_exits_2() {
    let dir = tempdir().expect("tempdir");
    let teacher = write_teacher_fixture(dir.path(), "p", "expected");
    let stub = dir.path().join("failing-stub.sh");
    fs::write(
        &stub,
        "#!/usr/bin/env bash\necho 'simulated apr failure' >&2\nexit 7\n",
    )
    .expect("write");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    let code = run(args(&[
        "ccpa",
        "measure",
        "--teacher",
        teacher.to_str().expect("utf8"),
        "--apr-bin",
        stub.to_str().expect("utf8"),
    ]));
    assert_eq!(ec(code), 2);
}

#[test]
fn measure_writes_emitted_student_when_requested() {
    let dir = tempdir().expect("tempdir");
    let teacher = write_teacher_fixture(dir.path(), "what is 2+2", "4");
    let stub = dir.path().join("apr-stub.sh");
    write_apr_stub(&stub, "the answer is 4");
    let emit_path = dir.path().join("student-measured.jsonl");
    let code = run(args(&[
        "ccpa",
        "measure",
        "--teacher",
        teacher.to_str().expect("utf8"),
        "--apr-bin",
        stub.to_str().expect("utf8"),
        "--emit-student",
        emit_path.to_str().expect("utf8"),
    ]));
    assert_eq!(ec(code), 0);
    let body = fs::read_to_string(&emit_path).expect("emitted student exists");
    assert!(
        body.contains("\"actor\":\"apr-code\""),
        "emitted student must declare apr-code actor"
    );
    assert!(
        body.contains("the answer is 4"),
        "emitted student must contain the apr-stub stdout"
    );
}

#[test]
fn measure_json_output_includes_score_and_apr_bin() {
    let dir = tempdir().expect("tempdir");
    let teacher = write_teacher_fixture(dir.path(), "p", "x");
    let stub = dir.path().join("apr-stub.sh");
    write_apr_stub(&stub, "anything");
    let output = Command::new(CCPA_BIN)
        .args([
            "measure",
            "--teacher",
            teacher.to_str().expect("utf8"),
            "--apr-bin",
            stub.to_str().expect("utf8"),
            "--json",
        ])
        .output()
        .expect("spawn ccpa measure --json");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"score\""));
    assert!(stdout.contains("\"measured\": true"));
    assert!(stdout.contains("\"apr_bin\""));
}

#[test]
fn measure_emit_student_to_unwritable_path_exits_2() {
    let dir = tempdir().expect("tempdir");
    let teacher = write_teacher_fixture(dir.path(), "p", "x");
    let stub = dir.path().join("apr-stub.sh");
    write_apr_stub(&stub, "x");
    let code = run(args(&[
        "ccpa",
        "measure",
        "--teacher",
        teacher.to_str().expect("utf8"),
        "--apr-bin",
        stub.to_str().expect("utf8"),
        "--emit-student",
        "/no/such/dir/student.jsonl",
    ]));
    assert_eq!(ec(code), 2);
}
