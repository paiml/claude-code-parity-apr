#!/usr/bin/env bash
# pin-check.sh — assert contracts/pin.lock.contract_sha256 matches the
# local contract file. NOT a re-implementation of `pv validate` (which is
# strictly forbidden per CLAUDE.md § "DOGFOOD pv, NEVER bash"); this only
# checks file-integrity of the pinned bytes.
#
# Run via:
#   - pre-commit hook (see scripts/install-hooks.sh)
#   - CI step (see .github/workflows/ci.yml)
#
# Exit codes:
#   0 — pin matches
#   1 — pin mismatch (contract file edited without updating pin.lock,
#       OR pin.lock edited without re-hashing)
#
# Refs: FALSIFY-CCPA-012 (pv_contract_gate_on_commit § pin freshness)

set -euo pipefail

PIN_FILE="${1:-contracts/pin.lock}"

if [[ ! -f "${PIN_FILE}" ]]; then
    echo "pin-check: ${PIN_FILE} not found" >&2
    exit 1
fi

contract_path=$(awk -F': *' '/^contract_path:/ {print $2; exit}' "${PIN_FILE}")
expected_sha=$(awk -F': *' '/^contract_sha256:/ {print $2; exit}' "${PIN_FILE}")

if [[ -z "${contract_path}" || -z "${expected_sha}" ]]; then
    echo "pin-check: ${PIN_FILE} missing contract_path or contract_sha256" >&2
    exit 1
fi

if [[ ! -f "${contract_path}" ]]; then
    echo "pin-check: pinned contract ${contract_path} not found" >&2
    exit 1
fi

actual_sha=$(sha256sum "${contract_path}" | awk '{print $1}')

if [[ "${expected_sha}" != "${actual_sha}" ]]; then
    cat >&2 <<EOF
pin-check FAIL — pinned contract sha256 drift
  pin file:        ${PIN_FILE}
  contract:        ${contract_path}
  expected sha256: ${expected_sha}
  actual sha256:   ${actual_sha}

Either:
  (a) Update ${PIN_FILE} to the new sha256 (and aprender_commit if the
      contract changed upstream), OR
  (b) Revert the local contract changes if they were unintended.
EOF
    exit 1
fi

echo "pin-check OK — ${contract_path} sha256 matches ${PIN_FILE}"
