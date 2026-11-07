# Everyday commands. The checks are the steps of scripts/ci.sh (what CI
# runs); `make help` lists every target.
#
# Tune with variables: `make test JOBS=4`, `make e2e E2E_WORKERS=2`.

JOBS ?= 8
E2E_WORKERS ?= 4
CI_MEMORY_LIMIT ?= 4000000000
PRETTIER := js/node_modules/.bin/prettier

export CARGO_BUILD_JOBS := $(JOBS)
export E2E_WORKERS
export CI_MEMORY_LIMIT

.DEFAULT_GOAL := help
.PHONY: help install dev build demo demo-release packages examples \
	check ci rust fmt fmt-check clippy test languages minimal optional docs \
	typecheck e2e e2e-only screenshots docs-generate sizes clean

help: ## List the targets
	@grep -hE '^[a-zA-Z0-9_-]+:.*?## ' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "} {printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2}'

# --- Setup -----------------------------------------------------------------

install: ## Install the npm workspaces, the e2e tools and Playwright's Chromium
	cd js && npm ci
	cd e2e && npm ci && npx playwright install chromium

# --- Run and build ---------------------------------------------------------

dev: ## Serve the demo with live reload (trunk serve)
	trunk serve --open

demo: ## Build the demo (debug, what the e2e tests load)
	trunk build

demo-release: ## Build the demo optimized (what the screenshots use)
	trunk build --release

packages: ## Build the npm packages (vanilla, React, Vue, Svelte) and their example pages
	cd js && npm run build

examples: ## Bundle only the example pages (after `make packages` once)
	cd js && npm run examples

build: demo packages ## Build the demo and the npm packages

# --- Checks (the steps of scripts/ci.sh) -------------------------------------

check: ## Every Rust and docs check CI runs (scripts/ci.sh)
	scripts/ci.sh

ci: check typecheck e2e ## Everything CI runs, browser tests included

rust: ## The Rust checks only: fmt, clippy, tests, each language, minimal, optional
	scripts/ci.sh rust optional

fmt: ## Format Rust and Markdown
	cargo fmt --all
	cd js/config-editor && cargo fmt
	git ls-files '*.md' | xargs $(PRETTIER) --log-level warn --write

fmt-check: ## Check formatting (scripts/ci.sh fmt)
	scripts/ci.sh fmt

clippy: ## Clippy in every configuration, warnings denied
	scripts/ci.sh clippy

test: ## The unit, integration and doc tests
	scripts/ci.sh test

languages: ## Each language feature built and tested on its own
	scripts/ci.sh languages

minimal: ## Every language without schema validation
	scripts/ci.sh minimal

optional: ## Each optional feature off, then all of them off
	scripts/ci.sh optional

docs: ## Generated docs current and well formed (contents, gallery, tables, Prettier)
	scripts/ci.sh docs

typecheck: ## Type-check the READMEs' examples (tsc, vue-tsc, svelte-check) and the Svelte package
	cd js && npm run typecheck

e2e: build e2e-only ## Build, then run the browser tests

e2e-only: ## Run the browser tests on the current builds
	cd e2e && npx playwright test

# --- Generated files ---------------------------------------------------------

docs-generate: ## Rewrite every generated doc: contents lists, settings tables, gallery page
	node scripts/readme-toc.mjs
	UPDATE_GENERATED=1 cargo test -p config-editor-js --test settings
	node e2e/screenshots.mjs --gallery
	git ls-files '*.md' | xargs $(PRETTIER) --log-level warn --write

screenshots: demo-release packages ## Retake every screenshot in docs/screenshots
	cd e2e && npm run screenshots

sizes: ## Measure the wasm sizes and rewrite the crate README's table
	scripts/wasm-sizes.sh --write

# --- Cleanup -------------------------------------------------------------------

clean: ## Remove build outputs (cargo target, demo, npm builds, test results)
	cargo clean
	rm -rf dist e2e/.standalone e2e/test-results js/config-editor/pkg js/config-editor-react/dist js/config-editor-vue/dist js/config-editor-svelte/dist js/examples/dist
