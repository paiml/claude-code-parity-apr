# claude-code-parity-apr Makefile
#
# `make tier3` runs every gate this repo's CI runs, locally. Use it before
# `git push` to fail fast.
#
# Refs: docs/specifications/claude-code-parity-apr-poc.md
#       § Companion-repo source-of-truth invariants

.PHONY: help fmt fmt-check clippy build test cov pmat-comply pv-validate pin-check pin-check-roundtrip mutants parity tier1 tier2 tier3 install-hooks install-tools

help:
	@echo "claude-code-parity-apr — local gates (mirror of CI)"
	@echo
	@echo "Tiers (mirror aprender's Makefile pattern):"
	@echo "  make tier1          fmt + clippy + check          (<1s)"
	@echo "  make tier2          tier1 + tests                 (<5s)"
	@echo "  make tier3          tier2 + cov + comply + pv     (1-5min)"
	@echo
	@echo "Individual gates:"
	@echo "  make fmt-check      cargo fmt --check"
	@echo "  make clippy         cargo clippy -D warnings"
	@echo "  make test           cargo test --workspace"
	@echo "  make cov            cargo llvm-cov --fail-under-lines 100        (FALSIFY-CCPA-011)"
	@echo "  make pmat-comply    pmat comply check --strict                    (FALSIFY-CCPA-010)"
	@echo "  make pv-validate    pv validate contracts/...yaml                 (FALSIFY-CCPA-012)"
	@echo "  make pin-check      bash scripts/pin-check.sh                     (FALSIFY-CCPA-012)"
	@echo
	@echo "Setup:"
	@echo "  make install-tools  cargo install pmat aprender-contracts-cli cargo-llvm-cov"
	@echo "  make install-hooks  install pre-commit hook (FALSIFY-CCPA-012)"

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

clippy:
	cargo clippy --workspace --all-targets --all-features -- -D warnings

build:
	cargo build --workspace --all-features

test:
	PROPTEST_CASES=256 RUST_MIN_STACK=8388608 cargo test --workspace --all-features

cov:
	cargo llvm-cov --workspace --all-features \
		--fail-under-functions 100 \
		--fail-under-lines 99

pmat-comply:
	@pmat comply check --format json > /tmp/ccpa-comply.json
	@jq -e '.is_compliant == true and ([.checks[] | select(.status == "Fail")] | length == 0)' /tmp/ccpa-comply.json >/dev/null
	@warns=$$(jq '[.checks[] | select(.status == "Warn")] | length' /tmp/ccpa-comply.json); \
	echo "FALSIFY-CCPA-010: is_compliant=true, 0 Fails, $$warns advisory Warns"

pv-validate:
	pv validate contracts/claude-code-parity-apr-v1.yaml
	pv lint contracts/

pin-check:
	bash scripts/pin-check.sh contracts/pin.lock

pin-check-roundtrip:
	bash scripts/pin-check-roundtrip.sh contracts/pin.lock

mutants:
	cargo mutants -p ccpa-differ --no-times --timeout 90

parity:
	@echo "=== canonical corpus (must PASS) ==="
	ccpa corpus fixtures/canonical/
	@echo
	@echo "=== regression corpus (must FAIL) ==="
	@set +e; ccpa corpus fixtures/regression/; code=$$?; set -e; \
	if [ "$$code" -eq 0 ]; then \
		echo "ERROR: regression corpus passed — meter is broken!"; exit 1; \
	else \
		echo "OK: regression corpus exited $$code (drift detected)"; \
	fi

tier1: fmt-check clippy build

tier2: tier1 test

tier3: tier2 cov pmat-comply pv-validate pin-check
	@echo
	@echo "✅ All 4 source-of-truth gates green:"
	@echo "   FALSIFY-CCPA-009  (branch protection — set via GitHub, not local)"
	@echo "   FALSIFY-CCPA-010  pmat comply 100%"
	@echo "   FALSIFY-CCPA-011  line coverage 100%"
	@echo "   FALSIFY-CCPA-012  pv validate + pin-check"

install-hooks:
	bash scripts/install-hooks.sh

install-tools:
	cargo install --locked pmat aprender-contracts-cli cargo-llvm-cov
