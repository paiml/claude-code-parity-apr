//! FALSIFY-CCPA-004 — `tool_call_equivalence`.
//!
//! Asserts the per-tool equivalence rules listed under
//! `contracts/claude-code-parity-apr-v1.yaml § tool_equivalence_rules`.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods // serde_json::json! expands to internal unwrap
)]

use ccpa_differ::{
    hook_event_equivalent, skill_invocation_equivalent, tool_call_equivalent, DriftCategory,
    HookProjection, SkillProjection, ToolCall,
};
use ccpa_trace::{HookDecision, SkillSource};
use serde_json::json;

fn tc(name: &str, input: serde_json::Value) -> ToolCall {
    ToolCall {
        name: name.to_owned(),
        input,
    }
}

#[test]
fn mismatched_tool_name_is_reported() {
    let a = tc("Bash", json!({ "command": "ls" }));
    let b = tc("Read", json!({ "path": "x" }));
    assert_eq!(
        tool_call_equivalent(&a, &b),
        Err(DriftCategory::MismatchedToolName)
    );
}

#[test]
fn bash_equal_commands_are_equivalent() {
    let a = tc("Bash", json!({ "command": "ls" }));
    let b = tc("Bash", json!({ "command": "ls" }));
    assert_eq!(tool_call_equivalent(&a, &b), Ok(()));
}

#[test]
fn bash_normalizes_whitespace_and_trailing_semicolon() {
    let a = tc("Bash", json!({ "command": "  ls   -la  ;  " }));
    let b = tc("Bash", json!({ "command": "ls -la" }));
    assert_eq!(tool_call_equivalent(&a, &b), Ok(()));
}

#[test]
fn bash_different_commands_are_not_equivalent() {
    let a = tc("Bash", json!({ "command": "ls" }));
    let b = tc("Bash", json!({ "command": "cat" }));
    assert_eq!(
        tool_call_equivalent(&a, &b),
        Err(DriftCategory::MismatchedToolInput)
    );
}

#[test]
fn read_same_path_offset_limit_is_equivalent() {
    let a = tc("Read", json!({ "path": "/x", "offset": 10, "limit": 100 }));
    let b = tc("Read", json!({ "path": "/x", "offset": 10, "limit": 100 }));
    assert_eq!(tool_call_equivalent(&a, &b), Ok(()));
}

#[test]
fn read_path_only_treated_as_full_file() {
    let a = tc("Read", json!({ "path": "/x" }));
    let b = tc("Read", json!({ "path": "/x" }));
    assert_eq!(tool_call_equivalent(&a, &b), Ok(()));
}

#[test]
fn read_different_path_is_not_equivalent() {
    let a = tc("Read", json!({ "path": "/x" }));
    let b = tc("Read", json!({ "path": "/y" }));
    assert_eq!(
        tool_call_equivalent(&a, &b),
        Err(DriftCategory::MismatchedToolInput)
    );
}

#[test]
fn read_different_offset_is_not_equivalent() {
    let a = tc("Read", json!({ "path": "/x", "offset": 10 }));
    let b = tc("Read", json!({ "path": "/x", "offset": 20 }));
    assert_eq!(
        tool_call_equivalent(&a, &b),
        Err(DriftCategory::MismatchedToolInput)
    );
}

#[test]
fn read_different_limit_is_not_equivalent() {
    let a = tc("Read", json!({ "path": "/x", "limit": 10 }));
    let b = tc("Read", json!({ "path": "/x", "limit": 20 }));
    assert_eq!(
        tool_call_equivalent(&a, &b),
        Err(DriftCategory::MismatchedToolInput)
    );
}

#[test]
fn write_same_path_and_content_is_equivalent() {
    let a = tc("Write", json!({ "path": "/x", "content": "hello\nworld" }));
    let b = tc("Write", json!({ "path": "/x", "content": "hello\nworld" }));
    assert_eq!(tool_call_equivalent(&a, &b), Ok(()));
}

#[test]
fn write_different_content_is_not_equivalent() {
    let a = tc("Write", json!({ "path": "/x", "content": "hello" }));
    let b = tc("Write", json!({ "path": "/x", "content": "world" }));
    assert_eq!(
        tool_call_equivalent(&a, &b),
        Err(DriftCategory::MismatchedToolInput)
    );
}

#[test]
fn edit_same_post_state_sha_is_equivalent() {
    let a = tc(
        "Edit",
        json!({ "path": "/x", "post_state_sha256": "abc", "old_string": "foo", "new_string": "bar" }),
    );
    let b = tc(
        "Edit",
        json!({ "path": "/x", "post_state_sha256": "abc", "old_string": "DIFFERENT", "new_string": "PATCH" }),
    );
    assert_eq!(tool_call_equivalent(&a, &b), Ok(()));
}

#[test]
fn edit_different_post_state_sha_is_not_equivalent() {
    let a = tc("Edit", json!({ "path": "/x", "post_state_sha256": "abc" }));
    let b = tc("Edit", json!({ "path": "/x", "post_state_sha256": "def" }));
    assert_eq!(
        tool_call_equivalent(&a, &b),
        Err(DriftCategory::MismatchedToolInput)
    );
}

#[test]
fn edit_falls_back_to_old_new_strings_when_no_sha() {
    let a = tc(
        "Edit",
        json!({ "path": "/x", "old_string": "foo", "new_string": "bar" }),
    );
    let b = tc(
        "Edit",
        json!({ "path": "/x", "old_string": "foo", "new_string": "bar" }),
    );
    assert_eq!(tool_call_equivalent(&a, &b), Ok(()));
}

#[test]
fn edit_different_paths_never_equivalent() {
    let a = tc("Edit", json!({ "path": "/x", "post_state_sha256": "abc" }));
    let b = tc("Edit", json!({ "path": "/y", "post_state_sha256": "abc" }));
    assert_eq!(
        tool_call_equivalent(&a, &b),
        Err(DriftCategory::MismatchedToolInput)
    );
}

#[test]
fn glob_same_pattern_is_equivalent() {
    let a = tc("Glob", json!({ "pattern": "**/*.rs" }));
    let b = tc("Glob", json!({ "pattern": "**/*.rs" }));
    assert_eq!(tool_call_equivalent(&a, &b), Ok(()));
}

#[test]
fn glob_different_pattern_is_not_equivalent() {
    let a = tc("Glob", json!({ "pattern": "**/*.rs" }));
    let b = tc("Glob", json!({ "pattern": "**/*.toml" }));
    assert_eq!(
        tool_call_equivalent(&a, &b),
        Err(DriftCategory::MismatchedToolInput)
    );
}

#[test]
fn grep_trims_pattern_whitespace() {
    let a = tc(
        "Grep",
        json!({ "pattern": "  TODO  ", "path": "/src", "regex": false }),
    );
    let b = tc(
        "Grep",
        json!({ "pattern": "TODO", "path": "/src", "regex": false }),
    );
    assert_eq!(tool_call_equivalent(&a, &b), Ok(()));
}

#[test]
fn grep_regex_flag_must_match() {
    let a = tc(
        "Grep",
        json!({ "pattern": "fn", "path": "/src", "regex": true }),
    );
    let b = tc(
        "Grep",
        json!({ "pattern": "fn", "path": "/src", "regex": false }),
    );
    assert_eq!(
        tool_call_equivalent(&a, &b),
        Err(DriftCategory::MismatchedToolInput)
    );
}

#[test]
fn agent_equivalence_uses_subagent_type_plus_prompt_sha() {
    let a = tc(
        "Agent",
        json!({ "subagent_type": "explore", "prompt": "find auth code" }),
    );
    let b = tc(
        "Agent",
        json!({ "subagent_type": "explore", "prompt": "find auth code" }),
    );
    assert_eq!(tool_call_equivalent(&a, &b), Ok(()));
}

#[test]
fn agent_different_subagent_type_is_not_equivalent() {
    let a = tc(
        "Agent",
        json!({ "subagent_type": "explore", "prompt": "x" }),
    );
    let b = tc("Agent", json!({ "subagent_type": "plan", "prompt": "x" }));
    assert_eq!(
        tool_call_equivalent(&a, &b),
        Err(DriftCategory::MismatchedToolInput)
    );
}

#[test]
fn agent_different_prompt_is_not_equivalent() {
    let a = tc(
        "Agent",
        json!({ "subagent_type": "explore", "prompt": "x" }),
    );
    let b = tc(
        "Agent",
        json!({ "subagent_type": "explore", "prompt": "y" }),
    );
    assert_eq!(
        tool_call_equivalent(&a, &b),
        Err(DriftCategory::MismatchedToolInput)
    );
}

#[test]
fn unknown_tool_falls_back_to_canonical_json_sha_equality() {
    let a = tc("MyCustom", json!({ "alpha": 1, "beta": [2, 3] }));
    let b = tc("MyCustom", json!({ "beta": [2, 3], "alpha": 1 })); // key order differs
    assert_eq!(
        tool_call_equivalent(&a, &b),
        Ok(()),
        "canonical sha should ignore key order"
    );
}

#[test]
fn unknown_tool_different_input_not_equivalent() {
    let a = tc("MyCustom", json!({ "x": 1 }));
    let b = tc("MyCustom", json!({ "x": 2 }));
    assert_eq!(
        tool_call_equivalent(&a, &b),
        Err(DriftCategory::MismatchedToolInput)
    );
}

#[test]
fn unknown_tool_handles_null_and_arrays() {
    let a = tc("X", json!({ "arr": [1, null, true, "s"] }));
    let b = tc("X", json!({ "arr": [1, null, true, "s"] }));
    assert_eq!(tool_call_equivalent(&a, &b), Ok(()));
}

#[test]
fn drift_category_is_copyable_and_eq() {
    let d = DriftCategory::MismatchedToolInput;
    let copied: DriftCategory = d;
    assert_eq!(d, copied);
}

// ── Schema-v2 hook+skill equivalence (M15) ──────────────────────────────

fn hp(event: &str, decision: HookDecision, exit_code: i32) -> HookProjection {
    HookProjection {
        event: event.to_owned(),
        matcher: None,
        decision,
        exit_code,
        output: String::new(),
    }
}

fn sp(name: &str, source: SkillSource) -> SkillProjection {
    SkillProjection {
        name: name.to_owned(),
        source,
        instructions_injected: true,
    }
}

#[test]
fn hook_identical_projections_are_equivalent() {
    let a = hp("PreToolUse", HookDecision::Allow, 0);
    let b = hp("PreToolUse", HookDecision::Allow, 0);
    assert_eq!(hook_event_equivalent(&a, &b), Ok(()));
}

#[test]
fn hook_different_event_drifts() {
    let a = hp("PreToolUse", HookDecision::Allow, 0);
    let b = hp("PostToolUse", HookDecision::Allow, 0);
    assert_eq!(
        hook_event_equivalent(&a, &b),
        Err(DriftCategory::MismatchedHookEvent)
    );
}

#[test]
fn hook_different_decision_drifts() {
    let a = hp("PreToolUse", HookDecision::Allow, 0);
    let b = hp("PreToolUse", HookDecision::Block, 2);
    assert_eq!(
        hook_event_equivalent(&a, &b),
        Err(DriftCategory::MismatchedHookEvent)
    );
}

#[test]
fn hook_different_exit_code_drifts_even_if_decision_matches() {
    let mut a = hp("PreToolUse", HookDecision::Warn, 1);
    let mut b = hp("PreToolUse", HookDecision::Warn, 99);
    assert_eq!(
        hook_event_equivalent(&a, &b),
        Err(DriftCategory::MismatchedHookEvent)
    );
    // sanity: matching exit codes pass
    a.exit_code = 1;
    b.exit_code = 1;
    assert_eq!(hook_event_equivalent(&a, &b), Ok(()));
}

#[test]
fn hook_matcher_compared_for_equality() {
    let mut a = hp("PreToolUse", HookDecision::Allow, 0);
    let mut b = hp("PreToolUse", HookDecision::Allow, 0);
    a.matcher = Some("Bash".into());
    b.matcher = Some("Read".into());
    assert_eq!(
        hook_event_equivalent(&a, &b),
        Err(DriftCategory::MismatchedHookEvent)
    );
    b.matcher = Some("Bash".into());
    assert_eq!(hook_event_equivalent(&a, &b), Ok(()));
}

#[test]
fn hook_output_whitespace_normalized_like_bash() {
    let mut a = hp("PreToolUse", HookDecision::Warn, 1);
    let mut b = hp("PreToolUse", HookDecision::Warn, 1);
    a.output = "  warning   line  ".into();
    b.output = "warning line".into();
    assert_eq!(
        hook_event_equivalent(&a, &b),
        Ok(()),
        "whitespace must collapse like Bash equivalence"
    );
}

#[test]
fn skill_identical_projections_are_equivalent() {
    let a = sp("rust-debug", SkillSource::AutoMatched);
    let b = sp("rust-debug", SkillSource::AutoMatched);
    assert_eq!(skill_invocation_equivalent(&a, &b), Ok(()));
}

#[test]
fn skill_different_name_drifts() {
    let a = sp("rust-debug", SkillSource::AutoMatched);
    let b = sp("python-debug", SkillSource::AutoMatched);
    assert_eq!(
        skill_invocation_equivalent(&a, &b),
        Err(DriftCategory::MismatchedSkillInvocation)
    );
}

#[test]
fn skill_different_source_drifts() {
    let a = sp("rust-debug", SkillSource::UserInvoked);
    let b = sp("rust-debug", SkillSource::AutoMatched);
    assert_eq!(
        skill_invocation_equivalent(&a, &b),
        Err(DriftCategory::MismatchedSkillInvocation)
    );
}

#[test]
fn skill_instructions_injected_difference_drifts() {
    let mut a = sp("rust-debug", SkillSource::UserInvoked);
    let mut b = sp("rust-debug", SkillSource::UserInvoked);
    a.instructions_injected = true;
    b.instructions_injected = false;
    assert_eq!(
        skill_invocation_equivalent(&a, &b),
        Err(DriftCategory::MismatchedSkillInvocation)
    );
}

// ── M24: mutation-coverage kill-tests ─────────────────────────────────
// These tests exercise the *distinction* between dedicated per-tool
// equivalence rules and the `default_equivalent` canonical-JSON fallback.
// Without them, deleting a Read/Write/Glob/Agent match arm in
// `tool_call_equivalent` survives mutation testing because the test
// suite never authored a pair that the dedicated rule says-equivalent
// but default-equivalent says drift. Each test below pairs (a) two
// inputs the dedicated rule accepts as equivalent (b) those same
// inputs differ syntactically enough that default_equivalent's
// canonical sha256 would say drift. Asserting Ok(()) kills the
// arm-deletion mutation.

#[test]
fn read_arm_kills_arm_deletion_mutant_via_extra_field() {
    // Same (path, offset, limit) tuple — read_equivalent accepts.
    // Extra field on `b` makes canonical JSON differ — default_equivalent
    // would reject. Asserting Ok kills "delete match arm Read".
    let a = tc(
        "Read",
        json!({ "path": "src/lib.rs", "offset": 0, "limit": 100 }),
    );
    let b = tc(
        "Read",
        json!({ "path": "src/lib.rs", "offset": 0, "limit": 100, "_origin": "auto" }),
    );
    assert_eq!(tool_call_equivalent(&a, &b), Ok(()));
}

#[test]
fn write_arm_kills_arm_deletion_mutant_via_extra_field() {
    let a = tc("Write", json!({ "path": "x.rs", "content": "hello" }));
    let b = tc(
        "Write",
        json!({ "path": "x.rs", "content": "hello", "_atime_hint": 1234 }),
    );
    assert_eq!(tool_call_equivalent(&a, &b), Ok(()));
}

#[test]
fn glob_arm_kills_arm_deletion_mutant_via_extra_field() {
    let a = tc("Glob", json!({ "pattern": "**/*.rs" }));
    let b = tc("Glob", json!({ "pattern": "**/*.rs", "_max_results": 50 }));
    assert_eq!(tool_call_equivalent(&a, &b), Ok(()));
}

#[test]
fn agent_arm_kills_arm_deletion_mutant_via_extra_field() {
    // (subagent_type, prompt_sha256) tuple — agent_equivalent accepts.
    let a = tc(
        "Agent",
        json!({ "subagent_type": "Explore", "prompt": "find foo" }),
    );
    let b = tc(
        "Agent",
        json!({ "subagent_type": "Explore", "prompt": "find foo", "description": "scout" }),
    );
    assert_eq!(tool_call_equivalent(&a, &b), Ok(()));
}

#[test]
fn edit_fallback_uses_logical_and_not_or() {
    // Edit fallback path (no post_state_sha256 supplied): rule is
    // `old_a == old_b && new_a == new_b`. If a mutation flipped this to
    // ||, two edits with matching old_string but DIFFERENT new_string
    // would falsely pass. This test catches that.
    let a = tc(
        "Edit",
        json!({ "path": "x.rs", "old_string": "foo", "new_string": "bar" }),
    );
    let b = tc(
        "Edit",
        json!({ "path": "x.rs", "old_string": "foo", "new_string": "DIFFERENT" }),
    );
    assert_eq!(
        tool_call_equivalent(&a, &b),
        Err(DriftCategory::MismatchedToolInput),
        "old matches but new differs MUST fail (kills && -> || mutation)"
    );
}
