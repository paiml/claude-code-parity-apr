# Regression fixture corpus — sensitivity test for the meter

`fixtures/canonical/` proves "when teacher and student agree, score = 1.0".
**`fixtures/regression/` proves the inverse**: every pair here has
deliberate drift; running `ccpa corpus` over this directory MUST exit
non-zero and the per-fixture rollup MUST identify the drift.

If `ccpa corpus fixtures/regression/` accidentally returns score = 1.0,
**the meter is broken** — that's a contract-level failure, not a
fixture mistake.

## Contents

| id | drift kind | DriftCategory expected |
|----|------------|------------------------|
| `0001-bash-different-cmd` | student runs `cat` instead of `ls` | `MismatchedToolInput` |
| `0002-missing-edit` | teacher emits 2 tool calls, student emits 1 | `MissingToolCall` |
| `0003-extra-tool` | student adds an extra Read | `ExtraToolCall` |

CI runs both corpora and asserts opposite outcomes; see
`.github/workflows/ci.yml § "regression corpus must FAIL"`.
