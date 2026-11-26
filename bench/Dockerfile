# The §9.2 benchmark image: PHP 8.6 pinned by digest (publishing-rule
# reproduction). The digest is the manifest list digest of the `8.6-cli`
# tag queried from the Docker registry on 2026-11-20 — not an invented pin.
#
# The build context is the repository root (make bench): the path repository
# `../php/runtime` in bench/composer.json must exist next to the bench
# package inside the image, so stage 1 mirrors that layout.
#
# Build & run (make bench):
#   docker build -f bench/Dockerfile -t lintl-bench .
#   docker run --rm \
#     -v "$PWD/target/release/lintl:/lintl/lintl:ro" \
#     -v "$PWD/bench/results:/app/bench/results" lintl-bench

# Stage 1: the competitor tooling lane (Blade/Twig/Plates) from the lock file.
FROM composer:2 AS deps
WORKDIR /src/bench
COPY bench/composer.json bench/composer.lock ./
COPY php/runtime /src/php/runtime
# roadrunner/worker + goridge (require-dev, pulled transitively by
# spiral/roadrunner-http) declare ext-sockets, which the composer image
# does not bundle. The bench harness drives the simulated RoadRunnerFlushTransport
# (a pure-PHP queue, no real socket), so the extension is not needed at
# runtime — ignore the platform req here so `composer install` resolves the
# locked vendor tree the harness requires. --ignore-platform-req=php is
# needed because the composer:2 image ships PHP 8.5 while the lock file
# requires >=8.6 (the actual runtime is stage 2, the pinned php:8.6-cli).
RUN composer install --no-interaction --no-progress --ignore-platform-req=php --ignore-platform-req=ext-sockets

# Stage 2: the pinned runtime.
FROM php:8.6-cli@sha256:d84acf3155d907afaa5afa9b9457c5f12016b6dc7ade84659f40cdc505cc087d
WORKDIR /app/bench
COPY --from=deps /src/bench/vendor ./vendor
COPY bench/lib ./lib
COPY bench/run.php ./
# The `lintl/runtime` path repository is installed as a relative symlink
# (`vendor/lintl/runtime -> ../../../php/runtime/`); mirroring php/runtime at
# /app/php/runtime keeps that symlink resolvable so the autoload `files`
# include (`src/functions.php`) loads in the pinned image.
COPY php/runtime /app/php/runtime

# The lintl binary is mounted at /lintl/lintl by the runner; results are
# written to the mounted /app/bench/results.
ENV LINTL_BIN=/lintl/lintl
ENV BENCH_MODE=docker

CMD ["php", "run.php"]
