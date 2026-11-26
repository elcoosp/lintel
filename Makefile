# The §18 operating-manual targets (architecture doc §18): make ci, make
# golden, make fuzz, make bench. This Makefile closes the carried
# "§18 fiction" debt — every target is real.
#
# `make fuzz` wires cargo-fuzz, which task-card T-09 owns: the tool and the
# fuzz targets do not exist yet, so the target preflights and points at T-09
# instead of pretending.

.PHONY: ci golden fuzz bench bench-docker bench-validate lint-php test-php stan-php check-php workflows playground size-budget docs-site claims

workflows:  ## actionlint over .github/workflows (parse-validates every workflow edit)
	@if ! command -v actionlint >/dev/null 2>&1; then \
		echo "make workflows: actionlint is not installed."; \
		echo "Install user-space with: bash <(curl -fsSL https://raw.githubusercontent.com/rhysd/actionlint/main/scripts/download-actionlint.bash)"; \
		exit 1; \
	fi
	actionlint

playground:  ## rebuild the committed T-13 wasm bundle into playground/assets/
	@if ! rustup target list --installed 2>/dev/null | grep -q wasm32-unknown-unknown; then \
		echo "make playground: the wasm32-unknown-unknown target is missing (rustup target add wasm32-unknown-unknown)"; \
		exit 1; \
	fi
	cargo build --profile wasm-release --target wasm32-unknown-unknown -p lintl-wasm
	wasm-bindgen --target web --no-typescript \
		--out-dir playground/assets \
		target/wasm32-unknown-unknown/wasm-release/lintl_wasm.wasm

size-budget:  ## the T-13 acceptance test: bundle + page gzip under 1 MB
	LINTL_WASM_SIZE_CHECK=1 cargo test -p lintl-wasm --test size_budget

docs-site:  ## build + link-check the T-17 docs site into docs-site/build/
	cargo run -p lintl-docs --quiet

claims:  ## the maturity sweep (M0-corrected for 0.1.0): no stale maturity claims outside the allowlist
	bash scripts/claims-sweep.sh

ci:  ## workflows, fmt, clippy, test, docs-site link check + maturity sweep, php lint, phpunit, phpstan
	$(MAKE) workflows
	cargo fmt --all --check
	cargo clippy --workspace --all-targets --all-features -- -D warnings -W clippy::pedantic
	cargo test --workspace --all-features
	$(MAKE) docs-site
	$(MAKE) claims
	bash scripts/php-gates.sh

golden:  ## the executable golden fixtures + the line-preservation invariant
	cargo test -p lintl-core --test golden --test line_preservation

fuzz:  ## 60s smoke (requires cargo-fuzz + the T-09 fuzz targets)
	@if ! command -v cargo-fuzz >/dev/null 2>&1; then \
		echo "make fuzz: cargo-fuzz is not installed."; \
		echo "The fuzz targets themselves are task-card T-09 (differential + fuzz), not started."; \
		echo "Install with: cargo install cargo-fuzz"; \
		exit 1; \
	fi
	cargo +nightly fuzz run compile_fuzz -- -max_total_time=60

bench:  ## Docker when a daemon exists (bench-docker), native fallback otherwise
	@if docker info >/dev/null 2>&1; then \
		$(MAKE) bench-docker; \
	else \
		echo "make bench: no Docker daemon — running the harness natively"; \
		echo "(results are tagged native; the CI bench job runs the pinned image)"; \
		test -x target/release/lintl || cargo build --release -p lintl-cli; \
		PHP_BIN=php LINTL_BIN="$$PWD/target/release/lintl" BENCH_MODE=native php bench/run.php; \
	fi

bench-docker:  ## the pinned reproduction, always Docker (no native fallback; the CI bench job runs exactly this)
	@if ! docker info >/dev/null 2>&1; then \
		echo "make bench-docker: no Docker daemon — this target is the pinned php:8.6-cli reproduction"; \
		echo "(docker build -f bench/Dockerfile -t lintl-bench . && docker run --rm lintl-bench)"; \
		echo "run \`make bench\` for the native fallback, or install/start Docker"; \
		exit 1; \
	fi
	docker build -f bench/Dockerfile -t lintl-bench . && \
	docker run --rm \
		-v "$$PWD/target/release/lintl:/lintl/lintl:ro" \
		-v "$$PWD/bench/results:/app/bench/results" \
		lintl-bench

bench-validate:  ## the M2 preflight: pin format, registry digest, entrypoint contract (builds the image when Docker exists)
	bash scripts/bench-docker-validate.sh

lint-php:  ## php -l over every tracked PHP file (no vendor)
	@bash scripts/php-gates.sh lint

test-php:  ## both PHPUnit suites
	@bash scripts/php-gates.sh test

stan-php:  ## PHPStan level max
	@bash scripts/php-gates.sh stan

check-php:  ## composer validate over every composer.json
	@bash scripts/php-gates.sh check
