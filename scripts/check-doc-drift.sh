#!/usr/bin/env bash
# check-doc-drift.sh — mechanical guard against the M22-step-4 drift class.
#
# CONTRIBUTING.md § "Bumping the contract (the M22 5-step ritual)" notes:
#
#     "These are NOT mechanically guarded by pin-check; a kaizen sweep is
#      the backstop. Step 4 makes the sweep unnecessary."
#
# That backstop kept catching drift on every kaizen pass (M34 sweep alone
# fixed 5 docs-only commits; M37 added another 6+). This script codifies
# the cross-reference asserts so authoring time catches what kaizen used
# to catch at sweep time.
#
# Asserts (each fails with a precise diff):
#
#   1. Spec header   "M0–MX SHIPPED"   tail M of sub-milestones table
#   2. Spec status   "M0–MX SHIPPED" == header M
#   3. README        "M0–MX all SHIPPED" == header M
#   4. CONTRIBUTING  "M0–MX all SHIPPED" == header M
#   5. Spec section header "Falsification conditions (N gates total)"
#      == count(FALSIFY-CCPA-NNN row markers in the gate tables)
#   6. README badge / README status / CONTRIBUTING status / spec status
#      snapshot all quote the same vX.Y.Z as the contract YAML's
#      metadata.version
#   7. measured-parity.json fixture_count == count(NNNN-* dirs in
#      fixtures/canonical/) AND README badge `corpus-N/30` matches
#   8. Spec Falsification run history latest "Run N" revision range
#      "M1–MX" end-M matches sub-milestones table tail M
#   9. README status badge `status-X-green.svg` matches contract YAML
#      top-level `status:` field
#
# This is NOT a re-implementation of `pv validate` (forbidden per
# CLAUDE.md § "DOGFOOD pv, NEVER bash"); it operates on docs/markdown
# only, never on contract YAML.
#
# Run via:
#   - `make tier3` (locally, before push)
#   - CI step (companion-side ci/gate)
#
# Exit codes:
#   0 — all 5 asserts pass
#   1 — at least one drift detected; offending lines + expected value printed
#
# Refs: M22 step 4 (CONTRIBUTING.md, 477de18 — drift class definition)
# Refs: M37 (six drift-fix commits this script's asserts would have caught)

set -euo pipefail

SPEC="${1:-docs/specifications/claude-code-parity-apr-poc.md}"
README="${2:-README.md}"
CONTRIBUTING="${3:-CONTRIBUTING.md}"

if [[ ! -f "${SPEC}" ]]; then
    echo "check-doc-drift: spec ${SPEC} not found" >&2
    exit 1
fi
if [[ ! -f "${README}" ]]; then
    echo "check-doc-drift: README ${README} not found" >&2
    exit 1
fi
if [[ ! -f "${CONTRIBUTING}" ]]; then
    echo "check-doc-drift: CONTRIBUTING ${CONTRIBUTING} not found" >&2
    exit 1
fi

drift_count=0
report() {
    drift_count=$((drift_count + 1))
    echo "" >&2
    echo "DRIFT #${drift_count}: $1" >&2
}

# 1. Tail M-number of the sub-milestones table — the GROUND TRUTH for
#    "highest milestone shipped". Pattern: lines starting with `| **MNN**`
#    or `| **M32d-...**` etc., we strip back to MNN integer.
#    The sub-milestones table is the section starting "## Phases / Milestones"
#    or similar; we just scan the whole spec for `| **M\d+**` rows and take max.
tail_m=$(grep -oE '^\| \*\*M[0-9]+\*\*' "${SPEC}" \
    | grep -oE '[0-9]+' \
    | sort -n \
    | tail -1)

if [[ -z "${tail_m}" ]]; then
    echo "check-doc-drift: could not find any '| **MNN**' rows in ${SPEC}" >&2
    exit 1
fi

# 2. Spec header  "M0–MX SHIPPED" or "M0-MX SHIPPED" (en-dash or hyphen).
header_m=$(grep -oE 'M0[–-]M[0-9]+ SHIPPED' "${SPEC}" \
    | head -1 \
    | sed -E 's/.*M([0-9]+) SHIPPED.*/\1/')

if [[ -z "${header_m}" ]]; then
    report "spec header has no 'M0–MX SHIPPED' line"
elif [[ "${header_m}" != "${tail_m}" ]]; then
    report "spec header says M0–M${header_m} SHIPPED but sub-milestones table tail is M${tail_m}"
fi

# 3. Spec status snapshot — find lines starting with "> **Status snapshot"
#    and check for "M0–MX SHIPPED".
status_m=$(grep -oE 'M0[–-]M[0-9]+ SHIPPED' "${SPEC}" \
    | sed -n '2p' \
    | sed -E 's/.*M([0-9]+) SHIPPED.*/\1/' || true)

if [[ -n "${status_m}" && "${status_m}" != "${tail_m}" ]]; then
    report "spec status snapshot says M0–M${status_m} SHIPPED but sub-milestones table tail is M${tail_m}"
fi

# 4. README status block — "M0–MX all SHIPPED".
readme_m=$(grep -oE 'M0[–-]M[0-9]+ all SHIPPED' "${README}" \
    | head -1 \
    | sed -E 's/.*M([0-9]+) all SHIPPED.*/\1/' || true)

if [[ -z "${readme_m}" ]]; then
    report "${README} has no 'M0–MX all SHIPPED' line"
elif [[ "${readme_m}" != "${tail_m}" ]]; then
    report "${README} says M0–M${readme_m} all SHIPPED but sub-milestones table tail is M${tail_m}"
fi

# 5. CONTRIBUTING status footer — "M0–MX all SHIPPED".
contributing_m=$(grep -oE 'M0[–-]M[0-9]+ all SHIPPED' "${CONTRIBUTING}" \
    | head -1 \
    | sed -E 's/.*M([0-9]+) all SHIPPED.*/\1/' || true)

if [[ -z "${contributing_m}" ]]; then
    report "${CONTRIBUTING} has no 'M0–MX all SHIPPED' line"
elif [[ "${contributing_m}" != "${tail_m}" ]]; then
    report "${CONTRIBUTING} says M0–M${contributing_m} all SHIPPED but sub-milestones table tail is M${tail_m}"
fi

# 6. Stated gate count == count of FALSIFY-CCPA-NNN row markers. The
#    spec's "## Falsification conditions (N gates total)" header should
#    match the number of FALSIFY-CCPA-NNN gate rows in the source-of-
#    truth-invariants + behavioral-parity tables that follow.
stated_gates=$(grep -oE '## Falsification conditions \([0-9]+ gates total\)' "${SPEC}" \
    | head -1 \
    | grep -oE '[0-9]+')

# Count unique FALSIFY-CCPA-NNN ids in the falsification-conditions
# section (rows of the form "| FALSIFY-CCPA-NNN | name | ..."). We dedup
# because some IDs can appear in narrative text outside the table.
actual_gates=$(awk '/^## Falsification conditions/,/^## Academic basis/' "${SPEC}" \
    | grep -oE '\| FALSIFY-CCPA-[0-9]+' \
    | sort -u \
    | wc -l)

if [[ -z "${stated_gates}" ]]; then
    report "spec has no '## Falsification conditions (N gates total)' header"
elif [[ "${stated_gates}" != "${actual_gates}" ]]; then
    report "spec header says (${stated_gates} gates total) but found ${actual_gates} FALSIFY-CCPA-NNN row markers"
fi

# 7. Contract version cross-references — README badge / README status
#    block / CONTRIBUTING status footer / spec status snapshot must all
#    quote the same vN.M.K as contracts/claude-code-parity-apr-v1.yaml's
#    metadata.version. M22 ritual step 1 bumps the YAML; step 4 refreshes
#    these mentions; this asserts step 4 was actually applied.
CONTRACT_YAML="${4:-contracts/claude-code-parity-apr-v1.yaml}"
contract_ver=""
if [[ -f "${CONTRACT_YAML}" ]]; then
    # Top-level `version: "X.Y.Z"` is canonical. The YAML also has nested
    # metadata.version + amendment_history version: entries (different
    # semantics). Anchor strictly to start-of-line.
    contract_ver=$(awk '/^version:/ {gsub(/[ "]/, "", $2); print $2; exit}' "${CONTRACT_YAML}")
fi

if [[ -n "${contract_ver}" ]]; then
    # README badge: contract-vX.Y.Z-green.svg
    readme_badge=$(grep -oE 'contract-v[0-9]+\.[0-9]+\.[0-9]+-' "${README}" \
        | head -1 \
        | sed -E 's/contract-v(.*)-/\1/')
    if [[ -n "${readme_badge}" && "${readme_badge}" != "${contract_ver}" ]]; then
        report "${README} badge says contract-v${readme_badge} but contract YAML metadata.version is v${contract_ver}"
    fi

    # README status: "Contract at vX.Y.Z"
    readme_status=$(grep -oE 'Contract at v[0-9]+\.[0-9]+\.[0-9]+' "${README}" \
        | head -1 \
        | sed -E 's/Contract at v//')
    if [[ -n "${readme_status}" && "${readme_status}" != "${contract_ver}" ]]; then
        report "${README} 'Contract at v${readme_status}' but contract YAML metadata.version is v${contract_ver}"
    fi

    # CONTRIBUTING status: "Status as of vX.Y.Z"
    contributing_ver=$(grep -oE 'Status as of v[0-9]+\.[0-9]+\.[0-9]+' "${CONTRIBUTING}" \
        | head -1 \
        | sed -E 's/Status as of v//')
    if [[ -n "${contributing_ver}" && "${contributing_ver}" != "${contract_ver}" ]]; then
        report "${CONTRIBUTING} 'Status as of v${contributing_ver}' but contract YAML metadata.version is v${contract_ver}"
    fi

    # Spec status snapshot: "claude-code-parity-apr-v1 vX.Y.Z"
    spec_ver=$(grep -oE 'claude-code-parity-apr-v1[^v]*v[0-9]+\.[0-9]+\.[0-9]+' "${SPEC}" \
        | head -1 \
        | sed -E 's/.*v([0-9]+\.[0-9]+\.[0-9]+)$/\1/')
    if [[ -n "${spec_ver}" && "${spec_ver}" != "${contract_ver}" ]]; then
        report "${SPEC} status snapshot 'claude-code-parity-apr-v1 v${spec_ver}' but contract YAML metadata.version is v${contract_ver}"
    fi
fi

# 8. Corpus count cross-references — measured-parity.json's fixture_count
#    must equal count(NNNN-* fixture dirs in fixtures/canonical/), and
#    the README badge `corpus-N%20%2F%2030` must equal the same N. Same
#    drift class as M-count: when fixtures are added/removed, the
#    measured-parity meta + README badge must follow.
CORPUS_DIR="${5:-fixtures/canonical}"
PARITY_JSON="${6:-fixtures/canonical/measured-parity.json}"

if [[ -d "${CORPUS_DIR}" && -f "${PARITY_JSON}" ]]; then
    actual_corpus=$(find "${CORPUS_DIR}" -mindepth 1 -maxdepth 1 -type d \
        -name '[0-9][0-9][0-9][0-9]-*' \
        | wc -l)
    json_corpus=$(awk '/"fixture_count":/ {gsub(/[ ,]/, "", $2); print $2; exit}' "${PARITY_JSON}")

    if [[ -n "${json_corpus}" && "${json_corpus}" != "${actual_corpus}" ]]; then
        report "${PARITY_JSON} fixture_count=${json_corpus} but actual corpus has ${actual_corpus} NNNN-* dirs"
    fi

    # README badge: corpus-N%20%2F%2030 (URL-encoded "N / 30")
    readme_corpus=$(grep -oE 'corpus-[0-9]+%20%2F%2030' "${README}" \
        | head -1 \
        | sed -E 's/corpus-([0-9]+)%20%2F%2030/\1/')
    if [[ -n "${readme_corpus}" && "${readme_corpus}" != "${actual_corpus}" ]]; then
        report "${README} badge says corpus-${readme_corpus}/30 but actual corpus has ${actual_corpus} NNNN-* dirs"
    fi
fi

# 9a. Contract status badge — README's `status-X-green.svg` badge text
#     (with `__` substituted back to `_`) must equal the YAML's top-level
#     `status:` field. Drift class: contract bumped DRAFT → ACTIVE_RUNTIME
#     (or any equivalent flip) but README badge stays at old value.
if [[ -n "${contract_ver}" ]]; then
    contract_status=$(awk '/^status:/ {print $2; exit}' "${CONTRACT_YAML}")

    readme_status_badge=$(grep -oE 'status-[A-Z_]+-' "${README}" \
        | head -1 \
        | sed -E 's/status-(.*)-/\1/' \
        | sed 's/__/_/g')
    if [[ -n "${contract_status}" && -n "${readme_status_badge}" \
          && "${readme_status_badge}" != "${contract_status}" ]]; then
        report "${README} status badge says '${readme_status_badge}' but contract YAML status is '${contract_status}'"
    fi
fi

# 9. Falsification run history — the most recent open Run row's revision
#    range "M1–MX" must end at the same M as the sub-milestones table
#    tail. Catches the class where a new milestone row gets added but the
#    Run history's "M1–MN" range stays stale (caught manually as drift in
#    9ec1ef3 + this commit).
run_end_m=$(grep -oE '\| Run [0-9]+ \|.*M1[–-]M[0-9]+' "${SPEC}" \
    | tail -1 \
    | sed -E 's/.*M1[–-]M([0-9]+).*/\1/')

if [[ -n "${run_end_m}" && "${run_end_m}" != "${tail_m}" ]]; then
    report "spec Falsification run history latest row says M1–M${run_end_m} but sub-milestones table tail is M${tail_m}"
fi

if [[ "${drift_count}" -gt 0 ]]; then
    cat >&2 <<EOF

check-doc-drift FAIL — ${drift_count} drift(s) found
M22 5-step ritual step 4 (refresh human-readable roll-up views) was not
fully applied. Fix: update each offending file so all M-count and
gate-count cross-references match the sub-milestones table tail.

References:
  - CONTRIBUTING.md § "Bumping the contract (the M22 5-step ritual)"
  - M37 (six commits this script's asserts would have caught)
EOF
    exit 1
fi

echo "check-doc-drift OK"
echo "  sub-milestones table tail:  M${tail_m}"
echo "  stated gate count:          ${stated_gates} (matches ${actual_gates} CCPA row markers)"
if [[ -n "${contract_ver}" ]]; then
    echo "  contract YAML version:      v${contract_ver} (matches all 4 cross-references)"
fi
if [[ -n "${actual_corpus:-}" ]]; then
    echo "  corpus fixture count:       ${actual_corpus} (matches measured-parity.json + README badge)"
fi
