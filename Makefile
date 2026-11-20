# The §18 operating-manual targets (architecture doc §18): make ci, make
# golden, make fuzz, make bench. This Makefile closes the carried
# "§18 fiction" debt — every target is real.
#
# `make fuzz` wires cargo-fuzz, which task-card T-09 owns: the tool and the
# fuzz targets do not exist yet, so the target preflights and points at T-09
# instead of pretending.

.PHONY: ci golden fuzz bench lint-php test-php stan-php check-php

ci:  ## fmt, clippy -D warnings, test, php lint, phpunit, phpstan
	cargo fmt --all --check
	cargo clippy --workspace --all-targets --all-features -- -D warnings -W clippy::pedantic
	cargo test --workspace --all-features
	bash scripts/php-gates.sh

golden:  ## the executable golden fixtures + the line-preservation invariant
	cargo test -p lintel-core --test golden --test line_preservation

fuzz:  ## 60s smoke (requires cargo-fuzz + the T-09 fuzz targets)
	@if ! command -v cargo-fuzz >/dev/null 2>&1; then \
		echo "make fuzz: cargo-fuzz is not installed."; \
		echo "The fuzz targets themselves are task-card T-09 (differential + fuzz), not started."; \
		echo "Install with: cargo install cargo-fuzz"; \
		exit 1; \
	fi
	cargo fuzz run compile_fuzz -- -max_total_time=60

bench:  ## Docker, pinned PHP 8.6 digest; writes bench/results/
	@if docker info >/dev/null 2>&1; then \
		docker build -f bench/Dockerfile -t lintel-bench bench/ && \
		docker run --rm \
			-v "$$PWD/target/release/lintel:/lintel/lintel:ro" \
			-v "$$PWD/bench/results:/app/bench/results" \
			lintel-bench; \
	else \
		echo "make bench: no Docker daemon — running the harness natively"; \
		echo "(results are tagged native; the CI bench job runs the pinned image)"; \
		test -x target/release/lintel || cargo build --release -p lintel-cli; \
		PHP_BIN=php LINTEL_BIN="$$PWD/target/release/lintel" BENCH_MODE=native php bench/run.php; \
	fi

lint-php:  ## php -l over every tracked PHP file (no vendor)
	@bash scripts/php-gates.sh lint

test-php:  ## both PHPUnit suites
	@bash scripts/php-gates.sh test

stan-php:  ## PHPStan level max
	@bash scripts/php-gates.sh stan

check-php:  ## composer validate over every composer.json
	@bash scripts/php-gates.sh check
