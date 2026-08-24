.PHONY: help build build-release install version bump release test check fmt fmt-check clippy lint validate ci clean man reference website serve

# Package version, parsed from the crate manifest.
VERSION := $(shell awk -F'"' '/^version/ { print $$2; exit }' Cargo.toml)
CORPUS := tests/corpus

help:  ## Display this help screen
	@echo -e "\033[1mAvailable commands:\033[0m\n"
	@grep -E '^[a-z.A-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-22s\033[0m %s\n", $$1, $$2}' | sort

# ==============================================================================
# Build targets
# ==============================================================================

build:  ## Build debug binary
	cargo build

build-release:  ## Build optimized release binary
	cargo build --release

install:  ## Build release binary and install to ~/.cargo/bin
	cargo install --path .

version:  ## Print the current package version
	@echo $(VERSION)

# Bump the crate version and refresh Cargo.lock. Usage: `make bump VERSION=0.0.2`.
bump:  ## Bump package version (usage: make bump VERSION=x.y.z)
	@if [ -z "$(VERSION)" ] || [ "$(VERSION)" = "$(shell awk -F'"' '/^version/ { print $$2; exit }' Cargo.toml)" ]; then \
	    echo "usage: make bump VERSION=x.y.z  (must differ from current $(shell awk -F'"' '/^version/ { print $$2; exit }' Cargo.toml))"; \
	    exit 1; \
	fi
	@sed -i.bak -E 's/^version = "[^"]*"/version = "$(VERSION)"/' Cargo.toml && rm Cargo.toml.bak
	@cargo update -w >/dev/null
	@echo "Bumped typst-doc to $(VERSION)."
	@git diff --stat Cargo.toml Cargo.lock
	@echo ""
	@echo "Next: commit Cargo.toml + Cargo.lock, then 'make release'."

# Tag the current commit and push the tag. Refuses to run on a dirty tree so the
# tag reflects committed code.
release:  ## Tag and push v$(VERSION)
	@test -z "$$(git status --porcelain)" || { echo "working tree is dirty; commit or stash first"; exit 1; }
	@echo "Tagging v$(VERSION) at $$(git rev-parse --short HEAD) and pushing..."
	git tag -a v$(VERSION) -m "Release v$(VERSION)"
	git push origin v$(VERSION)

clean:  ## Remove build artifacts
	cargo clean

# ==============================================================================
# Test and lint targets
# ==============================================================================

test:  ## Run unit and fixture tests
	cargo test

check:  ## Run cargo check (fast compile check)
	cargo check --all-targets

fmt:  ## Format the source tree
	cargo fmt

fmt-check:  ## Check formatting without rewriting files
	cargo fmt --check

clippy:  ## Run clippy with warnings denied
	cargo clippy --all-targets -- -D warnings

lint: fmt-check clippy  ## Run formatting and clippy checks

# Every corpus file must convert to Typst that parses cleanly.
validate:  ## Convert the fixture corpus and check the output parses (DIR=path)
	cargo run --example validate -- $(if $(DIR),$(DIR),$(CORPUS))

ci: lint test validate  ## Run everything CI runs

# ==============================================================================
# Website targets
# ==============================================================================

DOCS_SRC := docs-src
SITE_DIR := docs
HOST ?= 127.0.0.1
PORT ?= 8000

# The man page is generated from the clap command definition, so it cannot
# drift from the binary.
man:  ## Regenerate man/typst-doc.1 from the CLI definition
	cargo run -q --example mangen

# The reference page is the man page rendered by typst-doc itself, offset one
# heading level down so it nests under the page title.
reference: man  ## Regenerate docs-src/reference.typ from man/typst-doc.1
	@{ \
	    printf '#import "/.calepin/calepin.typ" as calepin\n#calepin.setup(eval: false)\n\n'; \
	    printf '#set document(title: [Reference])\n'; \
	    printf '#metadata((\n  summary: "The typst-doc manual page, rendered as Typst by typst-doc itself.",\n)) <website-metadata>\n\n'; \
	    printf '#title()\n\n'; \
	    printf 'Generated from `man/typst-doc.1` by `typst-doc` itself:\n'; \
	    printf '`typst-doc man/typst-doc.1 > docs-src/reference.typ`.\n\n'; \
	    printf '#set heading(offset: 1)\n\n'; \
	    cargo run -q -- man/typst-doc.1; \
	} > $(DOCS_SRC)/reference.typ

website: reference  ## Build the website from docs-src/ into docs/
	calepin compile $(DOCS_SRC) $(SITE_DIR)

serve: website  ## Build and serve the website at http://$(HOST):$(PORT)
	calepin serve $(SITE_DIR) --host $(HOST) --port $(PORT) --open
