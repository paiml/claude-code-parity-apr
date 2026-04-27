//! Mock-replayer kernel — FALSIFY-CCPA-002 (`replay_determinism`) +
//! FALSIFY-CCPA-003 (`mock_completeness`).
//!
//! Ships:
//!   - [`LlmDriver`] trait — the orchestrator-facing surface a real
//!     `apr code` LLM client must implement (mirrors `aprender-orchestrate`'s
//!     `LlmDriver` once that goes pub via `PMAT-CODE-LLM-DRIVER-PUBLIC-001`)
//!   - [`RecordedDriver`] — replays a teacher trace's assistant turns
//!     verbatim, asserting M3's `mock_completeness` invariants
//!   - [`replay`] — deterministic orchestrator that walks the teacher
//!     trace and produces a student trace by calling the driver
//!
//! The `aprender-orchestrate` adapter (mapping aprender's real
//! `LlmDriver` to this trait) lands as a thin shim crate once
//! PMAT-CODE-LLM-DRIVER-PUBLIC-001 closes upstream. Until then this
//! crate is fully self-contained and its tests fix the algorithmic
//! contracts.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod driver;
mod orchestrator;

pub use driver::{LlmDriver, NextTurn, RecordedDriver, ReplayError};
pub use orchestrator::replay;
