# Contributing to claude-code-parity-apr

Thanks for landing here. This repo is a record-replay-distill harness
proving [`apr code`](https://github.com/paiml/aprender) is byte-stable
against [Claude Code](https://docs.anthropic.com/claude/docs/claude-code)
at the action-stream level. Most contributions fall into one of three
categories:

1. **Adding a new fixture** to grow the canonical corpus.
2. **Bumping the contract** when the gate semantics or schema evolves.
3. **Fixing a gate** when CI fails on something that used to pass.

Each has a different ritual; doing the wrong ritual will fail CI in a
specific way.

## Source-of-truth split (read first)

Two repositories cooperate on this project:

| Repo | Canonical for |
|---|---|
| [`paiml/aprender`](https://github.com/paiml/aprender) | contract TEXT (`contracts/claude-code-parity-apr-v1.yaml`) |
| `paiml/claude-code-parity-apr` (this repo) | runtime ENFORCEMENT (CI gates, fixtures, the `ccpa` CLI) |

The contract YAML lives byte-identically in both repos. `contracts/pin.lock`
records which aprender commit holds the matching bytes; CI's
`scripts/pin-check-roundtrip.sh` (M22) verifies the round-trip on every
PR. Skipping a paired aprender push will fail CI deterministically — see
the contract-bump ritual below.

## Running the gates locally

```bash
# Install local tools (matches CI exactly)
make install-tools

# Install the FALSIFY-CCPA-012 pre-commit hook
make install-hooks

# Run every gate locally (mirror of CI)
make tier3
```

`make tier3` runs:
- `cargo fmt --check` + `cargo clippy -D warnings` + `cargo build`
- `cargo test --workspace --all-features`
- `cargo llvm-cov` (≥99% lines, 100% functions)
- `pmat comply check` (`is_compliant=true`, 0 Fail-status)
- `pv validate` + `pv lint`
- `pin-check.sh` (sha256 match)

For the round-trip companion ↔ aprender check (requires `gh` + a token
with `contents:read` on `paiml/aprender`):

```bash
make pin-check-roundtrip
```

## Adding a new fixture

Fixtures live in `fixtures/canonical/<NNNN>-<slug>/`. Each has three files:

```
fixtures/canonical/0042-my-scenario/
├── meta.toml                      # id, covers (parity-matrix rows), description
├── teacher.ccpa-trace.jsonl       # what Claude Code does for this scenario
└── student.ccpa-trace.jsonl       # what apr code SHOULD do
```

Steps:

1. **Pick a parity-matrix row to cover.** The 17 required rows live in
   [`paiml/aprender/contracts/apr-code-parity-v1.yaml § categories`](https://github.com/paiml/aprender/blob/main/contracts/apr-code-parity-v1.yaml).
   2 rows (`keyboard-shortcuts`, `status-line`) are OOS at the trace
   boundary; any of the other 15 are fair game.

2. **Author `meta.toml`:**

   ```toml
   [fixture]
   id = "0042-my-scenario"
   covers = ["builtin-tools-rwegs"]
   description = "What this fixture exercises and why."
   ```

3. **Author `teacher.ccpa-trace.jsonl`** — JSONL, one record per line.
   Record kinds (`session_start`, `user_prompt`, `assistant_turn`,
   `tool_result`, `session_end`, `hook_event`, `skill_invocation`) are
   typed in [`crates/ccpa-trace/src/lib.rs`](crates/ccpa-trace/src/lib.rs);
   the canonical schema lives in [`contracts/claude-code-parity-apr-v1.yaml § trace_schema`](contracts/claude-code-parity-apr-v1.yaml).

4. **Author `student.ccpa-trace.jsonl`** — same record sequence, but
   describing what `apr code` *should* do. Per-tool semantic-equivalence
   rules (Bash whitespace-collapse, Edit post-state-sha, …) mean
   teacher and student can be syntactically different while semantically
   identical; see the existing fixtures for examples.

5. **Verify locally:**

   ```bash
   ccpa corpus fixtures/canonical/                            # MUST exit 0
   ccpa coverage \
     --apr-code-parity-yaml ../aprender/contracts/apr-code-parity-v1.yaml \
     --fixtures-dir fixtures/canonical/ \
     --oos-rows keyboard-shortcuts,status-line                # MUST exit 0
   make tier3                                                 # full local sweep
   ```

6. **Open a PR.** No contract bump needed — fixture additions are
   automatic-coverage additions, not gate-semantics changes. The
   measured-parity record in `contracts/...v1.yaml § status_history`
   may want a refresh entry; precedent for this is M11–M19.

### Common fixture pitfalls

- **`session_id` and `cwd_sha256` are normalized at compare time** —
  they don't have to be real, just stable across teacher↔student.
- **Tool-call IDs (`toolu_...`)** are normalized to `<TOOL-N>` at
  compare time. Use any deterministic-looking string.
- **Per-record `v: 1`** field is required (this is the schema-v1
  per-record back-compat layer; the file-level schema is v2 since M15).
- **`stop_reason` on `assistant_turn` follows from `blocks`** — if any
  `Block::ToolUse` is present, `stop_reason: tool_use`; else `end_turn`.
- **`HookEvent` records sit BETWEEN the `assistant_turn` that emitted
  the matching `tool_use` and the corresponding `tool_result`.** See
  `0018-hook-pre-tool-use` and `0027-hook-block` for the canonical
  ordering.

## Bumping the contract (the M22 5-step ritual)

The contract is byte-identical across `paiml/aprender` and this repo.
Bumping it requires five steps, in order:

```
1. Edit contract here:        contracts/claude-code-parity-apr-v1.yaml
                              Refresh contract_sha256 in contracts/pin.lock.
2. Mirror those bytes:        cp contracts/...v1.yaml /path/to/aprender-worktree/contracts/...v1.yaml
                              git -C /path/to/aprender-worktree commit && git push
                              # capture the new commit hash, e.g. abcd1234e
3. Pin the new hash:          edit contracts/pin.lock — set
                              `aprender_commit: abcd1234e`
                              `last_synced_utc: <now>`
4. Refresh human-readable
   roll-up views:             — README.md badges (Contract version, status block dates)
                              — docs/specifications/...md "Sub-milestones (M11+)" table
                              — docs/specifications/...md "Falsification run history" table
                              — docs/specifications/...md status snapshot blockquote
                              — CONTRIBUTING.md status footer (this file)
                              These are NOT mechanically guarded by pin-check;
                              a kaizen sweep is the backstop. Step 4 makes the
                              sweep unnecessary.
5. Push your companion-side:  CI runs scripts/pin-check.sh AND
                              scripts/pin-check-roundtrip.sh; both must pass.
```

Skipping step 2 or 3 → `pin-check-roundtrip` fails with:

```
pin-check-roundtrip FAIL - companion vs aprender bytes diverge
  here     contracts/...v1.yaml                     = <local-sha>
  upstream paiml/aprender@<commit>:contracts/...v1.yaml = <remote-sha>
```

This is the M21 drift class; the M22 guard makes it impossible to land.

Skipping step 4 → no mechanical failure, but the README and spec
will silently drift out of sync with the contract. Confirmed
historical drift class (see commits b96b089, 1eff3f8, aada9ea,
ce6bcbc, 93fbd53 — five docs-only sweeps caught this on M34 alone).
Step 4 was added 2026-05-01 to make the drift class visible at
authoring time instead of catching it on the next kaizen sweep.

The companion-side `status_history` entry should record the bump
reason (M-numbered), the version transition (e.g. `v1.10.0 → v1.11.0`),
and what the surface change is. See any M11+ entry for the pattern.

### When NOT to bump the contract

- **Adding a fixture** (no semantics change; corpus depth grows but
  the gate surface is identical).
- **Refactoring a Rust crate** (no schema or behavioral change).
- **Editing a markdown comment in the contract** that isn't load-bearing.
  (Though running `make tier3` after will catch the sha256 mismatch and
  remind you to think about whether a bump is warranted.)

### When you MUST bump the contract

- Changing trace schema (record kinds, fields).
- Changing equivalence rule semantics (e.g. relaxing Bash normalization).
- Flipping a gate's enforcement level (informational → hard-blocking;
  see M16).
- Adding/removing a row from the OOS list (`keyboard-shortcuts`,
  `status-line`).
- Anything that would make a previously-green PR newly fail or
  vice-versa.

## Fixing a failing gate

When CI fails, the failing step's name maps to a gate ID:

| CI step | Gate | Where to look |
|---|---|---|
| `cargo fmt --check`       | (project hygiene)              | `cargo fmt --all` then re-push |
| `cargo clippy`            | (project hygiene)              | clippy output usually has a `try` block |
| `cargo test --workspace`  | FALSIFY-CCPA-001…008          | check the per-test crate; update the contract if the failure reflects a real semantics change |
| `cargo llvm-cov`          | FALSIFY-CCPA-011              | add tests for any uncovered function/line; gate is 100% functions ∧ ≥99% lines |
| `pv validate`             | FALSIFY-CCPA-012 (a)          | YAML schema or pv-rule violation; restructure the contract to fit the existing schema |
| `pin-check`               | FALSIFY-CCPA-012 (b)          | local contract bytes don't match `pin.lock`; usually means you edited the contract without re-hashing |
| `pin-check-roundtrip`     | FALSIFY-CCPA-012 (c)          | aprender-side bytes don't match local; you skipped step 2 or 3 of the 5-step ritual |
| `ccpa corpus canonical/`  | FALSIFY-CCPA-013 runtime      | a fixture pair scores < 1.0; inspect the drift category |
| `ccpa corpus regression/` | (meter sensitivity)           | regression corpus PASSED — meter is broken; debug the differ |
| `ccpa coverage`           | FALSIFY-CCPA-007 hard gate    | a reachable parity-matrix row is uncovered; author a fixture for it |
| `pmat comply check`       | FALSIFY-CCPA-010              | `is_compliant=false` or any Fail-status check; address the root cause |

## What NOT to do

- **Skip the round-trip on a contract bump.** CI will catch it; locally you
  catch it via `make pin-check-roundtrip`. Either way, fix it before merge.
- **Hand-edit `pin.lock` to silence `pin-check-roundtrip`.** The script
  refuses to lie — pointing `aprender_commit` at a commit whose bytes don't
  match locally is exactly the drift the guard prevents.
- **Use `cargo tarpaulin`.** Forbidden by global CLAUDE.md — slow and
  unreliable. Use `cargo llvm-cov`.
- **Mock the differ in fixture tests.** Fixtures are AUTHORED canonical
  references; they should drive the real `ccpa-differ` code path end-to-end.
- **Re-implement `pv` in bash.** Forbidden by global CLAUDE.md "DOGFOOD pv,
  NEVER bash". Extend `aprender-contracts` instead.
- **Skip `make tier3` before push.** It's the local mirror of CI; running
  it locally takes 30s and saves a PR cycle.

## Spec evolution

The companion-side spec at
[`docs/specifications/claude-code-parity-apr-poc.md`](docs/specifications/claude-code-parity-apr-poc.md)
is the project blueprint. Major behavioral changes update both:

1. The contract's `status_history` (factual, machine-readable record).
2. The spec markdown (narrative + milestone roll-up).

Status as of v1.23.0 (2026-05-02): M0–M37 all SHIPPED; corpus complete
(30/30); 13/13 gates green; companion ↔ aprender round-trip
mechanically guarded. **M32d numerical-parity FUNCTIONALLY DISCHARGED**
2026-05-02 (aprender PR #1228 squash 5235aaeb9): output transition
`%%%%%%%%` gibberish → `2 + 2 = 4` + multi-domain coherent answers.
Cosine ≥ 0.99 vs HF FP16 (formal flip of `qwen3-moe-forward-v1`
DRAFT → ACTIVE_RUNTIME) remains operator-confirm pending ~60GB
download.
