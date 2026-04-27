//! Semantic diff primitives for teacher/student trace comparison.
//!
//! Ships the per-tool equivalence rules referenced by `pv`-validated
//! `contracts/claude-code-parity-apr-v1.yaml § tool_equivalence_rules`,
//! which `FALSIFY-CCPA-004` (`tool_call_equivalence`) gates against.
//!
//! Pure functions, no IO. Walks two `Vec<Record>` slices (teacher and
//! student) emitted by [`ccpa_trace`] and reports an enum-typed
//! [`DriftCategory`] for each mismatch.
//!
//! Higher-level traces-walk + parity-score reduction lands in a
//! follow-up PR; this crate is the equivalence-rule kernel.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod equivalence;

pub use equivalence::{tool_call_equivalent, DriftCategory, ToolCall};
