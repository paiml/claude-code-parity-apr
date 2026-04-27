//! Parity-score reduction across two `[ccpa_trace::Record]` slices.
//!
//! Implements the formula in `contracts/claude-code-parity-apr-v1.yaml
//! § parity_score`:
//!
//! ```text
//! score = matched_actions / total_teacher_actions
//! ```
//!
//! `matched_actions` counts position-aligned **actions** where the
//! student emitted an equivalent observation. An *action* is any of:
//!   - a [`ccpa_trace::Block::ToolUse`] inside an `AssistantTurn`
//!   - a [`ccpa_trace::Record::HookEvent`] (Schema-v2, M15)
//!   - a [`ccpa_trace::Record::SkillInvocation`] (Schema-v2, M15)
//!
//! Each action kind is compared via its dedicated equivalence rule
//! ([`tool_call_equivalent`], [`hook_event_equivalent`],
//! [`skill_invocation_equivalent`]). Position alignment is the dense
//! index over the union of action kinds in trace order — a teacher
//! `[Tool, Hook, Tool]` paired with a student `[Tool, Tool, Hook]`
//! drifts at positions 1+2 (kind mismatch).
//!
//! Higher-level concerns (`file_mutation_equivalence` per
//! `FALSIFY-CCPA-005`, full-corpus aggregation per
//! `FALSIFY-CCPA-008`) compose ON TOP of this per-trace primitive.

use ccpa_trace::{Block, Record};

use crate::equivalence::{
    hook_event_equivalent, skill_invocation_equivalent, tool_call_equivalent, DriftCategory,
    HookProjection, SkillProjection, ToolCall,
};

/// One drift surfaced by the differ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drift {
    /// What kind of drift this is.
    pub category: DriftCategory,
    /// Dense action index across all assistant turns + hook + skill
    /// records where the drift was detected.
    pub position: usize,
    /// Tool name involved (teacher's name when missing/mismatched;
    /// student's name when extra). For hook drifts this is the canonical
    /// hook event name (e.g. `"PreToolUse"`); for skill drifts it's the
    /// skill name.
    pub tool_name: String,
}

/// Result of comparing two traces.
#[derive(Debug, Clone, PartialEq)]
pub struct ParityReport {
    /// `matched / total` ∈ [0.0, 1.0]. Equals `1.0` when the teacher
    /// emits zero actions and the student emits zero too.
    pub score: f64,
    /// Number of position-aligned, equivalent actions.
    pub matched_count: usize,
    /// Total actions in the teacher trace (the denominator).
    pub teacher_count: usize,
    /// Total actions in the student trace.
    pub student_count: usize,
    /// All drifts surfaced, in order of detection.
    pub drifts: Vec<Drift>,
}

/// Internal — typed projection of one action observation extracted from
/// a trace. Keeps tool/hook/skill projections aligned in a single
/// position-indexed sequence so the meter can dispatch cleanly.
#[derive(Debug, Clone)]
enum Action {
    Tool(ToolCall),
    Hook(HookProjection),
    Skill(SkillProjection),
}

impl Action {
    fn label(&self) -> String {
        match self {
            Self::Tool(t) => t.name.clone(),
            Self::Hook(h) => h.event.clone(),
            Self::Skill(s) => s.name.clone(),
        }
    }
}

/// Compute the parity score for two traces.
#[must_use]
pub fn compute_parity_score(teacher: &[Record], student: &[Record]) -> ParityReport {
    let teacher_actions = extract_actions(teacher);
    let student_actions = extract_actions(student);
    let teacher_count = teacher_actions.len();
    let student_count = student_actions.len();

    let mut drifts = Vec::new();
    let mut matched: usize = 0;

    for (i, t_action) in teacher_actions.iter().enumerate() {
        match student_actions.get(i) {
            None => drifts.push(Drift {
                category: missing_for(t_action),
                position: i,
                tool_name: t_action.label(),
            }),
            Some(s_action) => match action_equivalent(t_action, s_action) {
                Ok(()) => matched = matched.saturating_add(1),
                Err(category) => drifts.push(Drift {
                    category,
                    position: i,
                    tool_name: t_action.label(),
                }),
            },
        }
    }

    for (i, s_action) in student_actions.iter().enumerate().skip(teacher_count) {
        drifts.push(Drift {
            category: extra_for(s_action),
            position: i,
            tool_name: s_action.label(),
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

fn action_equivalent(a: &Action, b: &Action) -> Result<(), DriftCategory> {
    match (a, b) {
        (Action::Tool(a), Action::Tool(b)) => tool_call_equivalent(a, b),
        (Action::Hook(a), Action::Hook(b)) => hook_event_equivalent(a, b),
        (Action::Skill(a), Action::Skill(b)) => skill_invocation_equivalent(a, b),
        _ => Err(DriftCategory::MismatchedActionKind),
    }
}

fn missing_for(a: &Action) -> DriftCategory {
    match a {
        Action::Tool(_) => DriftCategory::MissingToolCall,
        Action::Hook(_) => DriftCategory::MissingHookEvent,
        Action::Skill(_) => DriftCategory::MissingSkillInvocation,
    }
}

fn extra_for(a: &Action) -> DriftCategory {
    match a {
        Action::Tool(_) => DriftCategory::ExtraToolCall,
        Action::Hook(_) => DriftCategory::ExtraHookEvent,
        Action::Skill(_) => DriftCategory::ExtraSkillInvocation,
    }
}

fn extract_actions(records: &[Record]) -> Vec<Action> {
    let mut out = Vec::new();
    for record in records {
        match record {
            Record::AssistantTurn { blocks, .. } => {
                for block in blocks {
                    if let Block::ToolUse { name, input, .. } = block {
                        out.push(Action::Tool(ToolCall {
                            name: name.clone(),
                            input: input.clone(),
                        }));
                    }
                }
            }
            Record::HookEvent {
                event,
                matcher,
                decision,
                exit_code,
                output,
                ..
            } => {
                out.push(Action::Hook(HookProjection {
                    event: event.clone(),
                    matcher: matcher.clone(),
                    decision: *decision,
                    exit_code: *exit_code,
                    output: output.clone(),
                }));
            }
            Record::SkillInvocation {
                name,
                source,
                instructions_injected,
                ..
            } => {
                out.push(Action::Skill(SkillProjection {
                    name: name.clone(),
                    source: *source,
                    instructions_injected: *instructions_injected,
                }));
            }
            _ => {}
        }
    }
    out
}
