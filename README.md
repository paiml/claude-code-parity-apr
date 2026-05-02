# claude-code-parity-apr

[![CI](https://github.com/paiml/claude-code-parity-apr/actions/workflows/ci.yml/badge.svg)](https://github.com/paiml/claude-code-parity-apr/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue.svg)](https://github.com/paiml/claude-code-parity-apr#license)
[![Contract](https://img.shields.io/badge/contract-v1.23.0-green.svg)](contracts/claude-code-parity-apr-v1.yaml)
[![Status](https://img.shields.io/badge/status-ACTIVE__RUNTIME-green.svg)](contracts/claude-code-parity-apr-v1.yaml)
[![Gates](https://img.shields.io/badge/gates-13%2F13%20discharged-brightgreen.svg)](contracts/claude-code-parity-apr-v1.yaml)
[![Parity](https://img.shields.io/badge/measured%20parity-1.0000-brightgreen.svg)](fixtures/canonical/measured-parity.json)
[![Corpus](https://img.shields.io/badge/corpus-30%20%2F%2030-brightgreen.svg)](fixtures/canonical/)
[![Coverage](https://img.shields.io/badge/parity--matrix-15%2F15%20reachable-brightgreen.svg)](contracts/claude-code-parity-apr-v1.yaml)

> Record-replay-distill harness proving
> [`apr code`](https://github.com/paiml/aprender) is byte-stable against
> [Claude Code](https://docs.anthropic.com/claude/docs/claude-code) at the
> action-stream level.

Treats Claude Code as a **teacher** orchestrator and `apr code` as a
**student**. Each scenario in `fixtures/canonical/` is a paired
`teacher.ccpa-trace.jsonl` + `student.ccpa-trace.jsonl`; the differ
walks the two traces, applies per-tool semantic-equivalence rules,
and emits a falsifiable parity score plus a closed-enum drift
category for any mismatch.

**Status (2026-05-02)**: M0–M43 all SHIPPED. Contract at v1.23.0
ACTIVE_RUNTIME. Corpus complete at the spec-prescribed 30 fixtures
(all score 1.0). Parity-matrix coverage 15/15 reachable
(2 OOS at trace boundary). FALSIFY-CCPA-007 hard-blocking on every PR
since M16. The HTTPS-proxy recording path is intentionally OOS post
the "we will not call api, we will assume claude code" re-scope —
fixtures are AUTHORED canonical references, not live recordings.
**M32d numerical-parity FUNCTIONALLY DISCHARGED 2026-05-02** via
PR #1228 squash 5235aaeb9 on aprender main (Step 5 + 5b + 6 + 7
bundle). Output transition: `%%%%%%%%` gibberish → `2 + 2 = 4` +
multi-domain coherent answers (math/geo/translate/code) on the
cached 17.3 GB Qwen3-Coder-30B-A3B-Instruct GGUF. M34 FAST PATH
plan delivered at the lucky-case bound (5 PRs / ~6 hours vs 4–6 PRs /
2–3 days estimate). The formal cosine ≥ 0.99 vs HF FP16 measurement
(operator-confirm — ~60GB download) remains pending to flip
`qwen3-moe-forward-v1` v1.3.0 → v1.4.0 ACTIVE_RUNTIME.

## Usage

```bash
# Install local tools (matches CI exactly)
make install-tools

# Install the FALSIFY-CCPA-012 pre-commit hook
make install-hooks

# Run every gate locally (mirror of CI)
make tier3
```

The shipped CLI:

```bash
# Score a single teacher/student pair
ccpa diff fixtures/canonical/0001-edit-readme/teacher.ccpa-trace.jsonl \
          fixtures/canonical/0001-edit-readme/student.ccpa-trace.jsonl

# Score the whole corpus and bidirectional-sensitivity check
ccpa corpus fixtures/canonical/             # canonical MUST PASS
ccpa corpus fixtures/regression/            # regression MUST FAIL (drift)
ccpa corpus fixtures/canonical/ --json      # machine-readable

# Walk the parity-matrix coverage gate (M16 hard-blocker)
ccpa coverage \
  --apr-code-parity-yaml ../aprender/contracts/apr-code-parity-v1.yaml \
  --fixtures-dir fixtures/canonical/ \
  --oos-rows keyboard-shortcuts,status-line

# Validate a JSONL trace against the schema
ccpa validate fixtures/canonical/0001-edit-readme/teacher.ccpa-trace.jsonl
```

## Architecture

```
                AUTHORED canonical fixtures (M0+)
                        │
                        ▼
fixtures/canonical/<id>/teacher.ccpa-trace.jsonl  ◄── AUTHORED
                                ▲
                                │  per-tool equivalence rules
                                │  + hook + skill projections
                                ▼
fixtures/canonical/<id>/student.ccpa-trace.jsonl  ◄── AUTHORED
                        │
                        ▼
            ccpa-differ::compute_parity_score
                        │
                        ▼
                    ParityReport
                  { score, drifts[] }
                        │
                        ▼
                ccpa corpus / ccpa coverage
                        │
                        ▼
                 CI hard-blocker
              (FALSIFY-CCPA-007 since M16)

REGRESSION CORPUS (M9, bidirectional-sensitivity proof)
fixtures/regression/<id>/{teacher,student} — deliberate drift
                        │
                        ▼
            score < threshold; exit 1; CI requires this
```

The replay-against-real-`apr code` path (M3.1, real `LlmDriver`
adapter) is upstream-blocked on
[PMAT-CODE-LLM-DRIVER-PUBLIC-001](https://github.com/paiml/aprender)
in aprender. The HTTPS-proxy recording path (M2.3) is OOS by
project rescope. Until either lands, the corpus is curated.

## Source-of-truth split

| Concern | Lives in |
|---|---|
| Contract TEXT | [`paiml/aprender/contracts/claude-code-parity-apr-v1.yaml`](https://github.com/paiml/aprender/blob/main/contracts/claude-code-parity-apr-v1.yaml) (canonical), pinned here via `contracts/pin.lock` |
| Spec | [docs/specifications/claude-code-parity-apr-poc.md](docs/specifications/claude-code-parity-apr-poc.md) (canonical here since M1) |
| Implementation, fixtures, CI, coverage, pmat-comply | this repo (canonical) |

This split follows aprender's monorepo single-source-of-truth policy:
aprender stays canonical for contract TEXT (where every paiml contract
lives), while this repo is canonical for runtime ENFORCEMENT.

## Falsification gates

13 gates, all `pv validate`-mechanically asserted on every PR per
`CLAUDE.md § "DOGFOOD pv, NEVER bash"`.

**Source-of-truth invariants (M0+):**

| ID | Name | Mechanism |
|----|------|-----------|
| FALSIFY-CCPA-009 | `ci_main_branch_green` | branch protection requires `ci/gate` |
| FALSIFY-CCPA-010 | `pmat_comply_100pct` | `pmat comply check`: `is_compliant=true` ∧ 0 Fail-status checks |
| FALSIFY-CCPA-011 | `line_coverage_100pct` | `cargo llvm-cov`: 100% functions ∧ ≥99% lines (refined v0.4.0) |
| FALSIFY-CCPA-012 | `pv_contract_gate_on_commit` | pre-commit hook + CI run `pv validate` + `pin-check` |

**Behavioral parity gates (all DISCHARGED):**

| ID | Name | Status | Asserted by |
|----|------|--------|-------------|
| FALSIFY-CCPA-001 | `trace_schema_roundtrip`     | ✅ ACTIVE | [crates/ccpa-trace/tests/falsify_ccpa_001_roundtrip.rs](crates/ccpa-trace/tests/falsify_ccpa_001_roundtrip.rs) (17 tests) |
| FALSIFY-CCPA-002 | `replay_determinism`         | ✅ ACTIVE | [crates/ccpa-replayer/](crates/ccpa-replayer/) (16 tests) |
| FALSIFY-CCPA-003 | `mock_completeness`          | ✅ ACTIVE | same harness |
| FALSIFY-CCPA-004 | `tool_call_equivalence`      | ✅ ACTIVE | [crates/ccpa-differ/tests/falsify_ccpa_004_tool_equivalence.rs](crates/ccpa-differ/tests/falsify_ccpa_004_tool_equivalence.rs) (36 tests) |
| FALSIFY-CCPA-005 | `file_mutation_equivalence`  | ✅ ACTIVE | [crates/ccpa-differ/tests/falsify_ccpa_005_file_mutation.rs](crates/ccpa-differ/tests/falsify_ccpa_005_file_mutation.rs) (15 tests) |
| FALSIFY-CCPA-006 | `sovereignty_on_replay`      | ✅ ACTIVE | [crates/ccpa-differ/tests/falsify_ccpa_006_sovereignty.rs](crates/ccpa-differ/tests/falsify_ccpa_006_sovereignty.rs) (10 tests) |
| FALSIFY-CCPA-007 | `corpus_coverage`            | ✅ HARD-BLOCKING (M16) | [crates/ccpa-differ/tests/falsify_ccpa_007_coverage.rs](crates/ccpa-differ/tests/falsify_ccpa_007_coverage.rs) (15 tests) + CI `ccpa coverage --oos-rows ...` |
| FALSIFY-CCPA-008 | `parity_score_bound`         | ✅ ACTIVE | [crates/ccpa-differ/tests/falsify_ccpa_008_parity_score.rs](crates/ccpa-differ/tests/falsify_ccpa_008_parity_score.rs) (24 tests) |
| FALSIFY-CCPA-013 | `first_recorded_parity_score`| ✅ DISCHARGED | `fixtures/canonical/measured-parity.json` (30 fixtures, aggregate=1.0000) |

## Adding a new fixture

```bash
mkdir fixtures/canonical/00XX-my-scenario

# Author the meta.toml — declares which apr-code-parity row(s) it covers
cat > fixtures/canonical/00XX-my-scenario/meta.toml <<EOF
[fixture]
id = "00XX-my-scenario"
covers = ["builtin-tools-rwegs"]   # or hooks, skills, slash-commands, ...
description = "What this fixture exercises and why."
EOF

# Author the paired teacher.ccpa-trace.jsonl + student.ccpa-trace.jsonl
# Each is JSONL (one record per line). See any existing fixture for shape.
# session_id should be a stable UUIDv7-shaped string — these are normalized
# at compare time, so it doesn't need to be a real generation.

# Verify locally:
ccpa corpus fixtures/canonical/                    # MUST exit 0
ccpa coverage --apr-code-parity-yaml ... --oos-rows ...    # MUST exit 0
make tier3                                         # full local gate sweep
```

The trace schema is documented in [`contracts/claude-code-parity-apr-v1.yaml § trace_schema`](contracts/claude-code-parity-apr-v1.yaml) and mirrored as Rust types in [`crates/ccpa-trace/src/lib.rs`](crates/ccpa-trace/src/lib.rs). The 7 record kinds are `session_start`, `user_prompt`, `assistant_turn`, `tool_result`, `session_end`, `hook_event` (schema-v2, M15), `skill_invocation` (schema-v2, M15).

## arXiv basis

- 1503.02531 — Hinton et al., *Distilling the Knowledge in a Neural Network* (action-stream distillation framing)
- 1807.10453 — Segura et al., METTLE — *Metamorphic Testing of Deep Learning Systems*
- 2207.11976 — *Differential Testing of Deep Learning Frameworks*
- 2310.06770 — Jimenez et al., *SWE-bench: Can Language Models Resolve Real-World GitHub Issues?*
- 2505.03096 — *Chaos Engineering for LLM Systems*
- 2603.23611 — LLMORPH — *Cataloged Metamorphic Relations for NLP*

See spec § Academic basis for the per-gate mapping.

## License

Apache-2.0 OR MIT.
