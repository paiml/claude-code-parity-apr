# claude-code-parity-apr

[![CI](https://github.com/paiml/claude-code-parity-apr/actions/workflows/ci.yml/badge.svg)](https://github.com/paiml/claude-code-parity-apr/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue.svg)](https://github.com/paiml/claude-code-parity-apr#license)
[![Contract](https://img.shields.io/badge/contract-v1.0.0-green.svg)](contracts/claude-code-parity-apr-v1.yaml)
[![Status](https://img.shields.io/badge/status-ACTIVE%20M6-green.svg)](docs/specifications/claude-code-parity-apr-poc.md#phases--milestones)
[![Gates](https://img.shields.io/badge/gates-12%2F12%20online-green.svg)](contracts/claude-code-parity-apr-v1.yaml#L70)

> Record-replay-distill harness proving
> [`apr code`](https://github.com/paiml/aprender) is byte-stable against
> [Claude Code](https://docs.anthropic.com/claude/docs/claude-code) at the
> action-stream level.

Treats Claude Code as a **teacher** orchestrator and `apr code` as a
**student**. Captures the teacher's full action stream via a recording
HTTPS proxy at `ANTHROPIC_BASE_URL`, replays it deterministically against
`apr code` with mocked LLM responses, then `pv`-validates the diff. Output:
a falsifiable parity score per fixture and a closed-enum contract verdict
per gate.

**Repo status**: `M1` — scaffold landing, behavioral gates not yet online.

## Usage

```bash
# Install local tools (matches CI exactly)
make install-tools

# Install the FALSIFY-CCPA-012 pre-commit hook
make install-hooks

# Run every gate locally (mirror of CI)
make tier3
```

Once `M2` lands you'll be able to:

```bash
# Record a Claude Code session as a fixture
ccpa record --out fixtures/0001-edit-readme.ccpa-trace.jsonl
ANTHROPIC_BASE_URL=http://127.0.0.1:8443 claude   # session captured

# Replay it against apr code (M3)
ccpa replay fixtures/0001-edit-readme.ccpa-trace.jsonl

# Diff teacher vs student traces (M4)
ccpa diff fixtures/ --json
```

## Architecture

```
RECORD (teacher)              REPLAY (student)              DISTILL+DIFF

Claude Code                   ccpa-replayer                 ccpa-differ
   │ HTTP                          │                             │
   ▼                              ▼                             ▼
ccpa-recorder ──► fixture ──► apr code (RecordedDriver) ──► drift report
   │                                                             │
   ▼                                                             ▼
api.anthropic.com                                          pv validate
(live, recording phase only)                               parity_score
```

## Source-of-truth split

| Concern | Lives in |
|---|---|
| Contract TEXT | [`paiml/aprender/contracts/claude-code-parity-apr-v1.yaml`](https://github.com/paiml/aprender/blob/main/contracts/claude-code-parity-apr-v1.yaml) (canonical), pinned here via `contracts/pin.lock` |
| Spec | [docs/specifications/claude-code-parity-apr-poc.md](docs/specifications/claude-code-parity-apr-poc.md) (canonical here from M1; aprender redirect coming) |
| Implementation, fixtures, CI, coverage, pmat-comply | this repo (canonical) |

This split follows aprender's monorepo single-source-of-truth policy: aprender stays canonical for contract TEXT (where every paiml contract lives), while this repo is canonical for runtime ENFORCEMENT.

## Falsification gates

Twelve gates total, all `pv validate`-mechanically asserted per `CLAUDE.md § "DOGFOOD pv, NEVER bash"`.

**Source-of-truth invariants (M0+, online today on every PR):**

| ID | Name | Mechanism |
|----|------|-----------|
| FALSIFY-CCPA-009 | `ci_main_branch_green` | branch protection requires `ci/gate` |
| FALSIFY-CCPA-010 | `pmat_comply_100pct` | `pmat comply check --strict` exit 0 |
| FALSIFY-CCPA-011 | `line_coverage_100pct` | `cargo llvm-cov --fail-under-lines 100` |
| FALSIFY-CCPA-012 | `pv_contract_gate_on_commit` | pre-commit hook + CI run `pv validate` + `pin-check` |

**Behavioral parity gates (M1..M6):**

| ID | Name | Phase | Status |
|----|------|-------|--------|
| FALSIFY-CCPA-001 | `trace_schema_roundtrip` | M1 | ✅ ACTIVE — see [`crates/ccpa-trace/tests/falsify_ccpa_001_roundtrip.rs`](crates/ccpa-trace/tests/falsify_ccpa_001_roundtrip.rs) |
| FALSIFY-CCPA-002 | `replay_determinism` | M3 | planned |
| FALSIFY-CCPA-003 | `mock_completeness` | M3 | planned |
| FALSIFY-CCPA-004 | `tool_call_equivalence` | M4 | planned |
| FALSIFY-CCPA-005 | `file_mutation_equivalence` | M4 | planned |
| FALSIFY-CCPA-006 | `sovereignty_on_replay` | M5 | planned |
| FALSIFY-CCPA-007 | `corpus_coverage` | M5 | planned |
| FALSIFY-CCPA-008 | `parity_score_bound` | M6 | planned |

## Quickstart

```bash
# 1. Install local tools (matches CI)
make install-tools

# 2. Install the FALSIFY-CCPA-012 pre-commit hook
make install-hooks

# 3. Run every gate locally (mirror of CI)
make tier3
```

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
