#!/usr/bin/env bash
# install-hooks.sh — install the pre-commit hook that enforces
# FALSIFY-CCPA-012 (pv_contract_gate_on_commit) locally.
#
# CI re-runs the same command, so a missing local hook only delays the
# failure to PR-time, not bypasses it.

set -euo pipefail

REPO_ROOT=$(git rev-parse --show-toplevel)
HOOKS_DIR="${REPO_ROOT}/.git/hooks"
PRE_COMMIT="${HOOKS_DIR}/pre-commit"

mkdir -p "${HOOKS_DIR}"

cat >"${PRE_COMMIT}" <<'HOOK'
#!/usr/bin/env bash
# pre-commit hook — claude-code-parity-apr
# Enforces FALSIFY-CCPA-012 locally. Mirrors the CI step.

set -euo pipefail

if ! command -v pv >/dev/null 2>&1; then
    echo "pre-commit: pv not found on PATH (cargo install aprender-contracts-cli)" >&2
    exit 1
fi

pv validate contracts/claude-code-parity-apr-v1.yaml
bash scripts/pin-check.sh contracts/pin.lock
bash scripts/check-doc-drift.sh
HOOK

chmod +x "${PRE_COMMIT}"
echo "Installed pre-commit hook at ${PRE_COMMIT}"
