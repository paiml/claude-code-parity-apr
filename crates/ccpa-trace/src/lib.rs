//! JSONL trace schema for the claude-code-parity-apr harness.
//!
//! This crate is the typed Rust mirror of the YAML schema in
//! `contracts/claude-code-parity-apr-v1.yaml § trace_schema`. The contract
//! is the source of truth; this code MUST round-trip every example the
//! contract enumerates, asserted by **FALSIFY-CCPA-001**
//! (`tests/falsify_ccpa_001_roundtrip.rs`).
//!
//! Every record kind is a variant of [`Record`], discriminated by the
//! `kind` JSON key. [`Trace`] is a thin wrapper around `Vec<Record>` that
//! parses and re-emits JSONL.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use serde::{Deserialize, Serialize};

/// Trace-schema version. Bumping this requires a contract revision.
///
/// `v2` (M15) adds two additive record kinds — [`Record::HookEvent`] and
/// [`Record::SkillInvocation`] — to cover the `hooks` and `skills` rows
/// of the apr-code parity matrix. Existing v1 traces remain parse-compatible.
pub const SCHEMA_VERSION: u32 = 2;

/// Which orchestrator produced the trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Actor {
    /// Anthropic Claude Code (the teacher).
    ClaudeCode,
    /// `apr code` (the student).
    AprCode,
}

/// Reason an assistant turn or session terminated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// Assistant signaled end-of-turn.
    EndTurn,
    /// `max_tokens` budget reached.
    MaxTokens,
    /// One of the configured `stop_sequences` matched.
    StopSequence,
    /// Assistant emitted a `tool_use` block; control returns to the runtime.
    ToolUse,
    /// Terminal error. Only valid on `Record::SessionEnd`.
    Error,
}

/// One content block within an assistant turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Block {
    /// Plain assistant text.
    Text {
        /// Verbatim assistant text.
        text: String,
    },
    /// Reasoning trace (Anthropic `thinking` block).
    Thinking {
        /// Verbatim reasoning text.
        thinking: String,
    },
    /// Tool invocation by the assistant.
    ToolUse {
        /// Provider-issued id (normalized to `<TOOL-N>` on diff).
        id: String,
        /// Registered tool name.
        name: String,
        /// JSON-shaped tool input — opaque to this schema.
        input: serde_json::Value,
    },
}

/// Decision a hook reached for the matched event. Mirrors
/// `aprender-orchestrate/agent/hooks.rs::HookDecision` 1:1 so a fixture
/// authored against either Claude Code or `apr code` semantics encodes
/// the same closed enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookDecision {
    /// Hook returned `exit_code=0` — proceed.
    Allow,
    /// Hook returned `exit_code=1` — proceed with a warning surfaced to the user.
    Warn,
    /// Hook returned `exit_code=2` — abort the in-flight event (tool, prompt, ...).
    Block,
}

/// How a skill came to be invoked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillSource {
    /// User typed `/skill-name` (or otherwise explicitly requested it).
    UserInvoked,
    /// `SkillRegistry::auto_match` fired on the active turn's text.
    AutoMatched,
}

/// File-system / process side-effects of a tool invocation.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SideEffects {
    /// Files the tool read (CWD-relative).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files_read: Vec<String>,
    /// Files the tool wrote or modified (CWD-relative).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files_written: Vec<String>,
    /// Process exit code, if applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

/// One JSONL record. The `kind` JSON key is the variant discriminator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Record {
    /// Opening record of a session.
    SessionStart {
        /// Schema version (1).
        v: u32,
        /// Stable session id (`UUIDv7`); normalized to `<SESSION>` on diff.
        session_id: String,
        /// ISO-8601 UTC timestamp; normalized to `<TS>` on diff.
        ts: String,
        /// Producer of the trace.
        actor: Actor,
        /// Model id.
        model: String,
        /// Git tree-hash of CWD at session start (asserted equal teacher==student).
        cwd_sha256: String,
    },
    /// User-typed prompt that drove this turn.
    UserPrompt {
        /// Schema version (1).
        v: u32,
        /// Zero-indexed turn number.
        turn: u32,
        /// Verbatim user text.
        text: String,
    },
    /// One assistant turn.
    AssistantTurn {
        /// Schema version (1).
        v: u32,
        /// Turn number ≥ 1.
        turn: u32,
        /// Ordered content blocks.
        blocks: Vec<Block>,
        /// Why the turn terminated.
        stop_reason: StopReason,
    },
    /// Result of one tool invocation.
    ToolResult {
        /// Schema version (1).
        v: u32,
        /// Turn number ≥ 2.
        turn: u32,
        /// Matches `Block::ToolUse.id`.
        tool_use_id: String,
        /// Whether the tool reported success.
        ok: bool,
        /// Tool stdout/stderr concatenation, or structured JSON.
        content: String,
        /// Optional file-system / process side-effects.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        side_effects: Option<SideEffects>,
    },
    /// One hook firing during the session. Schema-v2 (M15).
    ///
    /// Records a single hook invocation — for example a `PreToolUse` hook
    /// matching `Bash` and exiting with code `2` (Block). The runtime fires
    /// hooks at six canonical points (`SessionStart` / `PreToolUse` /
    /// `PostToolUse` / `UserPromptSubmit` / `Stop` / `SubagentStop`); the
    /// `event` field MUST be one of those, but is kept as a `String` so
    /// fixtures don't break if a future Claude Code revision adds a 7th.
    HookEvent {
        /// Schema version (1 — invariant under additive record-kind growth).
        v: u32,
        /// Turn number the hook fired against (`0` for `SessionStart` hooks
        /// that fire before any user prompt).
        turn: u32,
        /// Canonical event name (`"PreToolUse"`, `"PostToolUse"`, …).
        event: String,
        /// What the hook matched on, if applicable. For `PreToolUse` /
        /// `PostToolUse` this is the tool name (`"Bash"`); for
        /// `UserPromptSubmit` it may be a regex or `None`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        matcher: Option<String>,
        /// `Allow` / `Warn` / `Block` — see [`HookDecision`].
        decision: HookDecision,
        /// The literal exit code the hook process returned (`0` / `1` / `2`).
        /// Asserted equal to `decision`'s canonical mapping by the differ.
        exit_code: i32,
        /// Hook stdout/stderr concatenation, surfaced to the user verbatim.
        /// Empty when the hook silently allowed.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        output: String,
    },
    /// One skill being invoked. Schema-v2 (M15).
    ///
    /// Records the moment a skill (markdown-frontmatter-defined or
    /// agent-bundled) was loaded into the assistant's context, either
    /// because the user typed `/skill-name` or because
    /// `SkillRegistry::auto_match` fired on the turn text.
    SkillInvocation {
        /// Schema version (1 — invariant under additive record-kind growth).
        v: u32,
        /// Turn the skill was applied to.
        turn: u32,
        /// Skill name (matches the file stem of `<dir>/<name>.md` or the
        /// frontmatter `name:` of `<dir>/<name>/SKILL.md`).
        name: String,
        /// `UserInvoked` / `AutoMatched` — see [`SkillSource`].
        source: SkillSource,
        /// Whether the skill body was injected into the assistant's
        /// system prompt. Authored fixtures default this to `true`.
        #[serde(default)]
        instructions_injected: bool,
    },
    /// Closing record of a session.
    SessionEnd {
        /// Schema version (1).
        v: u32,
        /// Turn count.
        turn: u32,
        /// Why the session ended.
        stop_reason: StopReason,
        /// Wall-clock elapsed since `SessionStart`.
        elapsed_ms: u64,
        /// Total input tokens billed.
        tokens_in: u64,
        /// Total output tokens billed.
        tokens_out: u64,
    },
}

/// A complete trace — one JSONL file's contents.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Trace {
    /// Records in file order.
    pub records: Vec<Record>,
}

impl Trace {
    /// Construct an empty trace.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse a JSONL string into a trace. Empty lines are tolerated.
    ///
    /// # Errors
    ///
    /// Returns `serde_json::Error` if any non-empty line fails to parse
    /// against the schema (unknown record kind, missing required field,
    /// type mismatch, etc.).
    pub fn from_jsonl(s: &str) -> Result<Self, serde_json::Error> {
        let records = s
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(serde_json::from_str)
            .collect::<Result<Vec<Record>, _>>()?;
        Ok(Self { records })
    }

    /// Serialize the trace as JSONL (one record per line, trailing newline).
    ///
    /// # Errors
    ///
    /// Returns `serde_json::Error` if any record fails to serialize. With
    /// the bundled types this should be infallible, but the API is
    /// deliberately fallible to leave room for future custom types.
    pub fn to_jsonl(&self) -> Result<String, serde_json::Error> {
        let mut out = String::new();
        for record in &self.records {
            out.push_str(&serde_json::to_string(record)?);
            out.push('\n');
        }
        Ok(out)
    }
}
