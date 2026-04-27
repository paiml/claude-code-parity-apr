#!/usr/bin/env bash
# pin-check-roundtrip.sh — companion ↔ aprender source-of-truth round-trip.
#
# pin-check.sh asserts the LOCAL companion-side contract bytes match the
# sha256 in pin.lock. This script asserts the UPSTREAM aprender-side
# bytes (at the recorded `aprender_commit`) ALSO match — i.e., that the
# two repos are byte-identical.
#
# Closes the M21 drift class: pre-M22 it was possible for companion-side
# contract bumps to land without a paired aprender-side mirror push,
# and pin-check.sh would still pass because it only inspected
# companion-side bytes.
#
# Mechanism:
#   1. Read aprender_repo / aprender_commit / aprender_path from pin.lock.
#   2. Fetch the file at that exact commit via `gh api`.
#   3. base64-decode and sha256 the bytes.
#   4. Compare to local companion-side sha256.
#   5. Exit 1 on mismatch (round-trip broken).
#
# Run via:
#   - CI step (see .github/workflows/ci.yml — M22 step)
#   - locally: `make pin-check-roundtrip`
#
# Requires:
#   - `gh` CLI on PATH (in CI: pre-installed on ubuntu-latest)
#   - GH_TOKEN / GITHUB_TOKEN env var with `contents:read` on aprender_repo
#
# Refs: FALSIFY-CCPA-012 (pv_contract_gate_on_commit § round-trip freshness)
#       contract status_history M22 (this script's discharge target)

set -euo pipefail

PIN_FILE="${1:-contracts/pin.lock}"

if [[ ! -f "${PIN_FILE}" ]]; then
    echo "pin-check-roundtrip: ${PIN_FILE} not found" >&2
    exit 1
fi

# Tiny field extractor — same key:value style as pin-check.sh.
field() {
    awk -F': *' -v key="$1" '$1 == key { print $2; exit }' "${PIN_FILE}"
}

aprender_repo=$(field 'aprender_repo')
aprender_commit=$(field 'aprender_commit')
aprender_path=$(field 'aprender_path')
contract_path=$(field 'contract_path')

for f in aprender_repo aprender_commit aprender_path contract_path; do
    val="${!f}"
    if [[ -z "${val}" ]]; then
        echo "pin-check-roundtrip: ${PIN_FILE} missing required field '${f}'" >&2
        exit 1
    fi
done

if [[ ! -f "${contract_path}" ]]; then
    echo "pin-check-roundtrip: local contract ${contract_path} not found" >&2
    exit 1
fi

local_sha=$(sha256sum "${contract_path}" | awk '{print $1}')

# Fetch upstream bytes via gh api (returns base64-encoded `content` field).
gh_endpoint="repos/${aprender_repo}/contents/${aprender_path}?ref=${aprender_commit}"
remote_b64=$(gh api "${gh_endpoint}" --jq '.content' 2>/dev/null || true)

if [[ -z "${remote_b64}" ]]; then
    cat >&2 <<EOF
pin-check-roundtrip FAIL - could not fetch upstream bytes
  gh api: ${gh_endpoint}
  Either:
    (a) ${aprender_repo} is unreachable or unauthorized for this token
    (b) ${aprender_path} does not exist at ${aprender_commit}
    (c) gh CLI not installed / GH_TOKEN missing
EOF
    exit 1
fi

remote_sha=$(echo "${remote_b64}" | base64 -d | sha256sum | awk '{print $1}')

if [[ "${local_sha}" != "${remote_sha}" ]]; then
    cat >&2 <<EOF
pin-check-roundtrip FAIL - companion vs aprender bytes diverge
  here     ${contract_path}                                  = ${local_sha}
  upstream ${aprender_repo}@${aprender_commit}:${aprender_path} = ${remote_sha}

Companion-side bytes have drifted from the aprender-side canonical
copy at the pinned commit (this is the M21 drift class).

Remediation:
  step 1. Push the companion bytes to ${aprender_repo}@<branch>; merge to main.
  step 2. Update ${PIN_FILE} aprender_commit to the new aprender HEAD.
  step 3. Re-run this check.
EOF
    exit 1
fi

echo "pin-check-roundtrip OK - companion <-> aprender byte-identical (${local_sha:0:12}...)"
