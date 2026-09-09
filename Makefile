# Hats — common developer and release targets.
# Uses the standard cargo toolchain; the tap targets are outward-facing and
# guarded (see scripts/update-tap.sh).
#
# The crate lives in cli/, not at the workspace root, so `bump` edits
# cli/Cargo.toml. The lockstep guard in .github/workflows/release.yml requires
# that version to equal the tag, which is what `bump` keeps true.

CARGO ?= cargo
CRATE := cli/Cargo.toml
VERSION := $(shell $(CARGO) metadata --no-deps --format-version 1 2>/dev/null | \
	sed -n 's/.*"name":"hats","version":"\([^"]*\)".*/\1/p')

.PHONY: help build release test lint fmt fmt-check clippy check install clean \
        tap-formula tap-update bump version

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN{FS=":.*?## "}{printf "  \033[36m%-14s\033[0m %s\n", $$1, $$2}'

version: ## Print the current crate version
	@echo $(VERSION)

build: ## Debug build
	$(CARGO) build

release: ## Optimised release build
	$(CARGO) build --release

test: ## Run the test suite (excludes ignored network tests)
	$(CARGO) test --all-features

lint: fmt-check clippy ## Run all lint checks

fmt: ## Format the code
	$(CARGO) fmt --all

fmt-check: ## Check formatting
	$(CARGO) fmt --all --check

clippy: ## Run clippy with warnings as errors
	$(CARGO) clippy --all-targets --all-features -- -D warnings

check: ## Type-check without building artifacts
	$(CARGO) check --all-features

install: ## Install hats from source
	$(CARGO) install --path cli

clean: ## Remove build artifacts
	$(CARGO) clean

bump: ## Bump version, commit, tag and push (usage: make bump V=0.6.0)
	@test -n "$(V)" || { echo "usage: make bump V=x.y.z"; exit 1; }
	@echo "$(V)" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$$' || { echo "V must be x.y.z (no leading v)"; exit 1; }
	@test -z "$$(git status --porcelain)" || { echo "working tree is dirty; commit or stash first"; exit 1; }
	@test "$$(git branch --show-current)" = main || { echo "bump from main only"; exit 1; }
	@! git rev-parse -q --verify "refs/tags/v$(V)" >/dev/null || { echo "tag v$(V) already exists"; exit 1; }
	@$(MAKE) --no-print-directory lint test
	sed -i.bak 's/^version = ".*"/version = "$(V)"/' $(CRATE) && rm -f $(CRATE).bak
	$(CARGO) build --quiet
	git add $(CRATE) Cargo.lock
	git commit -q -m "Bump to $(V)"
	git tag -a "v$(V)" -m "v$(V)"
	git push origin main "v$(V)"
	@echo "Pushed v$(V). Once the Release workflow finishes, run: make tap-update"

tap-formula: ## Generate hats.rb for the current version (no push)
	@HATS_TAP_CONFIRM=0 scripts/update-tap.sh $(VERSION) < /dev/null || true

tap-update: ## Generate hats.rb and push it to the tap as Formula/hats.rb (prompts first)
	scripts/update-tap.sh $(VERSION)
