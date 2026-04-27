//! Per-tool semantic equivalence rules.
//!
//! Source-of-truth: `contracts/claude-code-parity-apr-v1.yaml
//! § tool_equivalence_rules`. Each rule defines what constitutes
//! "the student emitted the same tool call" for a given tool. Two
//! syntactically-different inputs may be semantically equivalent
//! (`Bash "ls"` vs `Bash " ls "`) — the rules normalize before equality.

use sha2::{Digest, Sha256};

/// Lightweight projection of a [`ccpa_trace::Block::ToolUse`] reduced to
/// the fields the equivalence rules care about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    /// Tool name as registered in `apr code` / Claude Code (`Bash`,
    /// `Edit`, `Read`, ...).
    pub name: String,
    /// JSON-shaped tool input (verbatim from the trace).
    pub input: serde_json::Value,
}

/// Drift category emitted when two tool calls are NOT equivalent.
/// Mirrors `parity_score.drift_categories` in the contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriftCategory {
    /// Tool name matched but normalized inputs differ.
    MismatchedToolInput,
    /// Tool name itself differs.
    MismatchedToolName,
}

/// Decide whether two tool calls are semantically equivalent under the
/// per-tool rule set. Returns `Ok(())` on equivalence, `Err(drift)`
/// otherwise.
///
/// Per-tool rules:
/// - `Bash`: normalize(command) — collapse whitespace, drop trailing `;`
/// - `Read`: tuple `(path_normalized, offset?, limit?)`
/// - `Write`: tuple `(path_normalized, content_sha256)`
/// - `Edit`: tuple `(path_normalized, post_state_sha256)` — caller
///   computes the post-state sha for both teacher + student; we just
///   compare what the trace records
/// - `Glob`: pattern verbatim
/// - `Grep`: tuple `(pattern.trim(), path_normalized, regex_or_literal)`
/// - `Agent`: tuple `(subagent_type, prompt_sha256)`
/// - any other: sha256 of canonicalized JSON input
///
/// # Errors
///
/// Returns [`DriftCategory::MismatchedToolName`] or
/// [`DriftCategory::MismatchedToolInput`] when calls don't match.
pub fn tool_call_equivalent(a: &ToolCall, b: &ToolCall) -> Result<(), DriftCategory> {
    if a.name != b.name {
        return Err(DriftCategory::MismatchedToolName);
    }
    let equal = match a.name.as_str() {
        "Bash" => bash_equivalent(&a.input, &b.input),
        "Read" => read_equivalent(&a.input, &b.input),
        "Write" => write_equivalent(&a.input, &b.input),
        "Edit" => edit_equivalent(&a.input, &b.input),
        "Glob" => glob_equivalent(&a.input, &b.input),
        "Grep" => grep_equivalent(&a.input, &b.input),
        "Agent" => agent_equivalent(&a.input, &b.input),
        _ => default_equivalent(&a.input, &b.input),
    };
    if equal {
        Ok(())
    } else {
        Err(DriftCategory::MismatchedToolInput)
    }
}

fn bash_equivalent(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    let cmd_a = a
        .get("command")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let cmd_b = b
        .get("command")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    normalize_bash(cmd_a) == normalize_bash(cmd_b)
}

fn normalize_bash(s: &str) -> String {
    let collapsed: String = s.split_whitespace().collect::<Vec<&str>>().join(" ");
    collapsed.trim_end_matches(';').trim_end().to_owned()
}

fn read_equivalent(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    let path_a = path_str(a);
    let path_b = path_str(b);
    let offset_a = a.get("offset").and_then(serde_json::Value::as_u64);
    let offset_b = b.get("offset").and_then(serde_json::Value::as_u64);
    let limit_a = a.get("limit").and_then(serde_json::Value::as_u64);
    let limit_b = b.get("limit").and_then(serde_json::Value::as_u64);
    path_a == path_b && offset_a == offset_b && limit_a == limit_b
}

fn write_equivalent(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    let path_a = path_str(a);
    let path_b = path_str(b);
    let content_a = a
        .get("content")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let content_b = b
        .get("content")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    path_a == path_b && sha256_of(content_a) == sha256_of(content_b)
}

fn edit_equivalent(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    // Trace records the *call* — we compare on declared post-state if
    // it's already supplied (e.g. by a higher-level differ that
    // pre-computed). Otherwise compare the literal patch fields.
    let path_a = path_str(a);
    let path_b = path_str(b);
    let post_a = a
        .get("post_state_sha256")
        .and_then(serde_json::Value::as_str);
    let post_b = b
        .get("post_state_sha256")
        .and_then(serde_json::Value::as_str);
    if path_a != path_b {
        return false;
    }
    if let (Some(pa), Some(pb)) = (post_a, post_b) {
        return pa == pb;
    }
    // Fallback: compare normalized old_string + new_string fields.
    let old_a = a
        .get("old_string")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let old_b = b
        .get("old_string")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let new_a = a
        .get("new_string")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let new_b = b
        .get("new_string")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    old_a == old_b && new_a == new_b
}

fn glob_equivalent(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    let pat_a = a
        .get("pattern")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let pat_b = b
        .get("pattern")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    pat_a == pat_b
}

fn grep_equivalent(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    let pat_lhs = a
        .get("pattern")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .trim();
    let pat_rhs = b
        .get("pattern")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .trim();
    let path_lhs = path_str(a);
    let path_rhs = path_str(b);
    let regex_lhs = a
        .get("regex")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let regex_rhs = b
        .get("regex")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    pat_lhs == pat_rhs && path_lhs == path_rhs && regex_lhs == regex_rhs
}

fn agent_equivalent(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    let type_a = a
        .get("subagent_type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let type_b = b
        .get("subagent_type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let prompt_a = a
        .get("prompt")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let prompt_b = b
        .get("prompt")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    type_a == type_b && sha256_of(prompt_a) == sha256_of(prompt_b)
}

fn default_equivalent(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    canonical_sha(a) == canonical_sha(b)
}

fn path_str(v: &serde_json::Value) -> &str {
    v.get("path")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
}

fn sha256_of(s: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    hex(&hasher.finalize())
}

fn canonical_sha(v: &serde_json::Value) -> String {
    // Canonicalize via `to_string` on a `BTreeMap`-like recursion.
    let canonical = canonicalize(v);
    sha256_of(&canonical)
}

fn canonicalize(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => "null".to_owned(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => format!("{s:?}"),
        serde_json::Value::Array(arr) => {
            let parts: Vec<String> = arr.iter().map(canonicalize).collect();
            format!("[{}]", parts.join(","))
        }
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let parts: Vec<String> = keys
                .into_iter()
                .map(|k| format!("{k:?}:{}", canonicalize(&map[k])))
                .collect();
            format!("{{{}}}", parts.join(","))
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}
