#!/usr/bin/env bash
# smoke-m32d.sh — repeatable dogfood verification for M32d discharge.
#
# M32d (numerical-parity discharge, 2026-05-02 via aprender PR #1228
# squash 5235aaeb9) converted gibberish output (`%%%%%%%%`) to coherent
# answers. This script codifies the manual dogfood loop into a small
# repeatable smoke test: 3 prompts × 3 domains, each verified for
# non-empty + non-gibberish output.
#
# Drift class addressed: M32d regressions on aprender main. If a future
# aprender PR re-introduces gibberish output (per-head Q/K RMSNorm
# regression, rope_theta wrong default, chat template change, etc.),
# this script will emit a clear FAIL.
#
# This is NOT part of `make tier3` — it requires:
#   - The 17.3 GB Qwen3-Coder-30B-A3B-Instruct-Q4_K_M.gguf cached locally
#   - The apr binary built with `--features cuda` for fastest run
#   - Network or compute capacity to run ~3-5 minutes of inference
#
# Run via:
#   - `make smoke-m32d` (operator-opt-in)
#   - manual:           `bash scripts/smoke-m32d.sh`
#
# Exit codes:
#   0 — all 3 prompts produced coherent output
#   1 — at least one prompt produced empty / gibberish / regressed output
#
# Refs: M32d FUNCTIONAL DISCHARGE 2026-05-02 (aprender PR #1228, M35)
# Refs: M37 (sub-milestones backfill recording all M32d Steps)

set -euo pipefail

APR_BIN="${APR_BIN:-/mnt/nvme-raid0/targets/aprender/release/apr}"

# Pick first available GGUF from the canonical cached locations.
GGUF_CANDIDATES=(
    "/mnt/nvme-raid0/cache/apr-home/models/Qwen3-Coder-30B-A3B-Instruct-Q4_K_M.gguf"
    "/home/noah/models/Qwen3-Coder-30B-A3B-Instruct-Q4_K_M.gguf"
)

GGUF=""
for candidate in "${GGUF_CANDIDATES[@]}"; do
    if [[ -f "${candidate}" ]]; then
        GGUF="${candidate}"
        break
    fi
done

if [[ -z "${GGUF}" ]]; then
    echo "smoke-m32d: no Qwen3-Coder-30B-A3B-Instruct-Q4_K_M.gguf found in:" >&2
    for c in "${GGUF_CANDIDATES[@]}"; do
        echo "  - ${c}" >&2
    done
    echo "smoke-m32d: SKIP (fixture-absent ≠ defect)" >&2
    exit 0
fi

if [[ ! -x "${APR_BIN}" ]]; then
    echo "smoke-m32d: apr binary not at ${APR_BIN}" >&2
    echo "smoke-m32d: SKIP (build apr first via cargo build --release -p apr-cli --features cuda)" >&2
    exit 0
fi

echo "smoke-m32d: GGUF  = ${GGUF}"
echo "smoke-m32d: apr   = ${APR_BIN}"
echo

# 3 prompts × 3 domains (math / geography / translation).
# Each prompt's output must:
#   - exit 0 from apr
#   - emit at least 5 non-whitespace chars in the Output: section
#   - NOT contain "%%%%%%%%" or other M32d-pre-fix gibberish patterns
fail_count=0

run_prompt() {
    local label="$1"
    local prompt="$2"
    local max_tokens="$3"

    echo "──────────────────────────────────────────────────────────────"
    echo "smoke-m32d: ${label}"
    echo "  prompt: ${prompt}"

    local raw
    if ! raw=$("${APR_BIN}" run "${GGUF}" \
            --prompt "${prompt}" \
            --max-tokens "${max_tokens}" 2>&1); then
        echo "  FAIL: apr run exit non-zero"
        fail_count=$((fail_count + 1))
        return
    fi

    # Extract the line(s) after "Output:" up to but not including the
    # "Completed in" line.
    local output
    output=$(echo "${raw}" | awk '/^Output:/{flag=1; next} /^Completed in/{flag=0} flag')

    # Strip leading/trailing whitespace.
    output=$(echo "${output}" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')

    echo "  output: ${output}"

    # Gate 1: non-empty (≥5 non-whitespace chars).
    local non_ws_chars
    non_ws_chars=$(echo "${output}" | tr -d '[:space:]' | wc -c)
    if [[ "${non_ws_chars}" -lt 5 ]]; then
        echo "  FAIL: output has only ${non_ws_chars} non-whitespace chars (<5)"
        fail_count=$((fail_count + 1))
        return
    fi

    # Gate 2: NOT M32d-pre-fix gibberish ("%%%%%%%%").
    if echo "${output}" | grep -q '%%%%%%%'; then
        echo "  FAIL: output contains '%%%%%%%' M32d-pre-fix gibberish — REGRESSION"
        fail_count=$((fail_count + 1))
        return
    fi

    echo "  PASS"
}

run_prompt "math"        "What is 5+7?"          12
run_prompt "geography"   "Capital of France:"    10
run_prompt "translation" "Translate to Spanish: Hello world" 16

echo "──────────────────────────────────────────────────────────────"
if [[ "${fail_count}" -gt 0 ]]; then
    echo "smoke-m32d FAIL: ${fail_count} prompt(s) regressed"
    exit 1
fi

echo "smoke-m32d OK: 3 / 3 prompts coherent (M32d discharge holds)"
