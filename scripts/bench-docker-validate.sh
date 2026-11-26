#!/usr/bin/env bash
# The M2 bench-docker preflight (T-14 / §9.2): validates the pinned bench
# image contract BEFORE the CI bench job spends its minutes on it.
#
#   1. Pin format: the Dockerfile's runtime stage is
#      `FROM php:8.6-cli@sha256:<64-hex>` — a digest pin, never a mutable tag.
#   2. Pin truth: the pinned digest is queried against the real Docker
#      registry (registry-1.docker.io, library/php:8.6-cli manifest list) —
#      the pin must be the tag's CURRENT digest, not an invented hex string.
#   3. Entrypoint contract: `CMD ["php", "run.php"]`, `bench/run.php` is
#      tracked, `BENCH_MODE=docker` and `LINTL_BIN=/lintl/lintl` are set,
#      and the results directory the container writes to is the mounted one.
#   4. Build (when a Docker daemon exists): `docker build` the image and
#      smoke `php -v` inside it. Without a daemon the structural + registry
#      checks above still run — the CI bench job performs the build+run on
#      the same Dockerfile, and this script says so explicitly.
#
# Exit 0 only when every check the environment allows has passed.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"
DOCKERFILE="bench/Dockerfile"
fail=0
step() { printf '\n== %s ==\n' "$1"; }
bail() { echo "BENCH-DOCKER FAIL: $1"; fail=1; }

step "1. pin format (digest pin, never a mutable tag)"
PIN=$(grep -E '^FROM php:8\.6-cli@sha256:[0-9a-f]{64}$' "$DOCKERFILE" | tail -1) || PIN=""
if [ -z "$PIN" ]; then
    bail "$DOCKERFILE does not pin the runtime stage as FROM php:8.6-cli@sha256:<64-hex>"
    PIN_DIGEST=""
else
    PIN_DIGEST=$(printf '%s' "$PIN" | sed -E 's/^FROM php:8\.6-cli@sha256:([0-9a-f]{64})$/\1/')
    echo "pinned digest: sha256:$PIN_DIGEST"
fi

step "2. pin truth (the digest must be library/php:8.6-cli's live manifest digest)"
if [ -n "$PIN_DIGEST" ] && command -v curl >/dev/null 2>&1; then
    TOKEN=$(curl -fsSL "https://auth.docker.io/token?service=registry.docker.io&scope=repository:library/php:pull" \
        | python3 -c "import json, sys; print(json.load(sys.stdin)['token'])") || TOKEN=""
    if [ -z "$TOKEN" ]; then
        bail "could not fetch a Docker registry token (registry unreachable?)"
    else
        LIVE=$(curl -fsSI \
            -H "Authorization: Bearer $TOKEN" \
            -H "Accept: application/vnd.docker.distribution.manifest.list.v2+json" \
            -H "Accept: application/vnd.oci.image.index.v1+json" \
            -H "Accept: application/vnd.docker.distribution.manifest.v2+json" \
            -H "Accept: application/vnd.oci.image.manifest.v1+json" \
            "https://registry-1.docker.io/v2/library/php/manifests/8.6-cli" \
            | tr -d '\r' | awk 'tolower($1) == "docker-content-digest:" {print $2}') || LIVE=""
        if [ -z "$LIVE" ]; then
            bail "could not read Docker-Content-Digest for library/php:8.6-cli from the registry"
        elif [ "$LIVE" != "sha256:$PIN_DIGEST" ]; then
            bail "the pin is STALE: the registry serves sha256:$LIVE for php:8.6-cli, the Dockerfile pins sha256:$PIN_DIGEST — re-query and update bench/Dockerfile (and its comment)"
        else
            echo "pin verified: registry serves sha256:$PIN_DIGEST for php:8.6-cli"
        fi
    fi
else
    bail "no pinned digest to verify (fix check 1 first)"
fi

step "3. entrypoint contract"
if grep -q '^CMD \["php", "run.php"\]$' "$DOCKERFILE"; then
    echo "present: CMD [\"php\", \"run.php\"]"
else
    bail "the image's CMD must be [\"php\", \"run.php\"] (the §9.2 harness entrypoint)"
fi
if git ls-files --error-unmatch bench/run.php >/dev/null 2>&1; then
    echo "present: bench/run.php is tracked"
else
    bail "bench/run.php is not tracked — the image would run nothing"
fi
if grep -q '^ENV BENCH_MODE=docker$' "$DOCKERFILE" && grep -q '^ENV LINTL_BIN=/lintl/lintl$' "$DOCKERFILE"; then
    echo "present: BENCH_MODE=docker + LINTL_BIN=/lintl/lintl (the binary arrives via the mounted volume)"
else
    bail "the image must set BENCH_MODE=docker and LINTL_BIN=/lintl/lintl"
fi
if grep -qE '^\s*-v "\$\$PWD/bench/results:/app/bench/results"' Makefile; then
    echo "present: the make bench-docker runner mounts bench/results into /app/bench/results"
else
    bail "the make bench-docker runner must mount \$PWD/bench/results:/app/bench/results (results land in the repo's bench/results/)"
fi

step "4. build + smoke (requires a Docker daemon)"
if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
    docker build -f "$DOCKERFILE" -t lintl-bench-validate . || bail "docker build failed"
    docker run --rm lintl-bench-validate php -v | head -1 || bail "the image cannot run php"
    docker rmi lintl-bench-validate >/dev/null 2>&1 || true
    echo "image built and smoke-ran (php -v inside the pinned image)"
else
    echo "no Docker daemon here — the build+run lane is the CI bench job's step"
    echo "(bench job: make bench-docker builds and runs THIS Dockerfile on ubuntu-latest)"
fi

printf '\n'
if [ "$fail" = 0 ]; then
    echo "bench-docker preflight: ALL OK"
    exit 0
fi
echo "bench-docker preflight: FAILED"
exit 1
