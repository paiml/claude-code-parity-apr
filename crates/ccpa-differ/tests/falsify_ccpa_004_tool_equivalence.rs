//! FALSIFY-CCPA-004 — `tool_call_equivalence`.
//!
//! Asserts the per-tool equivalence rules listed under
//! `contracts/claude-code-parity-apr-v1.yaml § tool_equivalence_rules`.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods // serde_json::json! expands to internal unwrap
)]

use ccpa_differ::{tool_call_equivalent, DriftCategory, ToolCall};
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
