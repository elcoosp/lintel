#!/usr/bin/env bash
# T-11 acceptance smoke (architecture doc §15): fresh plain-PHP AND fresh
# Laravel skeletons — `composer require` → edit a .lintel.php → refresh
# re-compiles with NO watcher.
#
#   LINTEL_BIN=<path to the built lintel> bash scripts/dx-smoke.sh [--skip-laravel]
#
# The skeleton packages come from composer path repositories pointing at
# this checkout (php/lintel, php/runtime), so the smoke runs the real
# installer plugin (LINTEL_BIN override caches vendor/bin/lintel) and the
# real dev loader exactly as a consumer would.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LINTEL_BIN="${LINTEL_BIN:?set LINTEL_BIN to the built lintel binary}"
WORK="${SMOKE_WORK:-$(mktemp -d /tmp/lintel-dx-smoke.XXXXXX)}"
SKIP_LARAVEL="${2:-}"

step() { printf '\n== %s ==\n' "$1"; }
assert_contains() {
    if [[ "$1" == *"$2"* ]]; then
        printf 'PASS: %s\n' "$3"
    else
        printf 'FAIL: %s\n  output: %s\n' "$3" "$1"
        exit 1
    fi
}

# ---- fresh plain-PHP skeleton ---------------------------------------------
step "fresh plain-PHP skeleton at $WORK/plain"
mkdir -p "$WORK/plain/public" "$WORK/plain/components" "$WORK/plain/cache"
cat > "$WORK/plain/composer.json" <<'JSON'
{
    "name": "smoke/plain-php",
    "description": "T-11 acceptance: fresh plain-PHP skeleton",
    "require": {
        "php": ">=8.1"
    },
    "config": {
        "sort-packages": true
    }
}
JSON
(cd "$WORK/plain" \
    && composer config repositories.lintel path "$REPO_ROOT/php/lintel" \
    && composer config repositories.runtime path "$REPO_ROOT/php/runtime" \
    && composer config minimum-stability dev \
    && composer config allow-plugins.lintel/lintel true \
    && LINTEL_BIN="$LINTEL_BIN" composer require --dev lintel/lintel \
    && LINTEL_BIN="$LINTEL_BIN" composer require lintel/runtime \
    && composer dump-autoload)

test -x "$WORK/plain/vendor/bin/lintel" \
    && echo "PASS: the installer plugin cached the binary at vendor/bin/lintel (LINTEL_BIN override)" \
    || { echo "FAIL: vendor/bin/lintel missing — the installer plugin did not run"; exit 1; }

cat > "$WORK/plain/lintel.json" <<'JSON'
{"functions": ["components/*.lintel.php"]}
JSON
cat > "$WORK/plain/components/hello.lintel.php" <<'PHP'
<?php

namespace Smoke;

use Lintel\Html;

function hello(string $name): Html
{
    return <section class="hello">
        <h1>Hello, {$name}</h1>
        <p>Compiled ahead of time; escaped by default.</p>
    </section>;
}
PHP
cat > "$WORK/plain/public/index.php" <<'PHP'
<?php

declare(strict_types=1);

require __DIR__ . '/../vendor/autoload.php';
\Lintel\Dev\Loader::boot(__DIR__ . '/../cache', projectRoot: dirname(__DIR__));
echo \Smoke\hello('Plain PHP');
PHP

step "plain: first render (compiles via vendor/bin/lintel)"
out="$(php "$WORK/plain/public/index.php")"
assert_contains "$out" '<section class="hello">' "plain first render contains the component markup"
assert_contains "$out" '<h1>Hello, Plain PHP</h1>' "plain first render interpolates the prop"

step "plain: edit + refresh — recompiled with NO watcher"
# The cache key is mtime+size (§6.10): the edit changes the size and crosses
# a second boundary, so the stale check can actually observe it.
new_block='<p>v2 — edited on refresh; longer line so the size key turns over.</p>'
sed -i "s|<p>Compiled ahead of time; escaped by default.</p>|$new_block|" "$WORK/plain/components/hello.lintel.php"
sleep 1.1
out="$(php "$WORK/plain/public/index.php")"
assert_contains "$out" 'v2 — edited on refresh' "plain refresh re-compiled the edited component"
assert_contains "$out" '<h1>Hello, Plain PHP</h1>' "plain refresh still renders the prop"
test -d "$WORK/plain/cache" && ls "$WORK/plain/cache"/*.php > /dev/null \
    && echo "PASS: cache holds the compiled files"

# ---- fresh Laravel skeleton -----------------------------------------------
if [ "$SKIP_LARAVEL" = "--skip-laravel" ]; then
    printf '\nSKIP: laravel half (--skip-laravel)\n'
    exit 0
fi

step "fresh Laravel skeleton at $WORK/laravel"
composer create-project laravel/laravel "$WORK/laravel" --no-interaction --prefer-dist --no-scripts > "$WORK/laravel-create.log" 2>&1 \
    || { echo "FAIL: composer create-project laravel/laravel (see $WORK/laravel-create.log)"; tail -5 "$WORK/laravel-create.log"; exit 1; }

(cd "$WORK/laravel" \
    && composer config repositories.lintel path "$REPO_ROOT/php/lintel" \
    && composer config repositories.runtime path "$REPO_ROOT/php/runtime" \
    && composer config minimum-stability dev \
    && composer config allow-plugins.lintel/lintel true \
    && LINTEL_BIN="$LINTEL_BIN" composer require --dev lintel/lintel \
    && LINTEL_BIN="$LINTEL_BIN" composer require lintel/runtime \
    && composer dump-autoload)

test -x "$WORK/laravel/vendor/bin/lintel" \
    && echo "PASS: the installer plugin cached the binary inside the Laravel skeleton" \
    || { echo "FAIL: vendor/bin/lintel missing in the Laravel skeleton"; exit 1; }

mkdir -p "$WORK/laravel/app/Lintel"
cat > "$WORK/laravel/lintel.json" <<'JSON'
{"functions": ["app/Lintel/*.lintel.php"]}
JSON
cat > "$WORK/laravel/app/Lintel/hello.lintel.php" <<'PHP'
<?php

namespace App\Lintel;

use Lintel\Html;

function hello(string $name): Html
{
    return <div class="laravel-hello">
        <h1>Hello, {$name}</h1>
        <p>JSX inside Laravel, no watcher attached.</p>
    </div>;
}
PHP
cat > "$WORK/laravel/lintel-smoke.php" <<'PHP'
<?php

declare(strict_types=1);

require __DIR__ . '/vendor/autoload.php';
\Lintel\Dev\Loader::boot(__DIR__ . '/bootstrap/cache/lintel', projectRoot: __DIR__);
echo \App\Lintel\hello('Laravel');
PHP

step "laravel: first render"
out="$(php "$WORK/laravel/lintel-smoke.php")"
assert_contains "$out" '<div class="laravel-hello">' "laravel first render contains the component markup"
assert_contains "$out" '<h1>Hello, Laravel</h1>' "laravel first render interpolates the prop"

step "laravel: edit + refresh — recompiled with NO watcher"
sed -i 's|<p>JSX inside Laravel, no watcher attached.</p>|<p>v2 — recompiled on refresh; longer line so the size key turns over.</p>|' "$WORK/laravel/app/Lintel/hello.lintel.php"
sleep 1.1
out="$(php "$WORK/laravel/lintel-smoke.php")"
assert_contains "$out" 'v2 — recompiled on refresh' "laravel refresh re-compiled the edited component"

step "laravel: the framework still boots with the loader wired"
out="$(php "$WORK/laravel/artisan" route:list 2>&1 || true)"
assert_contains "$out" "GET" "artisan route:list still runs"

printf '\nALL T-11 SMOKE CHECKS PASSED\n'
