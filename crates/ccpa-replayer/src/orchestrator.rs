//! Deterministic orchestrator that walks a teacher trace and produces a
//! student trace by calling [`crate::LlmDriver`].
//!
//! This is the algorithmic core of `ccpa replay`. The IO-bearing wrapper
//! that drives `apr code` lands in a follow-up adapter crate.

use ccpa_trace::{Record, SCHEMA_VERSION};

use crate::driver::{LlmDriver, ReplayError};

/// Replay a teacher [`Record`] sequence against `driver`, producing a
/// student [`Record`] sequence one assistant turn at a time.
///
/// Algorithm:
///   - Walk teacher records in order.
///   - For every `Record::AssistantTurn`, call `driver.next_turn()`
///     and emit a fresh student `AssistantTurn` with the same `turn`
///     number.
///   - Pass other record kinds through unchanged (they are not
///     LLM-driven — `UserPrompt` comes from the user, `ToolResult`
///     comes from tool execution, `SessionStart`/`End` are bookkeeping).
///   - At end of teacher records, assert the driver has no unconsumed
///     turns (FALSIFY-CCPA-003 `mock_completeness` lower bound).
///
/// # Errors
///
/// - [`ReplayError::DriverExhausted`] — driver ran out of turns mid-replay
/// - [`ReplayError::DriverHasRemaining`] — driver had leftover turns at
///   end-of-teacher (orchestrator failed to consume them)
pub fn replay(teacher: &[Record], driver: &mut dyn LlmDriver) -> Result<Vec<Record>, ReplayError> {
    let mut student = Vec::with_capacity(teacher.len());

    for record in teacher {
        match record {
            Record::AssistantTurn { turn, .. } => {
                let next = driver.next_turn()?;
                student.push(Record::AssistantTurn {
                    v: SCHEMA_VERSION,
                    turn: *turn,
                    blocks: next.blocks,
                    stop_reason: next.stop_reason,
                });
            }
            other => student.push(other.clone()),
        }
    }

    let remaining = driver.remaining();
    if remaining > 0 {
        return Err(ReplayError::DriverHasRemaining { remaining });
    }

    Ok(student)
}
