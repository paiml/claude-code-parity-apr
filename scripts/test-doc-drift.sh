#!/usr/bin/env bash
# test-doc-drift.sh — meta-test for scripts/check-doc-drift.sh.
#
# The drift detector itself can regress: a refactor breaks an assert, a
# regex stops matching, a typo silently disables a check. Each drift
# class is then no longer mechanically guarded. This script verifies
# every assert in check-doc-drift.sh by:
#
#   1. Running the detector on the live repo state — must exit 0 (clean)
#   2. For each drift class: corrupt the relevant file, re-run detector,
#      assert it exits 1 with the expected DRIFT message, then restore
#   3. Re-run the detector — must exit 0 again
#
# Drift class addressed: silent regression of check-doc-drift.sh. M38
# established the detector; this script ensures the detector itself
# stays load-bearing.
#
# Run via:
#   - locally: `make test-doc-drift`
#   - CI:      called from .github/workflows/ci.yml
#
# Exit codes:
#   0 — every drift class is correctly caught by the detector
#   1 — at least one expected drift was NOT caught (detector regressed)
#
# Refs: M38 (drift detector base, 7f66c57)
# Refs: M39, M40, M41, M42, M43, M44 (cumulative asserts)

set -euo pipefail

SPEC="docs/specifications/claude-code-parity-apr-poc.md"
README="README.md"
CONTRIBUTING="CONTRIBUTING.md"
PARITY_JSON="fixtures/canonical/measured-parity.json"

# Derive current M-count from spec sub-milestones table tail. This makes
# the test self-adjusting — bumping M-count in the live spec doesn't
# require editing this file (would otherwise add the test-script edit to
# every M22-step-4 ritual).
CURRENT_M=$(grep -oE '^\| \*\*M[0-9]+\*\*' "${SPEC}" \
    | grep -oE '[0-9]+' \
    | sort -n \
    | tail -1)

if [[ -z "${CURRENT_M}" ]]; then
    echo "test-doc-drift: could not derive current M-count from ${SPEC}" >&2
    exit 1
fi

echo "test-doc-drift: derived current M-count = M${CURRENT_M}"

# Pre-flight: detector must be clean on live repo.
echo "test-doc-drift: pre-flight — detector must be clean on live repo"
if ! bash scripts/check-doc-drift.sh > /dev/null 2>&1; then
    echo "FAIL: detector reports drift on live repo (cannot run meta-test)" >&2
    bash scripts/check-doc-drift.sh >&2 || true
    exit 1
fi
echo "  OK"
echo

# Test harness:
# corrupt FILE PATTERN REPLACEMENT EXPECTED_DRIFT_FRAGMENT
# Substitutes PATTERN→REPLACEMENT in FILE, runs detector, restores FILE.
# Fails if detector exits 0 OR if EXPECTED_DRIFT_FRAGMENT not in stderr.
fail_count=0
test_count=0

corrupt() {
    local file="$1"
    local pattern="$2"
    local replacement="$3"
    local expect="$4"
    local label="$5"

    test_count=$((test_count + 1))
    echo "[#${test_count}] ${label}"

    cp "${file}" "${file}.test.bak"
    sed -i "s|${pattern}|${replacement}|" "${file}"

    local stderr_out
    if stderr_out=$(bash scripts/check-doc-drift.sh 2>&1 1>/dev/null); then
        echo "  FAIL: detector exited 0 — drift NOT caught"
        fail_count=$((fail_count + 1))
    elif ! echo "${stderr_out}" | grep -qF "${expect}"; then
        echo "  FAIL: detector exited 1 but message did NOT contain '${expect}'"
        echo "  Actual stderr:"
        echo "${stderr_out}" | sed 's/^/    /'
        fail_count=$((fail_count + 1))
    else
        echo "  PASS"
    fi

    mv "${file}.test.bak" "${file}"
}

# 1. M-count drift in spec status snapshot
corrupt "${SPEC}" \
    "Status snapshot (2026-05-02)\\*\\*: M0–M${CURRENT_M} SHIPPED" \
    "Status snapshot (2026-05-02)**: M0–M99 SHIPPED" \
    "M0–M99 SHIPPED but sub-milestones table tail" \
    "spec status snapshot M-count"

# 2. M-count drift in README
corrupt "${README}" \
    "M0–M${CURRENT_M} all SHIPPED" \
    "M0–M99 all SHIPPED" \
    "README.md says M0–M99 all SHIPPED" \
    "README M-count"

# 3. M-count drift in CONTRIBUTING
corrupt "${CONTRIBUTING}" \
    "M0–M${CURRENT_M} all SHIPPED" \
    "M0–M99 all SHIPPED" \
    "CONTRIBUTING.md says M0–M99 all SHIPPED" \
    "CONTRIBUTING M-count"

# 4. README contract version drift
corrupt "${README}" \
    "Contract at v1.23.0" \
    "Contract at v9.99.0" \
    "Contract at v9.99.0" \
    "README contract version"

# 5. CONTRIBUTING contract version drift
corrupt "${CONTRIBUTING}" \
    "Status as of v1.23.0" \
    "Status as of v9.99.0" \
    "Status as of v9.99.0" \
    "CONTRIBUTING contract version"

# 6. Corpus fixture_count drift
corrupt "${PARITY_JSON}" \
    '"fixture_count": 30' \
    '"fixture_count": 99' \
    "fixture_count=99 but actual corpus has" \
    "measured-parity.json fixture_count"

# 7. Run history end-M drift
corrupt "${SPEC}" \
    "M1–M${CURRENT_M} (every merge" \
    "M1–M99 (every merge" \
    "M1–M99 but sub-milestones table tail" \
    "Run history end-M"

# 8. Status badge drift
corrupt "${README}" \
    "status-ACTIVE__RUNTIME-green" \
    "status-DEPRECATED-red" \
    "status badge says 'DEPRECATED'" \
    "README status badge"

# 9. Gates badge denominator drift
corrupt "${README}" \
    "gates-13%2F13" \
    "gates-13%2F99" \
    "gates badge denominator is 99" \
    "README gates badge denominator"

# 10. Parity badge value drift
corrupt "${README}" \
    "measured%20parity-1.0000" \
    "measured%20parity-0.5000" \
    "parity badge says 0.5000" \
    "README parity badge value"

# Post-flight: detector must be clean again on live repo (verifies all
# corruptions were correctly restored).
echo
echo "test-doc-drift: post-flight — detector must be clean on live repo"
if ! bash scripts/check-doc-drift.sh > /dev/null 2>&1; then
    echo "FAIL: detector reports drift on live repo after restore (test left files dirty)" >&2
    bash scripts/check-doc-drift.sh >&2 || true
    exit 1
fi
echo "  OK"

echo
echo "──────────────────────────────────────────────────────────────"
if [[ "${fail_count}" -gt 0 ]]; then
    echo "test-doc-drift FAIL — ${fail_count} of ${test_count} drift classes regressed"
    exit 1
fi

echo "test-doc-drift OK — ${test_count} / ${test_count} drift classes caught by detector"
