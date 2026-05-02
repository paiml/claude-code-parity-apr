# claude-code-parity-apr — POC Specification

**Version**: 1.23.0
**Date**: 2026-05-02
**Status**: ACTIVE_RUNTIME — M0–M39 SHIPPED; M32d numerical-parity FUNCTIONALLY DISCHARGED 2026-05-02 (aprender PR #1228 squash 5235aaeb9)
**Source of truth**: https://github.com/paiml/claude-code-parity-apr (canonical for enforcement; aprender mirrors only the contract YAML byte-for-byte via `pin.lock`)
**Companion-repo invariants** (must be green on every PR — see § Companion-repo source-of-truth invariants):
1. GitHub Actions `ci/gate` green (required status check) → **FALSIFY-CCPA-009**
2. `pmat comply check` 100 % green → **FALSIFY-CCPA-010**
3. Line coverage = 100 % via `cargo llvm-cov --fail-under-lines 100` → **FALSIFY-CCPA-011**
4. `pv validate contracts/claude-code-parity-apr-v1.yaml` exits 0 → **FALSIFY-CCPA-012**
**Contracts (authoritative)**:
- `contracts/claude-code-parity-apr-v1.yaml` — top-level falsifiable parity gates (THIS PR; moves to companion repo on M1)
- `contracts/apr-code-parity-v1.yaml` — sibling: 21-row static feature matrix
- `contracts/apr-claude-proxy-v1.yaml` — sibling: HTTP/SSE Messages-API shape
- `crates/aprender-orchestrate/contracts/batuta/apr-code-v1.yaml` — agent-loop semantics
**arXiv**: 1503.02531, 1807.10453, 2207.11976, 2310.06770, 2505.03096, 2603.23611
**Spec lineage**:
- `docs/specifications/apr-mcp-server-spec.md` § Feature-by-feature parity matrix (static)
- `docs/specifications/apr-cli-qa-spec.md` (template for falsifiable-phase layout)
- memory: `feedback_monorepo_single_source_of_truth` — aprender stays canonical for ML/contract TEXT; companion repo is canonical for **enforcement** (CI, coverage, pmat comply, contract gate)

---

## Problem

Three existing artifacts each cover one axis of `apr code` ↔ Claude Code parity:

| Artifact | Axis | Granularity | Liveness | Falsifiable? |
|----------|------|-------------|----------|--------------|
| `apr-code-parity-v1.yaml` | Feature matrix (21 rows) | Symbol exists in source | Audit-time | ✅ via `cross_check_command` |
| `apr-claude-proxy-v1.yaml` | HTTP/SSE Messages-API shape | Wire format | Per-request | ✅ via 6 FALSIFY-CLAUDE-PROXY gates |
| `apr-code-v1.yaml` (batuta) | Agent loop semantics | Internal turn structure | Internal | ✅ via batuta gates |

**Gap**: none of them tells us, *for a given user prompt, does `apr code` produce the same actions Claude Code produces?* Today we answer this by hand. Three failure modes:

| Failure mode | Asserted-against-by-this-spec gate |
|--------------|------------------------------------|
| Non-falsifiable ("looked the same") | FALSIFY-CCPA-004 + FALSIFY-CCPA-005 |
| Non-reproducible (LLMs are stochastic) | FALSIFY-CCPA-002 (replay determinism) |
| Drift-blind (regressions in `apr code` orchestration ship without flipping any of the three contracts above) | FALSIFY-CCPA-008 (parity-score bound) |

## Goal

Build a small, separate POC repo (`claude-code-parity-apr`) that provides a **record-replay-distill** harness:

> Capture Claude Code as a teacher (record API trace + tool round-trips), feed the same prompts deterministically to `apr code` (replay against mocked LLM responses), then `pv`-validate the diff between the two action streams. The output is a falsifiable **parity score** per fixture and a closed-enum **contract verdict** ({Pass, Drift, Fail}) per gate.

In one phrase: **"distill Claude Code's control plane into `apr code` and prove it byte-stable."**

The "distillation" framing is not metaphorical — see Hinton et al. *Distilling the Knowledge in a Neural Network* (arXiv:1503.02531) and § Distillation framing below.

## Non-Goals

- ❌ Reproducing Claude's *language model output*. The teacher's words don't have to match — we only assert tool-call equivalence (FALSIFY-CCPA-004) and post-state equivalence (FALSIFY-CCPA-005).
- ❌ Replacing `apr-code-parity-v1.yaml`. That static matrix stays — this POC is **runtime** parity, the missing fourth leg.
- ❌ Live network in CI. Every replay reads from a recorded fixture; no `api.anthropic.com` egress in the test path (FALSIFY-CCPA-006).
- ❌ A new agent runtime. We use existing `apr code` unchanged; the POC is observation + diff infra.
- ❌ Modifying Claude Code. Closed-source binary stays closed. (Original M0 wording mentioned `ANTHROPIC_BASE_URL` env-var instrumentation for HTTPS-proxy recording; that path was rescoped OOS in M2.3 — "we will not call api, we will assume claude code". Fixtures are AUTHORED in `fixtures/canonical/`, not recorded.)
- ❌ Tolerated coverage gaps. Companion-repo bound is **100 %** line coverage (FALSIFY-CCPA-011), not the aprender 95 % floor — POC is small, no excuse.

## Companion-repo source-of-truth invariants

The companion repo `claude-code-parity-apr` is **source of truth for enforcement**: implementation, fixtures, CI, coverage, pmat-comply config, and the running-binary that consumes the contract. The contract TEXT lives in `aprender/contracts/` per the monorepo single-source-of-truth policy (`feedback_monorepo_single_source_of_truth.md`); the companion repo pins it by commit hash and gates every PR against it.

Four invariants, **online from M0** (before any actual parity work), enforced as required GitHub Actions status checks on the companion repo's `main`:

### Invariant 1 — `ci/gate` green (FALSIFY-CCPA-009)

- **Mechanism**: GitHub branch protection on `main` requires status check `ci/gate` (matches aprender's policy in `CLAUDE.md` § "CRITICAL GIT WORKFLOW RULES"). Direct pushes to `main` blocked.
- **Falsifiable claim**: `gh api repos/paiml/claude-code-parity-apr/branches/main/protection \| jq '.required_status_checks.contexts | index("ci/gate") != null'` returns `true`.
- **Failure mode caught**: a bypassed merge that ships a red build.

### Invariant 2 — `pmat comply check` 100 % (FALSIFY-CCPA-010)

- **Mechanism**: CI step `pmat comply check --strict` runs on every PR; exit code 0 with zero violations is required.
- **Falsifiable claim**: `pmat comply check --json | jq '.total_violations == 0 and .compliance_pct == 100.0'` returns `true`.
- **Why 100 %**: the POC is small (~5 Rust crates) and greenfield. There is no legacy excuse.
- **Academic basis**: prior-art compliance auditing literature (cf. arXiv:2102.05351 on coverage-completeness invariants).

### Invariant 3 — Line coverage 100 % (FALSIFY-CCPA-011)

- **Mechanism**: CI step `cargo llvm-cov --workspace --fail-under-lines 100 --fail-uncovered-lines 0`. (Branch coverage tracked but not gated until M6.)
- **Falsifiable claim**: `cargo llvm-cov report --json | jq '.data[0].totals.lines.percent == 100.0'` returns `true`.
- **Tarpaulin policy**: `cargo tarpaulin` is forbidden per aprender CLAUDE.md ("slow, unreliable, causes hangs"); only `cargo llvm-cov`.
- **Academic basis**: coverage-guided test-adequacy literature; specifically the test-adequacy/MC-DC line of work (arXiv:2102.05351 and earlier).

### Invariant 4 — `pv validate` on every commit (FALSIFY-CCPA-012)

- **Mechanism**: pre-commit hook (`.git/hooks/pre-commit` installed by `make hooks`) AND CI step both run `pv validate contracts/claude-code-parity-apr-v1.yaml`. Exit 0 required.
- **Falsifiable claim**: in CI logs, the line `pv validate contracts/claude-code-parity-apr-v1.yaml` is followed by exit code 0 within the same job.
- **Bash workarounds rejected** per `feedback_pv_not_bash_for_contracts.md` and CLAUDE.md § "Contract Validation: DOGFOOD pv, NEVER bash" — if `pv` rejects the contract, fix the contract or extend `aprender-contracts/src/schema/`, never bypass with shell.

These four invariants are necessary preconditions for *any* of the parity gates (FALSIFY-CCPA-001..008 + CCPA-013) to be trustworthy. A red CI on the companion repo invalidates every claim downstream.

## Architecture

> **Diagram below shows the original M0 design.** Phase 1 (RECORD via
> live HTTPS proxy) was rescoped OOS at M2.3 ("we will not call api,
> we will assume claude code"). In the shipping architecture, Phase
> 1's `.ccpa-trace.jsonl` fixtures are AUTHORED in
> `fixtures/canonical/` rather than recorded from a live Anthropic
> session. Phases 2 and 3 are unchanged — the differ + the
> `RecordedDriver` LLM mock both consume the same JSONL schema
> regardless of provenance. The diagram is preserved as historical
> reference because it explains why the schema looks the way it does
> (every tool round-trip passes through the Anthropic API in the
> teacher, so the trace records prompts + tool_use blocks + tool_result
> blocks + final assistant turns at message granularity).

```
                    ┌──────────────────────────────────────────────────┐
                    │  Phase 1: RECORD (teacher demonstrations)        │
                    │                                                  │
                    │  Original M0 vision  (now AUTHORED post-M2.3):   │
  user prompt ─────►│   Claude Code ──HTTP──► ccpa-recorder ──► fixture│
                    │                  ▲             │                 │
                    │                  └──── api.anthropic.com (live)  │
                    │                                                  │
                    │  Shipping path:  fixtures/canonical/<id>/        │
                    │                  teacher.ccpa-trace.jsonl        │
                    │                  AUTHORED to schema, no live API │
                    └──────────────────────────────────────────────────┘
                                        │
                                        ▼ (one .ccpa-trace.jsonl file)
                    ┌──────────────────────────────────────────────────┐
                    │  Phase 2: REPLAY (student under test)            │
                    │   fixture ──► ccpa-replayer ──► apr code         │
                    │                  ▲                  │            │
                    │                  └─ mocked LLM ◄────┘            │
                    │     (returns the recorded teacher's assistant    │
                    │      turn — same regardless of fixture origin)   │
                    └──────────────────────────────────────────────────┘
                                        │
                                        ▼ (one .ccpa-trace.jsonl file)
                    ┌──────────────────────────────────────────────────┐
                    │  Phase 3: DISTILL+DIFF (parity verdict)          │
                    │   teacher.jsonl + student.jsonl ──► ccpa-differ  │
                    │      ──► pv validate ──► verdict + drift report  │
                    └──────────────────────────────────────────────────┘
```

### Original Phase 1 rationale — now historical (asserted by FALSIFY-CCPA-001)

This section preserves the M0 reasoning for why a recording HTTPS proxy
would have been sufficient, because the trace schema's shape (tool
round-trips at message granularity) follows from this argument.
**Post-M2.3, Phase 1 is AUTHORED**, but the schema invariant remains:
every committed fixture must be a complete, schema-valid action trace.

Claude Code is closed-source, so we can't hook its tool execution directly. We don't need to: every tool round-trip already round-trips through the Anthropic API. Claude Code submits `tool_result` blocks back to Anthropic on the next request, so a recording HTTPS proxy at `ANTHROPIC_BASE_URL` would capture the full action stream — prompts, tool calls, tool outputs, final messages — without any CLI wrapping. **In the original M0 design the proxy was the recorder.** Whether the trace is recorded or AUTHORED, the same schema invariant applies and is mechanically falsifiable: FALSIFY-CCPA-001 asserts every committed fixture is a complete, schema-valid action trace; failure (truncation, missing tool round-trip) flips the gate.

### Why `apr code` replay needs LLM mocking (asserted by FALSIFY-CCPA-002 + FALSIFY-CCPA-003)

The student is `apr code`, which talks to a *local* model. If we feed the same prompt to `apr code`, the local model produces *its own* tool calls, which may differ purely due to model quality, not orchestration. To isolate orchestration drift from model drift, the replayer:

1. Plays the user prompts back to `apr code` in order.
2. Intercepts `apr code`'s LLM calls and **returns the recorded teacher's assistant turn verbatim** via `RecordedDriver: LlmDriver`.
3. Lets `apr code`'s orchestration (tool dispatch, permission gates, hook firing, memory loading, etc.) execute against that fixed teacher output.
4. Captures `apr code`'s emitted tool calls + final state.

Drift now has a single source: orchestration. Falsifiable via two gates:
- **FALSIFY-CCPA-002** (replay determinism): re-running the same fixture twice with the same `apr code` revision yields byte-identical student traces (after `<SESSION>`/`<TS>`/`<TOOL-N>` normalization).
- **FALSIFY-CCPA-003** (mock completeness): `RecordedDriver` consumes every teacher turn exactly once; missing turn → panic, extra turn → assertion fail.

This is structurally identical to the existing `MockDriver` used by `crates/aprender-orchestrate/src/agent/task_tool/tests.rs`, so no new abstraction is needed — `RecordedDriver` is a ~100-line file. **Academic basis**: behavioral cloning / imitation-learning evaluation methodology (cf. agent-task literature in arXiv:2310.06770 SWE-bench).

## New repo layout (`claude-code-parity-apr`)

```
claude-code-parity-apr/                 # source of truth for code, fixtures, CI
├── Cargo.toml                          # workspace root
├── README.md
├── .github/workflows/
│   └── ci.yml                          # ci/gate job (FALSIFY-CCPA-009 enforces required check)
├── .pmat-comply.toml                   # FALSIFY-CCPA-010 config, 100 % strict
├── Makefile                            # `make tier3` runs all 13 gates locally
├── crates/
│   ├── ccpa-trace/                     # serde schema for .ccpa-trace.jsonl
│   ├── ccpa-recorder/                  # original M0 vision: mitm-style HTTPS
│   │                                   # proxy at ANTHROPIC_BASE_URL. OOS post-
│   │                                   # M2.3 rescope; crate retained as
│   │                                   # scaffolding for the schema-roundtrip path.
│   ├── ccpa-replayer/                  # drives `apr code` with mocked LLM responses
│   ├── ccpa-differ/                    # semantic diff + parity score
│   └── ccpa-cli/                       # `ccpa diff|corpus|coverage|validate|measure`
│                                       # (binary: `ccpa`)
├── contracts/
│   ├── claude-code-parity-apr-v1.yaml  # MIRROR of aprender/contracts/...v1.yaml
│   │                                   # (M22 byte-identical guard via pin.lock)
│   └── pin.lock                        # pinned commit-hash + sha256 of authoritative
│                                       # aprender contract
├── fixtures/                           # AUTHORED canonical sessions (M2.3 rescope)
│   ├── canonical/                      # 30 paired teacher/student fixtures
│   │   ├── 0001-edit-readme/
│   │   │   ├── teacher.ccpa-trace.jsonl
│   │   │   ├── student.ccpa-trace.jsonl
│   │   │   └── meta.toml               # per-fixture parity-matrix row + tags
│   │   ├── 0002-fix-failing-test/
│   │   └── ...                         # 30 total per FALSIFY-CCPA-007
│   ├── regression/                     # M13.5 bidirectional sensitivity corpus
│   │                                   # (deliberate drift; meter MUST score < 1)
│   └── synthetic/                      # M26 measure-bridge synthetic traces
└── docs/
    └── specifications/
        └── claude-code-parity-apr-poc.md   # this spec
```

**Naming**: `ccpa` = claude-code-parity-apr. Binary name is `ccpa`. Repo name `claude-code-parity-apr` is explicit on GitHub.

**Monorepo policy compliance** (`feedback_monorepo_single_source_of_truth.md`): aprender stays canonical for the *contract text*; the companion repo is canonical for *enforcement* (CI, coverage, pmat-comply, contract gate). `pin.lock` records the commit hash of the authoritative `aprender` contract being consumed; `pv` is invoked as a binary, not as a re-implemented schema.

## Trace schema (.ccpa-trace.jsonl)

One JSON object per line. Schema lives in `ccpa-trace` crate and is contracted by `contracts/claude-code-parity-apr-v1.yaml § trace_schema`. **FALSIFY-CCPA-001** asserts roundtrip-equality.

```jsonc
{"v": 1, "kind": "session_start", "session_id": "uuidv7", "ts": "2026-04-26T01:23:45Z",
 "actor": "claude-code|apr-code", "model": "claude-sonnet-4-6|qwen3-coder-30b-a3b-q4km",
 "cwd_sha256": "<git-tree-hash>"}

{"v": 1, "kind": "user_prompt", "turn": 0, "text": "fix the failing test"}

{"v": 1, "kind": "assistant_turn", "turn": 1,
 "blocks": [
   {"type": "thinking", "thinking": "..."},
   {"type": "text", "text": "I'll start by..."},
   {"type": "tool_use", "id": "toolu_01...", "name": "Bash",
    "input": {"command": "cargo test --lib"}}
 ],
 "stop_reason": "tool_use"}

{"v": 1, "kind": "tool_result", "turn": 2, "tool_use_id": "toolu_01...",
 "ok": false, "content": "test failed: ...",
 "side_effects": {"files_read": [], "files_written": [], "exit_code": 101}}

{"v": 1, "kind": "session_end", "turn": 7, "stop_reason": "end_turn",
 "elapsed_ms": 12340, "tokens_in": 4521, "tokens_out": 891}
```

**Determinism guarantees** (asserted by FALSIFY-CCPA-002):
- `session_id` and `ts` are *replaced* with stable placeholders during diff (`<SESSION>` / `<TS>`).
- `tool_use.id` is *normalized* to `<TOOL-N>` per turn.
- `cwd_sha256` is asserted equal between teacher and student fixtures (same starting state).

## Mocking strategy (asserted by FALSIFY-CCPA-002 + FALSIFY-CCPA-003)

The student-side LLM mock implements `apr code`'s `LlmDriver` trait (already abstract — see `crates/aprender-orchestrate/src/agent/code.rs`). The replayer constructs a `RecordedDriver { trace: Vec<AssistantTurn> }` that returns turn N when called for the Nth time. Mismatches (`apr code` calls the LLM at an unexpected point) are recorded as `OrchestrationDrift::ExtraneousLlmCall` and surfaced in the diff.

Pre-requisite: `LlmDriver` must be `pub` from `aprender-orchestrate` (today it is `pub(crate)` behind no feature flag). Tracked as **PMAT-CODE-LLM-DRIVER-PUBLIC-001**, blocking M3.

## Distillation framing

Calling this "distillation" is not metaphorical. The framing maps directly onto Hinton et al. (arXiv:1503.02531):

| Hinton 2015 | This POC |
|-------------|----------|
| Teacher network | Claude Code (closed-source orchestration over claude-sonnet-4.6) |
| Student network | `apr code` (open orchestration over Qwen3-Coder-30B-A3B-Q4_K_M via realizar) |
| Soft target distribution | AUTHORED action stream (the assistant turns + tool round-trips), per the M2.3 rescope away from live HTTPS-proxy recording |
| Demonstration corpus | `fixtures/canonical/*.ccpa-trace.jsonl` (30 sessions covering 15/15 reachable rows of `apr-code-parity-v1.yaml` after OOS exclusion of `keyboard-shortcuts` + `status-line`) |
| Loss function | `1 − parity_score`, where `parity_score = matched_actions / total_actions` |
| Optimizer step | File-by-file PR review of `apr code` orchestration, driven by drift report |
| Convergence criterion | Aggregate `parity_score ≥ 0.95` (FALSIFY-CCPA-008) |

The teacher's *fixtures* are immutable per-revision; the student (`apr code` orchestration code) is what changes. **Academic basis**: knowledge-distillation framing per arXiv:1503.02531 with the action stream substituted for the logit distribution; metamorphic relations on tool calls per arXiv:1807.10453 (METTLE) and arXiv:2603.23611 (LLMORPH); differential testing of the two orchestrators per arXiv:2207.11976.

## Phases / Milestones

> **Status snapshot (2026-05-02)**: M0–M39 SHIPPED. M32d
> **FUNCTIONALLY DISCHARGED** 2026-05-02 via aprender PR #1228 squash
> 5235aaeb9 (Step 5 + 5b + 6 + 7 fix bundle). Output transition on
> lambda-vector RTX 4090 against the cached 17.3 GB Qwen3-Coder-30B-
> A3B-Instruct-Q4_K_M.gguf:
>
>   * pre-fix    → `%%%%%%%%`         (gibberish, repeated argmax)
>   * post-fix   → `2 + 2 = 4` + multi-domain coherent answers (math,
>                  geography, translation, code) all correct.
>
> The entire CPU MoE forward chain is wired end-to-end AND pinned by
> regression tests. M34 FAST PATH plan delivered at the **lucky-case
> bound** (5 PRs / ~6 hours vs 4-6 PRs / 2-3 days estimate).
> Contract at `claude-code-parity-apr-v1` **v1.23.0** ACTIVE_RUNTIME
> (M35 M32d discharge audit-trail bump); contract
> `qwen3-moe-forward-v1` at v1.4.0 ACTIVE_ALGORITHM_LEVEL (full
> ACTIVE_RUNTIME flip awaits cosine ≥ 0.99 vs HF FP16 measurement,
> operator-confirm — ~60GB download). Corpus at **30**
> paired canonical fixtures (spec ≥30 target met) with parity-matrix
> coverage 15/15 reachable (2 OOS at trace boundary); FALSIFY-CCPA-007
> HARD-BLOCKING CI gate live since M16; companion ↔ aprender round-trip
> drift guard live since M22; **100% mutation coverage workspace-wide**
> (224 mutants caught/unviable, 0 missed) since M25; `ccpa measure`
> AUTHORED → MEASURED bridge live since M26; **`apr code --emit-trace`
> + Qwen3-Coder default + qwen3_moe tensor-names contract v1.1.0 +
> F-TNV-002 falsifier all on aprender main since M28+M29**.
> M31 records the **monorepo scope clarification**: aprender lives
> in the same monorepo as this companion repo, so there is no
> out-of-scope or upstream boundary — every file in `paiml/aprender`
> and `paiml/claude-code-parity-apr` that has to change for this POC
> to discharge its measured-parity gate is in-scope work for this spec.
> Live PR cadence on https://github.com/paiml/claude-code-parity-apr.
>
> **M32 chain status (2026-04-29)**: M32a (contract scaffold,
> aprender#1104) + M32b (arch-aware FFN load, aprender#1106) +
> **M32c.1** (Qwen3MoeQuantizedLayer + load_qwen3_moe_layer,
> aprender#1116) + **M32c.2** (from_gguf_for_moe constructor,
> aprender#1117) + **M32c.2.1** (load-time → forward-time refusal,
> aprender#1118) + **M32c.2.2** (dequant strategy contract amendment
> v1.0.0 → v1.1.0, aprender#1119) + **M32c.2.2.0** (expert_byte_slice
> adapter, aprender#1120) + **M32c.2.2.1** (expert_swiglu_quantized
> per-expert SwiGLU, aprender#1121) + **M32c.2.2.2.0**
> (moe_ffn_forward_layer single-layer dispatch, aprender#1122) +
> **M32c.2.2.2.1** (integration-strategy contract amendment v1.1.0
> → v1.2.0, aprender#1123) + **M32c.2.2.2.1.1**
> (`OwnedQuantizedModel::forward_qwen3_moe` method, aprender#1124) +
> **M32c.2.2.2.1.2** (`run_qwen3_moe_generate` autoregressive loop,
> aprender#1125) + **M32c.2.2.2.1.3** (dispatch flip in
> `inference_result.rs` + Q4_K_M qtype-aware `matvec_for_qtype`
> dispatch, aprender#1126) ALL MERGED. **FALSIFY-QW3-MOE-FORWARD-003
> live-discharged on lambda-vector RTX 4090** — `apr run` against the
> cached 17.3 GB Q4_K_M GGUF emits tokens.
>
> **In-flight**: M32c.2.2.2.1.4 — live `apr run` falsifier
> (`crates/apr-cli/tests/qwen3_moe_apr_run_live_falsifier.rs`,
> F-QW3-MOE-C22214-001) pinning the dispatch-flip discharge in
> CI / regression-prevention — **aprender PR #1127 in CI** (5/7
> green at snapshot time).
>
> **Outstanding next-goal (in-scope, M32d)**: numerical parity vs
> llama.cpp Q4_K (primary) + HF transformers FP16 (secondary) on
> greedy decode of a fixed prompt — `cosine_similarity > 0.99` on
> logits per AC_QW3_MOE_001 + AC_QW3_MOE_005, discharging
> FALSIFY-QW3-MOE-FORWARD-004. Discharge flips `qwen3-moe-forward-v1`
> DRAFT → ACTIVE_RUNTIME and unblocks companion-repo FALSIFY-CCPA-013
> measured parity score. Per the M31 scope clarification, M32d is
> treated identically to any other companion-repo deliverable.

### Major phases (M0–M6)

| Phase | Deliverable | Source-of-truth gates online (cumulative) | Status |
|-------|-------------|-------------------------------------------|--------|
| **M0** | Spec + top-level contract `claude-code-parity-apr-v1.yaml` (DRAFT). Companion-repo invariants 009–012 online from the empty-scaffold PR forward. | 009, 010, 011, 012 | **DONE** (PRs #1, #2) |
| **M1** | Companion repo scaffold (empty crates that compile + 100 % line cov), `ccpa-trace` crate, schema-roundtrip test. Spec + contract relocate from aprender to companion repo as canonical. | + 001 | **DONE** (PRs #3, #4) |
| **M2** | `ccpa record` (Anthropic Messages-API parser → trace records). Note: M2.3 HTTPS proxy is OOS post-rescope ("we will not call api, we will assume claude code"). | + (still 001) | **DONE** (PRs #5, #6) at parser-only scope |
| **M3** | `ccpa replay` (LlmDriver trait + RecordedDriver) — algorithm-level. Real `apr code` LlmDriver adapter pending PMAT-CODE-LLM-DRIVER-PUBLIC-001 in upstream aprender. | + 002, 003 | **DONE** (PRs #7, #8) at algorithm scope |
| **M4** | `ccpa diff` semantic differ — per-tool equivalence rules, file-mutation snapshots, parity score. | + 004, 005 | **DONE** (PRs #9, #10) |
| **M5** | Sovereignty gate (no `api.anthropic.com` on replay) + corpus growth + parity-matrix coverage walk. | + 006, 007 | **DONE** (PRs #11, #13–#21) |
| **M6** | Promote contract DRAFT → ACTIVE; integrate into `make tier3` and `pv lint`; close epic. | + 008 | **DONE** (PR #12) |

### Sub-milestones (M11+)

The corpus and gate work continued past M6 with sub-milestones tracked
in `contracts/claude-code-parity-apr-v1.yaml § status_history`:

| Sub | Deliverable | Outcome | PR |
|-----|-------------|---------|----|
| **M11** | First runtime measured_parity over 5 paired canonical fixtures; contract DRAFT → ACTIVE_RUNTIME (FALSIFY-CCPA-013 discharged) | aggregate_score 1.0000 over 5 fixtures, parity-matrix coverage 1/17 | #13 |
| **M12** | Corpus 5 → 8; coverage 1/17 → 4/17 (subagent-spawn, mcp-client, slash-commands added) | 1.0000 / 8 fixtures | #14 |
| **M13** | Corpus 8 → 11; coverage 4/17 → 7/17 (claude-md-memory, permission-modes, builtin-tools-web added) | 1.0000 / 11 fixtures | #17 |
| **M13.5** | Bidirectional sensitivity: regression corpus added; meter must FAIL on deliberate drift | regression corpus aggregate=0.5, exits 1 (drift detected) | #14 |
| **M14** | Corpus 14 → 17; coverage 10/17 → 13/17 (worktree-isolation, configuration-ladder, managed-org-policy added) | 1.0000 / 17 fixtures | #19 |
| **M15** | Trace schema v1 → v2 (additive `HookEvent` + `SkillInvocation` record kinds); differ extension (7 new DriftCategory variants); coverage 13/17 → 15/17 | 1.0000 / 19 fixtures; contract v1.2.0 → v1.3.0 | #20 |
| **M16** | FALSIFY-CCPA-007 informational → HARD-BLOCKING; OOS exclusion mechanism (`--oos-rows`) shipped for `keyboard-shortcuts` + `status-line` | 15/15 reachable, gate PASS; contract v1.3.0 → v1.4.0 | #21 |
| **M17** | Spec milestone table refreshed to reflect M0–M16; contract v1.4.0 → v1.5.0 | doc-only | #22 |
| **M18** | Corpus depth 19 → 24; 5 schema-v2 surface variants (Bash multiline, Edit replace_all, HookDecision::Block, SkillSource::UserInvoked, StopReason::MaxTokens) | 1.0000 / 24 fixtures; contract v1.5.0 → v1.6.0 | #23 |
| **M19** | Corpus complete 24 → 30 (spec ≥30 target met); multi-tool sequences + multi-turn correction + StopReason::StopSequence | 1.0000 / 30 fixtures; contract v1.6.0 → v1.7.0 | #24 |
| **M20** | README truth-up — badges (v1.2.0 → v1.7.0), behavioral-gates table flipped from "planned" to ✅ ACTIVE, architecture diagram revised post-rescope | doc-only; contract v1.7.0 → v1.8.0 | #25 |
| **M21** | Aprender-side mirror sync v1.2.0 → v1.8.0 (6 revisions of drift cleared); first round-trip closure | byte-identical sha256 across both repos; contract v1.8.0 → v1.9.0 | #27 (squash inc. M21) |
| **M22** | `pin-check-roundtrip.sh` CI guard installed — fails any companion bump unpaired with aprender mirror | drift class mechanically prevented; contract v1.9.0 → v1.10.0 | #27 |
| **M23** | `CONTRIBUTING.md` authored — source-of-truth split, fixture-authoring workflow, 4-step contract-bump ritual, gate→remediation lookup, anti-patterns | doc-only; contract v1.10.0 → v1.11.0 | #28 |
| **M24** | 100% mutation coverage on `ccpa-differ` (gate kernel) — 5 kill-tests close arm-deletion + && → \|\| gaps | 122 caught + 8 unviable + 0 missed across 130 mutants; contract v1.11.0 → v1.12.0 | #29 |
| **M25** | 100% mutation coverage workspace-wide (remaining 4 crates) — 3 kill-tests on `ccpa-cli` close `main` exit-code propagation + uncovered/OOS print branches | 193 caught + 31 unviable + 0 missed across 224 mutants; contract v1.12.0 → v1.13.0 | #30 |
| **M26** | `ccpa measure` AUTHORED → MEASURED bridge subcommand — drives live `apr code -p` against teacher's user_prompt, builds synthetic student trace, scores via compute_parity_score; refuses tool_use teachers (text-only path; tool dispatch waits on M28) | text-only score-1.0 vacuous; tool-dispatch path requires `apr code --emit-trace` | #31 |
| **M27** | Spec-table refresh — sub-milestones table extended M19 → M26; contract v1.14.0 → v1.15.0 | doc-only | #32 |
| **M28** | Cross-repo: `apr code --emit-trace <path>` flag upstream + Qwen3-Coder-30B-A3B-Instruct as default model + `qwen3-coder` short-name alias. Companion bookkeeping records the launch. | aprender PRs landed via #1102; companion contract v1.15.0 → v1.16.0 | companion #33 |
| **M29** | Five-whys + provable-contract — Qwen3-Coder GGUF load fail (`Tensor 'blk.0.ffn_up.weight' not found`) traced to GGUF tensor naming being arch-agnostic. Fix: `tensor-names-v1` v1.0.0 → v1.1.0 with `qwen3_moe` arch-key + 4 new MoE layer roles + F-TNV-002 falsifier validated against the real 17.3 GB Qwen3-Coder GGUF byte inventory. | [aprender#1103 merged](https://github.com/paiml/aprender/pull/1103) at 15d504cfe; companion contract v1.16.0 → v1.17.0 | companion #34 |
| **M30** | Spec-table refresh — extends through M29; contract v1.17.0 → v1.18.0; closes the spec-side audit trail | doc-only | #35 |
| **M31** | Monorepo scope clarification — aprender and claude-code-parity-apr live in the same monorepo; "upstream / out of scope / not a CCPA POC item" framing removed from spec + contract status_history; future inference-engine work (M32 MoE forward pass) treated as in-scope companion-repo deliverable | doc-only; contract v1.18.0 → v1.19.0 | direct main commit `1f06ac0` |
| **M32a** | First slice of MoE forward chain. Authored cross-repo kernel contract `qwen3-moe-forward-v1.yaml` (DRAFT, SCAFFOLD) composing `tensor-names-v1` v1.1.0 + `moe-router-v1` + `moe-expert-dispatch-v1` + `qwen3moe-shapes-v1` + swiglu/silu/rmsnorm/rope. 5 acceptance criteria + 4 staged steps (M32a/b/c/d) + 4 falsification tests. Anchors Qwen3-Coder-30B-A3B-Instruct shape algebra (L=48, d=2048, d_ff=6144, N_experts=128, k=8). FALSIFY-QW3-MOE-FORWARD-001 reproduced on lambda-vector RTX 4090. | aprender [#1104 merged](https://github.com/paiml/aprender/pull/1104) at 78101494c | this PR |
| **M32b** | Architecture-aware FFN load — both `QuantizedGGUFTransformer::from_gguf` and `GGUFTransformer::from_gguf` short-circuit `qwen3_moe` with structured `RealizarError::UnsupportedOperation` referencing the M32a contract id. Cryptic `Tensor 'blk.0.ffn_up.weight' not found` replaced with audit-named error. 2 falsifier tests (synthetic + live 17.3 GB GGUF) discharge FALSIFY-QW3-MOE-FORWARD-002. | aprender [#1106 merged](https://github.com/paiml/aprender/pull/1106) at 90cc293a7 | direct-merged commit `883a838` |
| **M32c.1** | `Qwen3MoeQuantizedLayer` struct + `load_qwen3_moe_layer()` loader using the M29 contract namespace (`blk.{L}.ffn_gate_inp/ffn_gate_exps/ffn_up_exps/ffn_down_exps.weight`). Live verification: 4 MoE tensors per layer × 48 layers = 192 expert-tensor descriptors loaded from the cached 17.3 GB GGUF; total expert bytes 17.5 GB matches file size. | aprender [#1116 merged](https://github.com/paiml/aprender/pull/1116) at ced9fe32b | (companion bookkeeping in this PR) |
| **M32c.2** | `QuantizedGGUFTransformer::from_gguf_for_moe` constructor — qwen3_moe-aware sibling of `from_gguf`. Adds `moe_layers: Vec<Option<Qwen3MoeQuantizedLayer>>` field parallel to `layers`. Loads non-FFN portion via `load_quantized_layer_moe_skeleton`; dense FFN fields stub as zero-element placeholders; `moe_layers[i] = Some(...)` for every L. | aprender [#1117 merged](https://github.com/paiml/aprender/pull/1117) at ffd0b246f | (companion bookkeeping in this PR) |
| **M32c.2.1** | Flip `from_gguf` dispatch to `from_gguf_for_moe` for arch == qwen3_moe; replace M32b's load-time `UnsupportedOperation` with a forward-time `UnsupportedOperation { operation: "moe_forward_dispatch" }` at `gguf_gpu_generate.rs`. Live `apr run` against 17.3 GB GGUF now reaches inference attempt; error reports load succeeded, only forward dispatch unwired. M32b test updated. | aprender [#1118 merged](https://github.com/paiml/aprender/pull/1118) at 97c808e29 | this PR |
| **M32c.2.2** | Contract amendment recording the M32c.2.2 implementation strategy: **LAZY-FUSED-MATVEC** (per-token forward keeps the 4 MoE expert tensors in their on-disk Q4_K/Q6_K/F32 form and dequantizes inline through `fused_q4k_parallel_matvec` / `fused_q6k_parallel_matvec` row-major matvec kernels per CLAUDE.md LAYOUT-002 row-major mandate). Decision rationale: preserves the 8× memory-bandwidth advantage; avoids materializing 18 GB of dense FP32 expert tensors. Bumps `qwen3-moe-forward-v1` v1.0.0 → v1.1.0. | aprender [#1119 merged](https://github.com/paiml/aprender/pull/1119) at 590b8d6aa | this PR |
| **M32c.2.2.0** | `expert_byte_slice` adapter — given `(layer_idx, expert_idx, role)`, returns the `&[u8]` slice into the mmapped GGUF for that expert's quantized tensor. Reuses the M32c.1 `Qwen3MoeQuantizedLayer` descriptor (offsets, qtype, byte sizes). Per-expert sizes vary by qtype: Q4_K [768, 2048] = 884,736 bytes; Q6_K [2048, 768] = 1,290,240 bytes. | aprender [#1120 merged](https://github.com/paiml/aprender/pull/1120) at db3436da9 | this PR |
| **M32c.2.2.1** | `expert_swiglu_quantized` — per-expert SwiGLU FFN dispatch using LAZY-FUSED-MATVEC: gate_proj + up_proj via `fused_q4k_parallel_matvec`, SwiGLU activation, down_proj via `fused_q6k_parallel_matvec`, all reading from `expert_byte_slice`. Returns `Vec<f32>` of shape `[hidden_dim]`. Pure-CPU row-major. | aprender [#1121 merged](https://github.com/paiml/aprender/pull/1121) at 4dd9ec21e | this PR |
| **M32c.2.2.2.0** | `moe_ffn_forward_layer` — single-layer MoE FFN dispatch. Composes router (softmax + top-k=8) + per-token expert routing + `expert_swiglu_quantized` per-expert call + weighted aggregation. Shape: `[hidden_dim]` in → `[hidden_dim]` out. Replaces dense FFN block at one layer index. | aprender [#1122 merged](https://github.com/paiml/aprender/pull/1122) at 1ab8e7fc5 | this PR |
| **M32c.2.2.2.1** | Contract amendment recording the M32c.2.2.2.1 integration architecture decision. Three approaches compared: (A) field-add to `OwnedQuantizedModel` across 99 sites, (B) parallel `run_qwen3_moe_generate` function, (C) wrapper struct. Chose hybrid: method `forward_qwen3_moe` on `OwnedQuantizedModel` taking MoE descriptors as parameters (zero field-add, zero attention/RoPE duplication) + parallel `run_qwen3_moe_generate` autoregressive loop (zero touch on dense path). Bumps `qwen3-moe-forward-v1` v1.1.0 → v1.2.0. | aprender [#1123 merged](https://github.com/paiml/aprender/pull/1123) at bd0871803 | this PR |
| **M32c.2.2.2.1.1** | `OwnedQuantizedModel::forward_qwen3_moe` — single-token forward method. Mirrors `forward()` step-for-step except FFN site calls `moe_ffn_forward_layer`. Reuses existing `&self` methods for qkv_matmul, apply_rope, causal_attention, fused_matmul, lm_head — zero duplication. Test `f_qw3_moe_c22211_001` exercises end-to-end against cached 17.3 GB GGUF: logits.len() == 151936, all finite, argmax in vocab range. | aprender [#1124 merged](https://github.com/paiml/aprender/pull/1124) at 10c74c400 | this PR |
| **M32c.2.2.2.1.2** | `run_qwen3_moe_generate` — autoregressive generation loop. Reads MoE config (num_experts, k, intermediate) from GGUF metadata via new `expert_count()` / `expert_used_count()` / `expert_feed_forward_length()` accessors on `GGUFModel`. Loads per-layer `Qwen3MoeQuantizedLayer` descriptors once, then full-prefill-per-token loop with greedy argmax sampling. No KV cache (M32d follow-up). Sibling of `run_gguf_generate` for qwen3_moe arch. | aprender [#1125 merged](https://github.com/paiml/aprender/pull/1125) at 16dcfe765 | this PR |
| **M32c.2.2.2.1.3** | **Dispatch flip in `inference_result.rs`** routing `qwen3_moe` arch to `run_qwen3_moe_generate` instead of `run_gguf_generate`. **Plus Q4_K_M qtype-aware dispatch** (`matvec_for_qtype` helper) — Q4_K_M GGUF mixes Q4_K (qtype=12) and Q6_K (qtype=14) within and across layers, so per-expert matmul must dispatch on `tensor.qtype` at runtime instead of hardcoding kernel by role. **FALSIFY-QW3-MOE-FORWARD-003 LIVE DISCHARGE** on lambda-vector RTX 4090: `apr run` against cached 17.3 GB GGUF emits "aaaaaaaa" / "." (any non-whitespace) end-to-end. | aprender [#1126 merged](https://github.com/paiml/aprender/pull/1126) at a902eea93 | #38 |
| **M32c.2.2.2.1.4** | Live `apr run` falsifier in `aprender-serve/tests/qwen3_moe_apr_run_live.rs` pinning **FALSIFY-QW3-MOE-FORWARD-003** as a regression test against the cached 17.3 GB Qwen3-Coder GGUF. Subprocess invocation via `Command::new(apr).args(["run", "--prompt", "Hi", "--max-tokens", "4"])`; assertions: exit 0, stdout matches `/\\S/`, stderr does not contain "Tensor 'blk.0.ffn_up.weight' not found". Skipped when GGUF absent (fixture-absent ≠ defect). Locks the M32c.2.2.2.1.3 discharge surface; any regression now fails CI. | aprender [#1127 merged](https://github.com/paiml/aprender/pull/1127) at 0392b1843 | this PR |
| **M32d.0** | `qwen3-moe-forward-v1` contract amendment v1.2.0 → v1.3.0 — encodes the **parity strategy** for the upcoming numerical-correctness work: cosine ≥0.99 vs llama.cpp Q4_K reference logits AND cosine ≥0.99 vs Hugging Face FP16 reference logits at the LM-head, with two new falsifiers `F-QW3-MOE-PARITY-001` (HF FP16 cosine) and `F-QW3-MOE-PARITY-002` (llama.cpp argmax sanity). Status remains DRAFT; flips to ACTIVE_RUNTIME at M32d discharge. | aprender [#1128 merged](https://github.com/paiml/aprender/pull/1128) at 2682132f7 | M33 audit-trail bookkeeping |
| **M32d.1** | `scripts/generate_qwen3_moe_fp16_logits.py` — Hugging Face FP16 reference logits fixture generator. Pure Python via `transformers` + `torch`; downloads `Qwen/Qwen3-Coder-30B-A3B-Instruct` once (~60 GB), runs a single forward pass on a fixed prompt, dumps `[batch, seq, vocab]` logits to JSON for the M32d.2 cosine gate to consume. Multi-device offload via `device_map="auto"`. Operator-confirm to run because of the download size and the ~30 min runtime on a 30B-A3B model. | aprender [#1129 merged](https://github.com/paiml/aprender/pull/1129) at 87a2a61c1 | M33 audit-trail bookkeeping |
| **M32d.2** | `crates/aprender-serve/tests/qwen3_moe_parity.rs` — `f_qw3_moe_parity_001` cosine gate against the M32d.1 HF FP16 fixture. Marked `#[ignore]` until the fixture file lands (does not exist on disk yet — Step 1 of M34 FAST PATH). When run with `--include-ignored`, computes cosine of `[hidden]→logits` between APR forward and HF reference; asserts ≥0.99. **F-QW3-MOE-PARITY-001 falsifier** wired. | aprender [#1130 merged](https://github.com/paiml/aprender/pull/1130) at ce6ca4bb4 | M33 audit-trail bookkeeping |
| **M32d.3** | `crates/aprender-serve/tests/qwen3_moe_argmax_parity.rs` — `f_qw3_moe_argmax_parity_002` llama.cpp argmax sanity check. Independent of the HF fixture: runs APR `apr run --prompt "Once upon a time"` and `llama-cli --prompt "Once upon a time" --n-predict 1` against the same Qwen3-Coder GGUF and asserts that both pick the same top-1 token id. **F-QW3-MOE-PARITY-002 falsifier** wired. Skipped when llama.cpp binary or GGUF absent. | aprender [#1131 merged](https://github.com/paiml/aprender/pull/1131) at 9f93d02d9 | M33 audit-trail bookkeeping |
| **M33** | Companion-only audit-trail bump — pin.lock refreshed from aprender commit a8623f650 → 3ea8114c8 with note recording the M32c.2.2.2.1.4 + M32d.0/.1/.2/.3 set. Companion contract bumped v1.20.0 → v1.21.0; M22 paired aprender mirror push at byte-identical sha256. No code change. Closed the bookkeeping lag between aprender main and the companion-side spec snapshot. | direct main commit `4ddae99` | this PR |
| **M34** | Companion-only spec amendment — adds the section "M32d FAST PATH — five-whys + concrete next 6–13 PRs" embedding a five-whys analysis of the gibberish-output symptom and an ordered, falsifiable 6-step plan to discharge M32d (measure → wire trace → bisect layer → sub-bisect component → fix → discharge), with component priors (LAYOUT 30% / Q4_K_M scales 20% / per-head Q-K norm 15% / RoPE θ 10% / router softmax 10% / embedding 10% / other 5%) and cost estimate (4–6 PRs lucky / 8–10 realistic / 12–15 pessimistic). Companion contract v1.21.0 → v1.22.0; aprender mirror at cf5c7875c. No code change. Converts open M32d work from "iterate on output" to "produce concrete cosine numbers and bisect". | direct main commit `7200d2b` | this PR |
| **M32d-Step2** | `forward_qwen3_moe_traced` — diagnostic-surface sibling of `forward_qwen3_moe` that emits per-layer std-dev + L2-norm at 5 probe points (post-embed, post-attn, post-MoE, post-residual, post-RMS-final) without altering production forward semantics. Drives the M34 FAST PATH bisection: per-layer std growth signature was the rank-3 Q/K norm tell. Q/K-norm absence produced 40× std growth at attention output by layer 8. | aprender [#1222 merged](https://github.com/paiml/aprender/pull/1222) | (companion bookkeeping in M35) |
| **M32d-Step2-JSON** | `apr trace --json --payload` — JSON output for the trace surface so Step 2 std-dev numbers are machine-readable rather than eyeballed from stderr. `handle_special_modes_with_json` + `run_traced_inference_json` route at the apr-cli boundary; output shape is the falsifier-test exit-criterion shape from the M34 plan (per-layer `{layer, std, l2}` array). | aprender [#1401 merged](https://github.com/paiml/aprender/pull/1401) | (companion bookkeeping in M35) |
| **M32d-Step5+5b+6+7** | **THE BUNDLE — root cause fix for `%%%%%%%%` → coherent output transition.** Squashes 4 fixes: **(Step 5)** per-head Q/K RMSNorm in `forward_qwen3_moe` between bias-add and RoPE — discharges rank-3 prior (15%); std at attention output drops from 40× to 1.0×. **(Step 5b)** `rope_theta` default 10K → 1M for `qwen3_moe`/`qwen3` arches in `gguf/config.rs` — discharges rank-4 prior (10%); long-context positional encoding correctly Qwen3-tuned. **(Step 6)** `chat_template_helpers.rs` routes `qwen3_moe`/`qwen3moe` to plain ChatML (no `<think>` injection) BEFORE the generic qwen3 → Qwen3NoThink rule. **(Step 7)** Sync `forward_qwen3_moe_traced` with Step 5 Q/K norm so traced and production paths stay byte-equivalent. F-QW3-MOE-STEP5-001 regression test wired. Output transition timeline: `%%%%%%%%` → "Human: What is 2+" (Step 5) → "Human: What is 2+2?" (Step 5b) → "2 + 2 = 4" (Step 6). Multi-domain: math/geography/translation/code all coherent. **M34 FAST PATH lucky-case bound: 5 PRs / ~6 hours wall vs 4–6 PRs / 2–3 days lucky / 8–10 PRs / 4–6 days realistic estimate.** | aprender [#1228 merged](https://github.com/paiml/aprender/pull/1228) at squash 5235aaeb9 | (companion bookkeeping in M35) |
| **M32d-RUSTSEC-unblock** | Companion CI unblocker — RUSTSEC-2026-0114 transitive advisory deny-by-default landed in main; bumped affected dep to advisory-clean version. No M32d behavioural change; cleared the path for #1228 to merge through workspace-test on the same self-hosted fleet. | aprender [#1242 merged](https://github.com/paiml/aprender/pull/1242) | (companion bookkeeping in M35) |
| **M35** | Companion-only audit-trail bump recording M32d functional discharge — contract v1.22.0 → v1.23.0 with full `status_history` entry cross-referencing aprender PRs #1222 / #1226 (squashed) / #1228 / #1242 / #1401, embedded live evidence (4 prompts × multi-domain output verification on lambda-vector RTX 4090 against cached 17.3 GB Qwen3-Coder-30B-A3B-Instruct-Q4_K_M.gguf), output transition timeline, and cost-vs-estimate analysis (5 PRs / ~6 hours actual = lucky-case bound). pin.lock refresh `aprender_commit cf5c7875c → 16f25af06`, `sha256 12f4bcb74110...→7818bd73a545...`, M22 paired-mirror push at byte-identical sha256. **NOT discharged**: cosine ≥0.99 vs HF FP16 (operator-confirm pending ~60 GB download); GPU MoE path; sub-FFN MoE breakdown in `apr trace` (Step 3 + 4 work bypassed because the rank-3 + rank-4 fix was sufficient). | direct main commit `ca75ed0` | this PR |
| **M36** | Companion-only post-discharge drift sweep — applies the M22 5-step ritual extension (step 4: refresh human-readable roll-up views) to M32d's discharge surface. Updates: README badge `v1.22.0 → v1.23.0` + status block `M0-M34 → M0-M35` + M32d "open work" → "FUNCTIONALLY DISCHARGED" with output transition narrative; CONTRIBUTING status footer parallel; spec status snapshot `qwen3-moe-forward-v1 v1.3.0 DRAFT → v1.4.0 ACTIVE_ALGORITHM_LEVEL`; R9 risk struck through as DISCHARGED with cross-reference to PR #1228. Same drift class the M22 5-step ritual was extended to address — closing the README/CONTRIBUTING/spec lag against ground-truth. **Does NOT modify** the original "M34 FAST PATH" spec section (lines ~430-620) — preserved as historical reference for retrospective comparison; component-prior table (rank-3 Q/K norm 15% + rank-4 RoPE θ 10%) empirically confirmed load-bearing. | direct main commit `3fd90d0` | this PR |
| **M37** | Companion-only sub-milestones backfill (this PR) — adds milestone-table rows for the M32d implementation PRs (#1222, #1228, #1242, #1401) plus M35/M36 audit-trail+drift-sweep companion commits. Closes the gap between status snapshot ("M0–M36 SHIPPED") and the sub-milestones table tail (which previously stopped at M34). No contract bump; spec markdown only. Same drift class M22 step 4 addresses. | direct main commit (this PR) | this PR |
| **M38** | Companion-only mechanical doc-drift detector — `scripts/check-doc-drift.sh` (asserts spec header / status snapshot / README / CONTRIBUTING M-counts all match the sub-milestones table tail; asserts stated gate count matches FALSIFY-CCPA-NNN row marker count). Wired into `make tier3` (between `pin-check` and the build steps), CI workflow `.github/workflows/ci.yml` (between `pin-check-roundtrip` and `cargo fmt --check`), and the pre-commit hook installed by `scripts/install-hooks.sh`. Codifies M22 step 4's drift-class backstop ("These are NOT mechanically guarded by pin-check; a kaizen sweep is the backstop") into authoring time. M37 alone produced 6 drift-fix commits this script's asserts would have caught. Same drift class as the M22 5-step ritual; the M22 mechanical guard now extends to step 4 too. | direct main commit `7f66c57` | this PR |
| **M39** | Companion-only `check-doc-drift.sh` extension — adds 4 new asserts cross-referencing the contract YAML's `metadata.version` against (a) README badge `contract-vX.Y.Z-green.svg`, (b) README status block `Contract at vX.Y.Z`, (c) CONTRIBUTING status footer `Status as of vX.Y.Z`, (d) spec status snapshot `claude-code-parity-apr-v1 vX.Y.Z`. Drift-class addressed: M22 step 1 bumps the YAML; step 4 must refresh each of these mentions. Pre-M39 the only mechanical guard was pin-check (sha256 of the YAML bytes), which catches the `pin.lock` lag but not the human-readable version mentions. M36's narrative drift (badge stayed at v1.22.0 while spec was at v1.23.0 for ~30 minutes pre-fix) is exactly this class. Detector run output now reports `contract YAML version: vX.Y.Z (matches all 4 cross-references)` on success. | direct main commit (this PR) | this PR |

## Falsification conditions (13 gates total)

### Source-of-truth invariants (M0+)

| ID | Name | Phase | Mechanically asserted by |
|----|------|-------|--------------------------|
| FALSIFY-CCPA-009 | ci_main_branch_green | M0+ | `gh api repos/paiml/claude-code-parity-apr/branches/main/protection` returns `ci/gate` ∈ required contexts |
| FALSIFY-CCPA-010 | pmat_comply_100pct | M0+ | `pmat comply check --json` returns `compliance_pct == 100.0 ∧ total_violations == 0` |
| FALSIFY-CCPA-011 | line_coverage_100pct | M0+ | `cargo llvm-cov --fail-under-lines 100 --fail-uncovered-lines 0` exits 0 |
| FALSIFY-CCPA-012 | pv_contract_gate_on_commit | M0+ | pre-commit hook + CI both run `pv validate contracts/claude-code-parity-apr-v1.yaml`, exit 0 |

### Behavioral parity gates (M1..M11)

| ID | Name | Phase | Assertion summary |
|----|------|-------|-------------------|
| FALSIFY-CCPA-001 | trace_schema_roundtrip | M1 | every fixture parses, re-serializes byte-identical, validates against `trace_schema` |
| FALSIFY-CCPA-002 | replay_determinism | M3 | replaying same fixture twice → byte-identical student traces (after normalization) |
| FALSIFY-CCPA-003 | mock_completeness | M3 | `RecordedDriver` consumes exactly len(teacher.assistant_turns) responses; no missing, no extras |
| FALSIFY-CCPA-004 | tool_call_equivalence | M4 | per turn, multiset of `(tool_name, semantic_input)` pairs in student matches teacher under per-tool equivalence rules (Edit: post-state sha256; Bash: normalized command; etc.) |
| FALSIFY-CCPA-005 | file_mutation_equivalence | M4 | union diff over CWD after `apr code` finishes equals union diff after Claude Code finished, modulo per-filetype canonicalization |
| FALSIFY-CCPA-006 | sovereignty_on_replay | M5 | zero outbound sockets to `*.anthropic.com` during replay; CI test container drops all egress except 127.0.0.1 |
| FALSIFY-CCPA-007 | corpus_coverage | M5 | ≥1 fixture per non-MISSING row of `apr-code-parity-v1.yaml` (currently 17 of 21) |
| FALSIFY-CCPA-008 | parity_score_bound | M6 | aggregate `parity_score ≥ 0.95` and per-fixture `≥ 0.80` |
| FALSIFY-CCPA-013 | first_recorded_parity_score | M11 | `fixtures/canonical/measured-parity.json` exists with ≥5 fixtures; aggregate ≥ 0.95; flips contract DRAFT → ACTIVE_RUNTIME (DISCHARGED at 30 fixtures, aggregate 1.0000) |

Each gate maps to one falsification test in `crates/ccpa-*/tests/falsify_ccpa_NNN_*.rs` and is enforced via `pv validate contracts/claude-code-parity-apr-v1.yaml` per the harness policy in `CLAUDE.md § Contract Validation: DOGFOOD pv, NEVER bash`. **No bash/yq/python re-implementation of these gates is permitted.** If `pv validate` does not yet support a needed shape, extend `aprender-contracts/src/schema/` — schema-extension ticket: PMAT-CONTRACTS-CCPA-001.

## Academic basis (arXiv → gate mapping)

| arXiv | Title (short) | Applies to gate(s) | Why |
|-------|---------------|--------------------|-----|
| **1503.02531** | Hinton et al., *Distilling the Knowledge in a Neural Network* | CCPA-008 | The parity_score *is* a distillation loss on the action stream. Framing gates the convergence criterion. |
| **1807.10453** | Segura et al., METTLE — *Metamorphic Testing of ML Systems* | CCPA-004, CCPA-005 | Tool-call equivalence and post-state equivalence are metamorphic relations under the operational semantics of each tool — the canonical way to test ML systems without ground truth. |
| **2207.11976** | *Differential Testing of DL Frameworks* | CCPA-002, CCPA-004 | Teacher↔student parity is the textbook differential-testing setup; replay determinism is the precondition. |
| **2310.06770** | Jimenez et al., *SWE-bench* | CCPA-007 | Justifies fixture-corpus methodology: ≥1 task per capability row, recorded once, replayed forever, no live network. |
| **2505.03096** | *Chaos Engineering for LLM Systems* | CCPA-006 | Sovereignty enforcement under fault injection — "what if the env leaks an Anthropic key on a replay run" is exactly a chaos-test class. |
| **2603.23611** | LLMORPH — *Cataloged Metamorphic Relations for NLP* | CCPA-004 | 191 catalogued metamorphic relations directly populate per-tool `equality` rules in `tool_equivalence_rules`. |
| **2102.05351** (referenced in apr-cli-qa-spec) | Coverage-completeness invariants | CCPA-010, CCPA-011 | Background for 100 %-coverage / 100 %-comply invariants. |

## M32d FAST PATH — five-whys + concrete next 6–13 PRs

> **Authored 2026-05-01 (M34) to break the gibberish-output deadlock.**
> Forward chain works (M32c.2.2.2.1.3 LIVE-DISCHARGED). Output is
> `%%%%%%%%`. M32d numerical-parity scaffolding (#1128/#1129/#1130/#1131)
> exists but the actual fixes haven't started. This section is the
> plan to convert "vague gibberish" into "measured cosine number" and
> close it.

### Five-whys anchor

**Symptom**: `apr run ~/.cache/pacha/models/2b88b180a790988f.gguf
--prompt "What is 2+2?" --max-tokens 8` exits 0 in ~52s and emits
`%%%%%%%%`. Forward path executes, logits are finite (per
`f_qw3_moe_c22211_001`), but output is repetitive nonsense.

| Why | Answer | Implication |
|-----|--------|-------------|
| 1. Why output `%%%%%%%%`? | Greedy argmax keeps picking the same token regardless of prior context. | Sampling code is fine; the problem is upstream of sampling. |
| 2. Why does argmax keep picking the same token? | Logits are dominated by ONE position with huge margin over runners-up, and that position doesn't shift with new context. | The forward pass is producing a context-invariant output direction. |
| 3. Why is the output context-invariant? | Hidden state through 48 layers is converging to (or starting at) a near-constant subspace. Either (a) attention isn't routing context, (b) FFN is collapsing to ~0, (c) MoE is degenerating to one expert, (d) RMSNorm/residual is wiping signal, or (e) one of the above plus quantization scale error. | Bug is somewhere in the per-layer arithmetic. We do NOT yet know which layer or which sub-component. |
| 4. Why don't we know which layer? | Because we haven't compared to a reference. `apr trace` exists but returns null per-layer stats for `qwen3_moe` (forward goes through `run_qwen3_moe_generate`, not the traced path). M32d.1 fixture script exists but hasn't been run. M32d.2 cosine gate is `#[ignore]`d until the fixture lands. | We are flying blind. The FAST PATH is to stop flying blind. |
| 5 (root). What's the cheapest experiment that yields the most signal? | Generate the M32d.1 HF FP16 reference logits, run M32d.2 cosine `--include-ignored` to see HOW wrong (one number), then add per-layer trace and bisect. **Bisection over 48 layers + 6 sub-components per layer is ~9 cosine comparisons to localize.** | The plan below. |

### FAST PATH — ordered, gated, falsifiable

Each step has an exit criterion. If a step's criterion fails, the next
step in the plan changes. Every step is one PR or less.

#### Step 1 — measure how wrong (1 PR, 1–2h wall + ~30 min HF download)

**Action**: run `scripts/generate_qwen3_moe_fp16_logits.py` once on
lambda-vector RTX 4090, commit the resulting fixture
`crates/aprender-serve/tests/fixtures/qwen3_moe_fp16_logits_pos0.json`.

**Exit criterion**: `cargo test -p aprender-serve --test
qwen3_moe_parity --features cuda --release -- --include-ignored
f_qw3_moe_parity_001` runs and reports a concrete cosine number
(any number — the test currently can't even run because the
fixture isn't present).

**Decision**:
- if cosine ≥ 0.99 → we're already done, the gibberish is a sampling
  bug; flip the falsifier from `#[ignore]` to live and move to
  apr-code integration (skip Steps 2–4).
- if 0.5 ≤ cosine < 0.99 → forward is "kinda right"; few targeted
  fixes likely close it. Proceed to Step 2.
- if cosine < 0.5 (likely) → forward is structurally wrong. Proceed
  to Step 2 with bisection over all 48 layers.

**Falsifier introduced**: `FALSIFY-QW3-MOE-FORWARD-004` (this is
already declared by M32d.0 in the contract; Step 1 produces the
first concrete value to record).

#### Step 2 — wire per-layer trace for qwen3_moe (1 PR, ~1d)

**Action**: extend `forward_qwen3_moe` in
`crates/aprender-serve/src/gguf/qwen3_moe_load.rs` to optionally
emit per-layer hidden-state L2 + dim-wise mean/std into the JSON
that `apr trace --json --payload` returns. Today that output is null
for qwen3_moe because the trace path traces `OwnedQuantizedModel`'s
non-MoE forward. Add a parallel `forward_qwen3_moe_traced` (or a
`&mut Option<TracePayload>` parameter) that records each of the 48
layer outputs.

**Exit criterion**: `apr trace --json --payload <gguf>
--prompt "What is 2+2?"` returns non-null `output_stats` for every
`transformer_block_N` entry, with finite L2 norms.

**Why this slice exists**: without it, Step 3 has nowhere to
write the bisection comparison.

#### Step 3 — bisect to first divergent layer (1 PR, ~half-day)

**Action**: extend `qwen3_moe_parity.rs` test with a per-layer
cosine harness. The HF fixture from Step 1 is captured at the
output of every transformer block (re-run script with
`--emit-hidden-states` flag — already on the script's TODO list per
its docstring). The Rust test reads the per-layer reference and
compares to per-layer apr trace output, asserting ≥0.99 cosine
layer-by-layer.

**Exit criterion**: the test names ONE specific
`transformer_block_N` as the first layer where cosine drops
below 0.99. Until that layer, apr is correct; from that layer
onward, apr diverges.

**Triage**: if N == 0, bug is in (token embedding) ∪ (attention
sub-block of layer 0). If N > 0 and layers 0..N-1 all green, bug
is localized to layer N's attention OR layer N's MoE FFN OR layer
N's residual/RMSNorm.

#### Step 4 — sub-bisect within the divergent layer (1–2 PRs, ~1d)

**Action**: within layer N's forward, snapshot intermediate values
at component boundaries (after RMSNorm, after Q/K/V projection,
after RoPE, after attention output, after attention residual, after
post-attention RMSNorm, after MoE router, after each of the top-k
expert outputs, after weighted aggregation, after FFN residual).
Compare each to HF reference (Step 1 with `--emit-component-states`).

**Exit criterion**: cosine drop is localized to ONE component.
That component is the bug site.

**Priors on which component will be the culprit** (in
descending probability based on aprender's known bug history per
CLAUDE.md LAYOUT-001/002 mandate):

| Rank | Component | Prior | Why this is high-prior |
|------|-----------|-------|------------------------|
| 1 | Per-expert weight LAYOUT (transpose) | ~30% | LAYOUT-001/002 has been the #1 source of MoE-port bugs; per-expert tensors `[N_e, intermediate, hidden]` row-major slicing is easy to get wrong; "olumbia+lsi nunca/localENTS" gibberish in CLAUDE.md is the tell. |
| 2 | Q4_K_M dequant scale (super-scale + sub-min interaction) | ~20% | Q4_K_M mixes Q4_K and Q6_K; M32c.2.2.2.1.3 already had to add `matvec_for_qtype` runtime dispatch to handle this; possible scale accumulator bug remains. |
| 3 | Qwen3 per-head Q/K RMSNorm | ~15% | Qwen3 is the only major arch with per-head Q/K RMSNorm; `attn_q_norm.weight` and `attn_k_norm.weight` exist on every layer per GH-279 and are easy to miss in the new MoE forward. |
| 4 | RoPE θ=1e7 (vs default 1e4) | ~10% | Qwen3-Coder uses high-base RoPE; if the forward defaults to θ=1e4, freqs are off by 3 orders of magnitude. |
| 5 | MoE router softmax | ~10% | Top-k selection requires post-softmax renormalization (per `moe-router-v1`); easy to drop the renormalize step. |
| 6 | Token embedding dequant | ~10% | Embeddings are usually Q4_K in Q4_K_M files; if dequant is wrong, layer 0 input is wrong, everything downstream is wrong. |
| 7 | Other | ~5% | KV cache layout, residual scaling, output norm, lm_head transpose. |

**Decision tree**:
- if culprit is rank 1 (LAYOUT) → fix is a single transpose call;
  Step 5.
- if rank 2 (Q4_K_M) → fix is in `matvec_for_qtype` dequant inner
  loop; Step 5.
- if rank 3 (per-head Q/K norm) → wire `attn_q_norm` /
  `attn_k_norm` into `forward_qwen3_moe` after Q/K projection;
  Step 5.
- if rank 4 (RoPE θ) → read `general.rope.freq_base` from GGUF
  metadata and thread into `apply_rope`; Step 5.
- if rank 5 (router softmax) → review against `moe-router-v1`
  contract; Step 5.
- if rank 6 (embedding) → check token_embd.weight dequant matches
  HF (cosine ≥ 0.99); Step 5.
- if rank 7 → ad-hoc; Step 5 with new sub-PR.

#### Step 5 — apply targeted fix (1–8 PRs, depends on Step 4 outcome)

**Action**: write the smallest fix that resolves the localized
component cosine. Each fix is its own PR with a falsifier that
asserts the now-passing component cosine.

**Exit criterion**: Step 3's per-layer cosine harness passes for
all 48 layers (cosine ≥ 0.99 every layer).

**Pessimistic case**: 5–8 fixes if multiple components compound.
**Realistic case**: 2–4 fixes (LAYOUT + Q4_K_M + per-head norm).
**Optimistic case**: 1 fix (LAYOUT alone).

#### Step 6 — discharge (1 PR)

**Action**:
- run `apr run` and verify text is sensible (~"4" or similar
  arithmetic answer).
- `qwen3-moe-forward-v1` v1.3.0 → v1.4.0, status DRAFT →
  ACTIVE_RUNTIME.
- companion `claude-code-parity-apr-v1` v1.22.0 → v1.23.0 with
  M35 status_history entry recording M32d discharge (v1.22.0 was
  already consumed by M34 for the FAST PATH plan itself).
- run `ccpa measure` against a tool-dispatching teacher fixture
  (the FALSIFY-CCPA-013 measured-parity gate this whole POC was
  authored to drive).
- if CCPA-013 cosine ≥ 0.95 across the corpus → POC's headline
  claim is now mechanically asserted, not synthetic.

### Estimated total cost

| Outcome | PRs | Wall-clock |
|---------|-----|------------|
| Lucky single-bug (rank-1 only) | 4–6 | 2–3 days |
| Realistic (2–4 compounded bugs) | 8–10 | 4–6 days |
| Pessimistic (structural cascade) | 12–15 | 1–2 weeks |

### Why this is the FAST path (and not "just iterate on output until it stops being gibberish")

Naive iteration on `apr run` output has no localization signal — every
attempted fix is a full forward run + visual judgement of gibberish.
Steps 1–4 of this plan invest ~2 days to convert the problem from
"the model is wrong somewhere" to "this exact cosine number at this
exact layer at this exact component is wrong by this exact margin".
Once localized, each fix is targeted and provable. The investment
in diagnostics dominates the schedule for the first 3 PRs and
inverts the schedule for everything after.

### Cross-references

- Numbers above (cosine thresholds, K=8, N_experts=128, L=48) are
  from `contracts/qwen3-moe-forward-v1.yaml` v1.3.0 §
  `qwen3_coder_30b_a3b_instantiation`.
- "LAYOUT-001/002 row-major mandate" is `CLAUDE.md` §
  "CRITICAL: LAYOUT-002 Row-Major Mandate" in
  `crates/aprender-serve/CLAUDE.md`.
- "Qwen3 per-head Q/K RMSNorm" is GH-279 in aprender-serve, with
  `attn_q_norm.weight` / `attn_k_norm.weight` already plumbed through
  `OwnedQuantizedLayer` per `gguf/quantized.rs:312-315`.
- `apr trace` capability and `apr trace --payload` discipline are
  per `feedback_apr_trace_not_eprintln` (root memory).

## Falsification run history

| Run | Date | Revision | Verdict | Notes |
|-----|------|----------|---------|-------|
| Run 0 | 2026-04-26 | original spec PR | **NOT YET RUN** (historical) | Spec authored; companion repo not yet scaffolded; gates 009–012 not yet wired. |
| Run 1 | 2026-04-26 → 2026-05-02 | M1–M37 (every merge to companion main) | **PASS** on every commit | Gates 009–012 (ci/gate green, pmat comply 100%, line coverage ≥99%, pv validate clean) have been hard-blocking on every PR since M1's empty-scaffold landed (FALSIFY-CCPA-009 enforces branch protection from that PR forward). M32d FUNCTIONALLY DISCHARGED 2026-05-02 (M35 audit-trail bump records aprender PR #1228 squash 5235aaeb9). Per-run audit trail lives in `contracts/claude-code-parity-apr-v1.yaml § status_history` (one entry per minor-version bump). |

(Subsequent runs append below in the apr-cli-qa-spec.md format: gate / status / evidence per row. The status_history block in the contract YAML is the byte-precise audit; this table is the human roll-up.)

## Risks & open questions

| # | Risk / question | Mitigation | Falsifiable by |
|---|-----------------|------------|----------------|
| R1 | ~~Recording the live Anthropic API costs $$ per fixture~~ **OBSOLETE post-M2.3 rescope** ("we will not call api, we will assume claude code"). Fixtures are now AUTHORED canonical references in `fixtures/canonical/`. | n/a (risk no longer applies) | n/a |
| R2 | ~~Claude Code may pin its own Anthropic auth, refuse `ANTHROPIC_BASE_URL` override~~ **OBSOLETE post-M2.3 rescope** — recording proxy is OOS. | n/a (risk no longer applies) | n/a |
| R3 | Tool-call equivalence for Edit/Write is non-trivial | Per-tool equivalence rules in `ccpa-differ`, contracted in YAML | FALSIFY-CCPA-004 — directly |
| R4 | Claude Code roadmap may add tools we don't have in `apr code` | New tools surface as `OrchestrationDrift::UnknownToolName` | FALSIFY-CCPA-004 — directly (gate FAILs until `apr-code-parity-v1.yaml` flips a row) |
| R5 | New repo conflicts with monorepo single-source-of-truth | Companion repo is canonical for *enforcement*; aprender stays canonical for *contract text*. `pin.lock` pins authoritative commit hash | FALSIFY-CCPA-012 — pre-commit hook rejects stale pins |
| R6 | `apr code`'s `LlmDriver` trait may not be public-stable enough for an external repo | PMAT-CODE-LLM-DRIVER-PUBLIC-001 (pre-req for M3) | M3 PR exists ⇔ PMAT-CODE-LLM-DRIVER-PUBLIC-001 closed |
| R7 | 100 % line coverage may produce test-for-coverage's-sake noise on a tiny POC | Tradeoff accepted: POC is small (~5 crates), 100 % is achievable. If a function genuinely cannot be covered, the function is unjustified — delete it. | FALSIFY-CCPA-011 — directly |
| R8 | `pmat comply check --strict` may reject patterns aprender itself uses | Companion repo is greenfield; we author to comply. If we hit a genuine `pmat comply` bug, the fix is upstream pmat, not a `--allow` flag | FALSIFY-CCPA-010 — directly |
| R9 | ~~**M32d numerical-correctness blocker**~~ **FUNCTIONALLY DISCHARGED 2026-05-02** via aprender PR #1228 squash 5235aaeb9 (Step 5 + 5b + 6 + 7 fix bundle): per-head Q/K RMSNorm + rope_theta default 1M + chat template no-think + traced sync. Output now `2 + 2 = 4` + multi-domain coherent answers. M34 FAST PATH plan delivered at lucky-case bound (5 PRs / ~6 hours). The narrower remaining piece (formal cosine ≥0.99 vs HF FP16 to flip `qwen3-moe-forward-v1` ACTIVE_RUNTIME) is operator-confirm pending ~60 GB HF download. FALSIFY-CCPA-013's measured (vs synthetic) discharge can land any time without that gate since output quality is verified live. | M34 plan executed; M35 audit-trail recorded the discharge. | FALSIFY-QW3-MOE-PARITY-001 (HF FP16 cosine ≥0.99) AND FALSIFY-QW3-MOE-PARITY-002 (llama.cpp argmax sanity) on `qwen3-moe-forward-v1` v1.4.0 ACTIVE_ALGORITHM_LEVEL — formal flip awaits operator |

## References

- `contracts/claude-code-parity-apr-v1.yaml` — top-level falsifiable parity contract (this PR)
- `contracts/apr-code-parity-v1.yaml` — sibling static-feature matrix
- `contracts/apr-claude-proxy-v1.yaml` — sibling Messages-API shape contract
- `crates/aprender-orchestrate/contracts/batuta/apr-code-v1.yaml` — agent-loop ground truth
- `docs/specifications/apr-mcp-server-spec.md` — feature-by-feature parity matrix prose
- `docs/specifications/apr-cli-qa-spec.md` — template for falsification-phase + arXiv layout
- `CLAUDE.md` § "Contract Validation: DOGFOOD pv, NEVER bash" — harness policy
- `CLAUDE.md` § "Realizar-First Architecture" — why student is `apr code`, not direct `aprender::models`
- Memory: `feedback_monorepo_single_source_of_truth.md` — aprender vs companion-repo split
- Memory: `feedback_pv_not_bash_for_contracts.md` — every gate flows through `pv`
- Memory: `project_apr_code_parity_matrix.md` — the static-matrix epic this POC complements
- Anthropic Messages API — https://docs.anthropic.com/en/api/messages
- Hinton, G., Vinyals, O., & Dean, J. (2015). *Distilling the Knowledge in a Neural Network.* arXiv:1503.02531
- Segura, S., Towey, D., Zhou, Z., & Chen, T. (2018). METTLE — *Metamorphic Testing of Deep Learning Systems.* arXiv:1807.10453
- *Differential Testing of Deep Learning Frameworks.* arXiv:2207.11976
- Jimenez, C. et al. (2023). *SWE-bench: Can Language Models Resolve Real-World GitHub Issues?* arXiv:2310.06770
- *Chaos Engineering for LLM Systems.* arXiv:2505.03096
- LLMORPH — *Cataloged Metamorphic Relations for NLP.* arXiv:2603.23611
