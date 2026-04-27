//! `LlmDriver` trait + `RecordedDriver` impl.
//!
//! The trait deliberately mirrors the surface that
//! `aprender-orchestrate::agent::code::LlmDriver` will expose once
//! PMAT-CODE-LLM-DRIVER-PUBLIC-001 closes — i.e. one method that takes
//! a conversation tail and returns the assistant's next turn. The
//! mock-side implementation [`RecordedDriver`] feeds back the teacher's
//! pre-recorded turns one at a time; using it inside an `apr code`-like
//! orchestrator isolates *orchestration drift* from *model drift*,
//! which is the core of the parity harness.

use ccpa_trace::Block;
use thiserror::Error;

/// Errors the [`LlmDriver`] surface can return. Carried by
/// `Result<NextTurn, ReplayError>` so the orchestrator can distinguish
/// recoverable mock-completeness failures from runtime LLM errors.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ReplayError {
    /// Orchestrator asked the driver for an assistant turn but the
    /// recorded teacher trace is exhausted. Signals
    /// FALSIFY-CCPA-003 violation: the orchestrator made an
    /// extraneous LLM call vs the teacher's behaviour.
    #[error("RecordedDriver exhausted at turn {position} — teacher recorded {total} turns")]
    DriverExhausted {
        /// 0-based position of the offending call.
        position: usize,
        /// Total assistant turns the teacher recorded.
        total: usize,
    },
    /// Orchestrator finished but the driver still has unconsumed
    /// teacher turns. Signals FALSIFY-CCPA-003 violation: the
    /// orchestrator skipped one or more LLM calls.
    #[error("RecordedDriver finished with {remaining} unconsumed teacher turn(s)")]
    DriverHasRemaining {
        /// Count of unconsumed teacher turns at orchestrator exit.
        remaining: usize,
    },
}

/// One assistant turn returned by the driver: the content blocks plus a
/// stop reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NextTurn {
    /// Ordered content blocks (`text` / `thinking` / `tool_use`).
    pub blocks: Vec<Block>,
    /// Why the assistant stopped (passes through to the trace record).
    pub stop_reason: ccpa_trace::StopReason,
}

/// Surface the orchestrator calls during a replay. Mirrors the shape of
/// `aprender-orchestrate::agent::code::LlmDriver` once that goes
/// `pub` upstream.
pub trait LlmDriver {
    /// Hand the orchestrator the next assistant turn.
    ///
    /// # Errors
    ///
    /// Returns `ReplayError::DriverExhausted` for [`RecordedDriver`] if
    /// the recorded teacher trace has no more turns. Real production
    /// drivers would surface network / model errors here; the mock
    /// uses the same channel for a different failure mode by design.
    fn next_turn(&mut self) -> Result<NextTurn, ReplayError>;

    /// Number of teacher turns still available to play. Real drivers
    /// can return [`usize::MAX`]; the orchestrator only uses this for
    /// the FALSIFY-CCPA-003 completeness check at session end.
    fn remaining(&self) -> usize;
}

/// Replays a teacher trace's assistant turns verbatim. Constructed
/// from the `Vec<NextTurn>` extracted from a teacher
/// [`ccpa_trace::Trace`] at fixture-load time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedDriver {
    turns: Vec<NextTurn>,
    cursor: usize,
}

impl RecordedDriver {
    /// Wrap a recorded teacher turn list as a driver.
    #[must_use]
    pub fn new(turns: Vec<NextTurn>) -> Self {
        Self { turns, cursor: 0 }
    }
}

impl LlmDriver for RecordedDriver {
    fn next_turn(&mut self) -> Result<NextTurn, ReplayError> {
        match self.turns.get(self.cursor) {
            Some(turn) => {
                let out = turn.clone();
                self.cursor = self.cursor.saturating_add(1);
                Ok(out)
            }
            None => Err(ReplayError::DriverExhausted {
                position: self.cursor,
                total: self.turns.len(),
            }),
        }
    }

    fn remaining(&self) -> usize {
        self.turns.len().saturating_sub(self.cursor)
    }
}
