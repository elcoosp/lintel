# The §9.2 benchmark image: PHP 8.6 pinned by digest (publishing-rule
# reproduction). The digest is the manifest list digest of the `8.6-cli`
# tag queried from the Docker registry on 2026-11-20 — not an invented pin.
#
# Build & run (make bench):
#   docker build -f bench/Dockerfile -t lintel-bench bench/
#   docker run --rm \
#     -v "$PWD/target/release/lintel:/lintel/lintel:ro" \
#     -v "$PWD/bench/results:/app/bench/results" lintel-bench

# Stage 1: the competitor tooling lane (Blade/Twig/Plates) from the lock file.
FROM composer:2 AS deps
WORKDIR /app
COPY composer.json composer.lock ./
RUN composer install --no-interaction --no-progress

# Stage 2: the pinned runtime.
FROM php:8.6-cli@sha256:d84acf3155d907afaa5afa9b9457c5f12016b6dc7ade84659f40cdc505cc087d
WORKDIR /app/bench
COPY --from=deps /app/vendor ./vendor
COPY lib ./lib
COPY run.php ./

# The lintel binary is mounted at /lintel/lintel by the runner; results are
# written to the mounted /app/bench/results.
ENV LINTEL_BIN=/lintel/lintel
ENV BENCH_MODE=docker

CMD ["php", "run.php"]
