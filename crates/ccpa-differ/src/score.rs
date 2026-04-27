//! Parity-score reduction across two `[ccpa_trace::Record]` slices.
//!
//! Implements the formula in `contracts/claude-code-parity-apr-v1.yaml
//! § parity_score`:
//!
//! ```text
//! score = matched_actions / total_teacher_actions
//! ```
//!
//! `matched_actions` counts position-aligned tool calls where the student
//! emitted an equivalent `(tool_name, semantic_input)` pair under
//! [`crate::tool_call_equivalent`]. Position is the dense index over all
//! `Block::ToolUse` blocks across all assistant turns.
//!
//! Higher-level concerns (`file_mutation_equivalence` per
//! `FALSIFY-CCPA-005`, full-corpus aggregation per
//! `FALSIFY-CCPA-008`) compose ON TOP of this per-trace primitive.

use ccpa_trace::{Block, Record};

use crate::equivalence::{tool_call_equivalent, DriftCategory, ToolCall};

/// One drift surfaced by the differ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drift {
    /// What kind of drift this is.
    pub category: DriftCategory,
    /// Dense tool-call index across all assistant turns where the drift
    /// was detected.
    pub position: usize,
    /// Tool name involved (teacher's name when missing/mismatched;
    /// student's name when extra).
    pub tool_name: String,
}

/// Result of comparing two traces.
#[derive(Debug, Clone, PartialEq)]
pub struct ParityReport {
    /// `matched / total` ∈ [0.0, 1.0]. Equals `1.0` when the teacher
    /// emits zero tool calls and the student emits zero too.
    pub score: f64,
    /// Number of position-aligned, equivalent tool calls.
    pub matched_count: usize,
    /// Total tool calls in the teacher trace (the denominator).
    pub teacher_count: usize,
    /// Total tool calls in the student trace.
    pub student_count: usize,
    /// All drifts surfaced, in order of detection.
    pub drifts: Vec<Drift>,
}

/// Compute the parity score for two traces.
#[must_use]
pub fn compute_parity_score(teacher: &[Record], student: &[Record]) -> ParityReport {
    let teacher_calls = extract_tool_calls(teacher);
    let student_calls = extract_tool_calls(student);
    let teacher_count = teacher_calls.len();
    let student_count = student_calls.len();

    let mut drifts = Vec::new();
    let mut matched: usize = 0;

    for (i, teacher_call) in teacher_calls.iter().enumerate() {
        match student_calls.get(i) {
            None => drifts.push(Drift {
                category: DriftCategory::MissingToolCall,
                position: i,
                tool_name: teacher_call.name.clone(),
            }),
            Some(student_call) => match tool_call_equivalent(teacher_call, student_call) {
                Ok(()) => matched = matched.saturating_add(1),
                Err(category) => drifts.push(Drift {
                    category,
                    position: i,
                    tool_name: teacher_call.name.clone(),
                }),
            },
        }
    }

    for (i, student_call) in student_calls.iter().enumerate().skip(teacher_count) {
        drifts.push(Drift {
            category: DriftCategory::ExtraToolCall,
            position: i,
            tool_name: student_call.name.clone(),
        });
    }

    #[allow(clippy::cast_precision_loss)]
    let score = if teacher_count == 0 {
        if student_count == 0 {
            1.0
        } else {
            0.0
        }
    } else {
        matched as f64 / teacher_count as f64
    };

    ParityReport {
        score,
        matched_count: matched,
        teacher_count,
        student_count,
        drifts,
    }
}

fn extract_tool_calls(records: &[Record]) -> Vec<ToolCall> {
    let mut out = Vec::new();
    for record in records {
        if let Record::AssistantTurn { blocks, .. } = record {
            for block in blocks {
                if let Block::ToolUse { name, input, .. } = block {
                    out.push(ToolCall {
                        name: name.clone(),
                        input: input.clone(),
                    });
                }
            }
        }
    }
    out
}
