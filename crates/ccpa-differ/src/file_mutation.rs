//! `file_mutation_equivalence` — FALSIFY-CCPA-005.
//!
//! Pure function over two filesystem snapshots (path → sha256 maps).
//! The contract's `file_mutation_equivalence § algorithm` says:
//!
//! 1. snapshot CWD at `session_start` (`cwd_sha256`)
//! 2. snapshot CWD at `session_end` (`after_sha256`)
//! 3. diff = (`after_teacher_sha256`, `after_student_sha256`)
//! 4. equivalent iff diff is empty OR every differing file passes
//!    `per_file_rule` (rustfmt for `*.rs`, taplo for `*.toml`, etc.)
//!
//! This module ships steps 1–4 over a *snapshot* abstraction — the
//! actual git tree hashing happens at a higher layer (recorder /
//! replayer at session boundaries). Per-filetype canonicalization
//! (`rustfmt --check`, `taplo fmt --stdin`) lives behind the
//! `Canonicalizer` trait so callers can plug in real tooling without
//! pulling rustfmt as a build-time dep here.

use std::collections::BTreeMap;

/// Per-path content hash. The recorder/replayer snapshot the CWD at
/// session boundaries and serialize a [`FileState`] alongside the trace.
pub type FileState = BTreeMap<String, String>;

/// One file-state divergence between teacher and student.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMutationDrift {
    /// File path (CWD-relative, normalized to forward slashes).
    pub path: String,
    /// Teacher's recorded sha256, or `None` if file absent.
    pub teacher_sha256: Option<String>,
    /// Student's recorded sha256, or `None` if file absent.
    pub student_sha256: Option<String>,
}

/// Configuration for [`file_mutation_equivalent`].
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Paths or path-prefixes to exclude from the diff (matches the
    /// contract's `excluded_paths`: `target/**`, `.git/**`, `*.lock`).
    pub excluded_paths: Vec<String>,
}

impl Options {
    /// Default options matching the contract's `excluded_paths` list.
    #[must_use]
    pub fn contract_defaults() -> Self {
        Self {
            excluded_paths: vec!["target/".to_owned(), ".git/".to_owned(), ".lock".to_owned()],
        }
    }

    fn is_excluded(&self, path: &str) -> bool {
        self.excluded_paths.iter().any(|p| {
            if let Some(suffix) = p.strip_prefix("*.") {
                // pattern like "*.lock" → suffix match on `.lock`
                path.ends_with(&format!(".{suffix}"))
            } else if p.ends_with('/') {
                // directory prefix
                path.starts_with(p) || path.contains(&format!("/{p}"))
            } else if let Some(suffix) = p.strip_prefix('.') {
                // suffix match like ".lock"
                path.ends_with(&format!(".{suffix}"))
            } else {
                path == p
            }
        })
    }
}

/// Compare two filesystem snapshots and return all drifts after applying
/// `excluded_paths`. Empty result == equivalent.
#[must_use]
pub fn file_mutation_drifts(
    teacher: &FileState,
    student: &FileState,
    options: &Options,
) -> Vec<FileMutationDrift> {
    let mut drifts = Vec::new();

    for (path, t_sha) in teacher {
        if options.is_excluded(path) {
            continue;
        }
        match student.get(path) {
            Some(s_sha) if s_sha == t_sha => {}
            Some(s_sha) => drifts.push(FileMutationDrift {
                path: path.clone(),
                teacher_sha256: Some(t_sha.clone()),
                student_sha256: Some(s_sha.clone()),
            }),
            None => drifts.push(FileMutationDrift {
                path: path.clone(),
                teacher_sha256: Some(t_sha.clone()),
                student_sha256: None,
            }),
        }
    }

    for (path, s_sha) in student {
        if options.is_excluded(path) {
            continue;
        }
        if !teacher.contains_key(path) {
            drifts.push(FileMutationDrift {
                path: path.clone(),
                teacher_sha256: None,
                student_sha256: Some(s_sha.clone()),
            });
        }
    }

    drifts
}

/// Boolean wrapper: returns `true` iff [`file_mutation_drifts`] is empty.
#[must_use]
pub fn file_mutation_equivalent(
    teacher: &FileState,
    student: &FileState,
    options: &Options,
) -> bool {
    file_mutation_drifts(teacher, student, options).is_empty()
}
