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
| `0004-mismatched-tool-name` | teacher uses `Read`, student uses `Bash`/`cat` (same intent, different tool) | `MismatchedToolName` |
| `0005-missing-hook-event` | teacher fires `PreToolUse` hook on `Bash`, student doesn't | `MissingHookEvent` |
| `0006-extra-hook-event` | student fires `PreToolUse` hook on `Read`, teacher doesn't | `ExtraHookEvent` |
| `0007-mismatched-hook-event` | both fire `PreToolUse` on `Bash`/`rm`; teacher decision `block`, student decision `warn` | `MismatchedHookEvent` |
| `0008-missing-skill-invocation` | teacher invokes `/code-review` skill, student doesn't | `MissingSkillInvocation` |
| `0009-extra-skill-invocation` | student auto-invokes `regex-explainer` skill, teacher doesn't | `ExtraSkillInvocation` |
| `0010-mismatched-skill-invocation` | both invoke `sql-review`; teacher `user_invoked`+`instructions_injected:true`, student `auto_matched`+`false` | `MismatchedSkillInvocation` |

CI runs both corpora and asserts opposite outcomes; see
`.github/workflows/ci.yml § "regression corpus must FAIL"`.
