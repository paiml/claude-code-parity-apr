# CLAUDE.md — claude-code-parity-apr

## Methodology: contract-first

This is a **contract-first** repo: every behavior gate (`FALSIFY-CCPA-001..012`) is encoded as a falsifiable assertion in `contracts/claude-code-parity-apr-v1.yaml` *before* code lands. Tests prove the gate; `pv validate` proves the contract; `pmat comply` proves the project's compliance posture (CB-1400 et al.). No code ships without a contract.

## Repo overview

POC repo for record-replay-distill parity testing between Claude Code (teacher) and `apr code` (student). See `docs/specifications/claude-code-parity-apr-poc.md` for the full spec; gates live in `contracts/claude-code-parity-apr-v1.yaml`.

## Source of truth

- **Contract TEXT** is canonical in `paiml/aprender/contracts/claude-code-parity-apr-v1.yaml`. This repo pins it via `contracts/pin.lock` (sha256 + commit).
- **Implementation, fixtures, CI, coverage, pmat-comply** are canonical *here*.

## Quick commands

```bash
make tier1          # fmt + clippy + check          (<5s)
make tier2          # tier1 + tests                 (<30s)
make tier3          # tier2 + cov + comply + pv     (1-3 min)
make install-hooks  # FALSIFY-CCPA-012 pre-commit hook
```

## Code search policy: prefer `pmat query` over grep

Mirrors aprender's policy. `pmat query` returns quality-annotated, semantically ranked results (TDG grades, complexity, fault patterns). Raw grep returns lines.

| Task | Command |
|------|---------|
| Find functions by intent | `pmat query "trace serialization" --limit 10` |
| Find tests by topic | `pmat query "schema roundtrip" --limit 10` |
| Find with fault patterns | `pmat query "unwrap" --faults --exclude-tests` |
| Regex search (like rg -e) | `pmat query --regex "fn\s+test_\w+"` |
| Files with matches (like rg -l) | `pmat query "Trace::" --files-with-matches` |

`grep` / `rg` is acceptable only for non-Rust files (TOML, YAML, Markdown) or quick one-off debugging — **NEVER use grep** for Rust code search.

## Contract validation: dogfood `pv`, never bash

`pv` (binary from `aprender-contracts-cli`) is the dogfooded contract validator. Re-implementing what `pv` already does in bash/python is muda and is rejected. If `pv validate` rejects a contract, fix the contract or extend `aprender-contracts/src/schema/`.

## Falsification gate IDs

| ID | Name | Phase |
|----|------|-------|
| FALSIFY-CCPA-009 | `ci_main_branch_green` | M0+ |
| FALSIFY-CCPA-010 | `pmat_comply_100pct` (= `is_compliant: true`) | M0+ |
| FALSIFY-CCPA-011 | `line_coverage_100pct` | M0+ |
| FALSIFY-CCPA-012 | `pv_contract_gate_on_commit` | M0+ |
| FALSIFY-CCPA-001 | `trace_schema_roundtrip` | M1 |
| FALSIFY-CCPA-002..008 | behavioral parity | M3..M6 |

## Forbidden tools

- `cargo tarpaulin` — slow, unreliable. Use `cargo llvm-cov` only.
- `bash` re-implementations of `pv` / `pmat` / `cargo-llvm-cov` checks.
