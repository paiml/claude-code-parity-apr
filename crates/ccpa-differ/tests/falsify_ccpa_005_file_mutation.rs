//! FALSIFY-CCPA-005 — `file_mutation_equivalence`.
//!
//! Asserts the diff algorithm in
//! `contracts/claude-code-parity-apr-v1.yaml § file_mutation_equivalence`.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods // serde_json::json! expands to internal unwrap
)]

use std::collections::BTreeMap;

use ccpa_differ::{
    file_mutation_drifts, file_mutation_equivalent, FileMutationDrift, FileMutationOptions,
    FileState,
};

fn fs(entries: &[(&str, &str)]) -> FileState {
    let mut m = BTreeMap::new();
    for (k, v) in entries {
        m.insert((*k).to_owned(), (*v).to_owned());
    }
    m
}

#[test]
fn empty_snapshots_are_equivalent() {
    let opts = FileMutationOptions::default();
    assert!(file_mutation_equivalent(&fs(&[]), &fs(&[]), &opts));
}

#[test]
fn identical_snapshots_are_equivalent() {
    let opts = FileMutationOptions::default();
    let snap = fs(&[("a.rs", "deadbeef"), ("b.toml", "cafebabe")]);
    assert!(file_mutation_equivalent(&snap, &snap, &opts));
}

#[test]
fn divergent_hash_for_same_path_drifts() {
    let opts = FileMutationOptions::default();
    let teacher = fs(&[("README.md", "111")]);
    let student = fs(&[("README.md", "222")]);
    let drifts = file_mutation_drifts(&teacher, &student, &opts);
    assert_eq!(drifts.len(), 1);
    assert_eq!(
        drifts[0],
        FileMutationDrift {
            path: "README.md".to_owned(),
            teacher_sha256: Some("111".to_owned()),
            student_sha256: Some("222".to_owned()),
        }
    );
    assert!(!file_mutation_equivalent(&teacher, &student, &opts));
}

#[test]
fn file_only_in_teacher_drifts_with_student_none() {
    let opts = FileMutationOptions::default();
    let teacher = fs(&[("only_teacher.rs", "abc")]);
    let drifts = file_mutation_drifts(&teacher, &fs(&[]), &opts);
    assert_eq!(drifts.len(), 1);
    assert_eq!(drifts[0].teacher_sha256, Some("abc".to_owned()));
    assert_eq!(drifts[0].student_sha256, None);
}

#[test]
fn file_only_in_student_drifts_with_teacher_none() {
    let opts = FileMutationOptions::default();
    let student = fs(&[("only_student.rs", "xyz")]);
    let drifts = file_mutation_drifts(&fs(&[]), &student, &opts);
    assert_eq!(drifts.len(), 1);
    assert_eq!(drifts[0].teacher_sha256, None);
    assert_eq!(drifts[0].student_sha256, Some("xyz".to_owned()));
}

#[test]
fn excluded_target_dir_is_skipped() {
    let opts = FileMutationOptions::contract_defaults();
    let teacher = fs(&[("src/lib.rs", "111"), ("target/debug/build.rs", "999")]);
    let student = fs(&[
        ("src/lib.rs", "111"),
        ("target/debug/build.rs", "AAA"), // would diverge if not excluded
    ]);
    let drifts = file_mutation_drifts(&teacher, &student, &opts);
    assert!(drifts.is_empty(), "target/ excluded by contract default");
}

#[test]
fn excluded_git_dir_is_skipped() {
    let opts = FileMutationOptions::contract_defaults();
    let teacher = fs(&[(".git/HEAD", "ref: refs/heads/main")]);
    let student = fs(&[(".git/HEAD", "ref: refs/heads/other")]);
    assert!(file_mutation_equivalent(&teacher, &student, &opts));
}

#[test]
fn excluded_lock_extension_is_skipped() {
    let opts = FileMutationOptions::contract_defaults();
    let teacher = fs(&[("Cargo.lock", "111")]);
    let student = fs(&[("Cargo.lock", "222")]);
    assert!(file_mutation_equivalent(&teacher, &student, &opts));
}

#[test]
fn glob_lock_pattern_excludes_lock_files() {
    let opts = FileMutationOptions {
        excluded_paths: vec!["*.lock".to_owned()],
    };
    let teacher = fs(&[("foo.lock", "a")]);
    let student = fs(&[("foo.lock", "b")]);
    assert!(file_mutation_equivalent(&teacher, &student, &opts));
}

#[test]
fn exact_path_exclusion_matches_only_that_path() {
    let opts = FileMutationOptions {
        excluded_paths: vec!["secrets.toml".to_owned()],
    };
    let teacher = fs(&[("secrets.toml", "old"), ("config.toml", "111")]);
    let student = fs(&[("secrets.toml", "new"), ("config.toml", "222")]);
    let drifts = file_mutation_drifts(&teacher, &student, &opts);
    assert_eq!(drifts.len(), 1);
    assert_eq!(drifts[0].path, "config.toml");
}

#[test]
fn prefix_only_match_excludes_only_at_root_or_subpath() {
    let opts = FileMutationOptions {
        excluded_paths: vec!["build/".to_owned()],
    };
    let teacher = fs(&[
        ("build/x.o", "a"),
        ("crates/build/y.rs", "b"),
        ("src/main.rs", "c"),
    ]);
    let student = fs(&[
        ("build/x.o", "AAA"),         // excluded — leading prefix
        ("crates/build/y.rs", "BBB"), // excluded — `/build/` infix
        ("src/main.rs", "c"),         // identical
    ]);
    assert!(file_mutation_equivalent(&teacher, &student, &opts));
}

#[test]
fn options_default_excludes_nothing() {
    let opts = FileMutationOptions::default();
    let teacher = fs(&[("Cargo.lock", "a")]);
    let student = fs(&[("Cargo.lock", "b")]);
    assert!(!file_mutation_equivalent(&teacher, &student, &opts));
}

#[test]
fn multiple_drifts_in_single_diff() {
    let opts = FileMutationOptions::default();
    let teacher = fs(&[("src/a.rs", "a1"), ("src/b.rs", "b1"), ("docs/c.md", "c1")]);
    let student = fs(&[
        ("src/a.rs", "a1"), // match
        ("src/b.rs", "b2"), // drift
        ("src/d.rs", "d1"), // student-only
                            // docs/c.md missing from student
    ]);
    let drifts = file_mutation_drifts(&teacher, &student, &opts);
    assert_eq!(drifts.len(), 3);
    let paths: Vec<&str> = drifts.iter().map(|d| d.path.as_str()).collect();
    assert!(paths.contains(&"src/b.rs"));
    assert!(paths.contains(&"docs/c.md"));
    assert!(paths.contains(&"src/d.rs"));
}

#[test]
fn options_clone_works() {
    let a = FileMutationOptions::contract_defaults();
    let b = a.clone();
    assert_eq!(a.excluded_paths, b.excluded_paths);
}

#[test]
fn drift_record_supports_clone_eq_debug() {
    let d1 = FileMutationDrift {
        path: "x".into(),
        teacher_sha256: Some("a".into()),
        student_sha256: Some("b".into()),
    };
    let d2 = d1.clone();
    assert_eq!(d1, d2);
    let _ = format!("{d1:?}");
}
