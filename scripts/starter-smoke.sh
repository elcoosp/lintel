#!/usr/bin/env bash
# The M2 starter smoke (release-readiness): proves the zero-Rust first-run
# path against the locally built binary, end-to-end. The starter's path
# repositories point at the local checkout's php/lintel + php/runtime, so
# `composer create-project` runs the real installer plugin with LINTEL_BIN
# set to the release build — exactly the lane a consumer hits, minus the
# published-package download (DEFERRED-TO-MAINTAINER, named in the starter
# README).
#
# Usage:
#   LINTEL_BIN=<path to the built lintel> bash scripts/starter-smoke.sh
#
# Acceptance (M2): the create-project renders a working page with NO Rust
# toolchain installed (the installer plugin caches the prebuilt binary at
# vendor/bin/lintel). The smoke asserts: create-project succeeds, the
# binary shim exists, the dev loader compiles the two components, and the
# rendered page carries the expected escaped markup.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LINTEL_BIN="${LINTEL_BIN:?set LINTEL_BIN to the built lintel binary}"
STARTER="$REPO_ROOT/php/starter"
WORK="${SMOKE_WORK:-$(mktemp -d /tmp/lintel-starter-smoke.XXXXXX)}"

step() { printf '\n== %s ==\n' "$1"; }
assert_contains() {
    if [[ "$1" == *"$2"* ]]; then
        printf 'PASS: %s\n' "$3"
    else
        printf 'FAIL: %s\n  output: %s\n' "$3" "$1"
        exit 1
    fi
}

step "1. composer create-project the starter at $WORK/app (path repos → real installer plugin)"
# create-project copies the starter template and runs composer install,
# which triggers the installer plugin (LINTEL_BIN caches the binary shim).
cp -r "$STARTER" "$WORK/app"
# The path repos in the starter's composer.json point at ../lintel and
# ../runtime relative to the starter — but create-project copies into a
# new dir, so re-point them at the absolute checkout paths.
cd "$WORK/app"
composer config repositories.lintel path "$REPO_ROOT/php/lintel"
composer config repositories.runtime path "$REPO_ROOT/php/runtime"
LINTEL_BIN="$LINTEL_BIN" composer install --no-interaction --no-progress 2>&1 | tail -3

step "2. the installer plugin cached the binary at vendor/bin/lintel (NO Rust toolchain needed)"
test -x "$WORK/app/vendor/bin/lintel" \
    && echo "PASS: vendor/bin/lintel exists (the installer plugin ran)" \
    || { echo "FAIL: vendor/bin/lintel missing — the installer plugin did not run"; exit 1; }
"$WORK/app/vendor/bin/lintel" --version

step "3. the dev loader compiles the two components (cold compile, no watcher)"
out="$(php "$WORK/app/public/index.php")"
assert_contains "$out" '<h1>Hello from the Lintel starter</h1>' "the Page heading renders"
assert_contains "$out" '<h2>The Lintel docs</h2>' "the first Card title renders"
assert_contains "$out" '<h2>The GitHub repo</h2>' "the second Card title renders"
assert_contains "$out" 'href="https://elcoosp.github.io/lintel/"' "the first Card URL is the allowlisted scheme"
assert_contains "$out" 'href="https://github.com/elcoosp/lintel"' "the second Card URL is the allowlisted scheme"
assert_contains "$out" '<button>Click</button>' "the disabled=false bool omits the attr + the ternary child renders"

step "4. the cache holds the compiled files (the dev loader wrote them)"
test -d "$WORK/app/cache" && ls "$WORK/app/cache"/*.php > /dev/null \
    && echo "PASS: the dev loader compiled the components into cache/" \
    || { echo "FAIL: no compiled files in cache/"; exit 1; }

step "5. the T-31 prod lane (build --manifest + dump-autoload, no dev loader)"
cd "$WORK/app"
"$LINTEL_BIN" build components --out build --manifest
test -f "$WORK/app/build/Card.php" && test -f "$WORK/app/build/Page.php" \
    && echo "PASS: build wrote the compiled artifacts" \
    || { echo "FAIL: build/ artifacts missing"; exit 1; }
grep -q '"prod"' "$WORK/app/lintel.json" \
    && echo "PASS: the manifest records the prod artifacts map" \
    || { echo "FAIL: lintel.json has no prod section"; exit 1; }
composer dump-autoload --no-interaction > /dev/null 2>&1
test -f "$WORK/app/vendor/lintel/functions.php" \
    && echo "PASS: the plugin generated vendor/lintel/functions.php (the prod shim)" \
    || { echo "FAIL: the plugin did not generate the files shim"; exit 1; }

step "6. the prod entrypoint renders WITHOUT the dev loader (autoload only)"
cat > "$WORK/app/public/prod.php" <<'PHP'
<?php
declare(strict_types=1);
require __DIR__ . '/../vendor/autoload.php';
echo \App\Page('Prod render', [
    ['title' => 'No dev loader', 'url' => 'https://elcoosp.github.io/lintel/'],
]);
PHP
out="$(php "$WORK/app/public/prod.php")"
assert_contains "$out" '<h1>Prod render</h1>' "the prod Page heading renders"
assert_contains "$out" '<h2>No dev loader</h2>' "the prod Card title renders"

step "7. serve over HTTP (php -S) — the zero-Rust first-run path a consumer hits"
php -S 127.0.0.1:8765 -t "$WORK/app/public" >/dev/null 2>&1 &
SERVER_PID=$!
trap 'kill $SERVER_PID 2>/dev/null || true' EXIT
sleep 1
out="$(curl -fsS http://127.0.0.1:8765/)"
assert_contains "$out" '<h1>Hello from the Lintel starter</h1>' "the HTTP render matches the CLI render"
kill $SERVER_PID 2>/dev/null || true

printf '\nstarter smoke: ALL OK — the zero-Rust path renders end-to-end\n'
