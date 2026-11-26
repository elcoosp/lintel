#!/usr/bin/env bash
# T-11 acceptance smoke (architecture doc §15): fresh plain-PHP AND fresh
# Laravel skeletons — `composer require` → edit a .lintl.php → refresh
# re-compiles with NO watcher.
#
#   LINTL_BIN=<path to the built lintl> bash scripts/dx-smoke.sh [--skip-laravel]
#
# The skeleton packages come from composer path repositories pointing at
# this checkout (php/lintl, php/runtime), so the smoke runs the real
# installer plugin (LINTL_BIN override caches vendor/bin/lintl) and the
# real dev loader exactly as a consumer would.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LINTL_BIN="${LINTL_BIN:?set LINTL_BIN to the built lintl binary}"
WORK="${SMOKE_WORK:-$(mktemp -d /tmp/lintl-dx-smoke.XXXXXX)}"
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
        "php": ">=8.6"
    },
    "config": {
        "sort-packages": true
    }
}
JSON
(cd "$WORK/plain" \
    && composer config repositories.lintl path "$REPO_ROOT/php/lintl" \
    && composer config repositories.runtime path "$REPO_ROOT/php/runtime" \
    && composer config minimum-stability dev \
    && composer config allow-plugins.lintl/lintl true \
    && LINTL_BIN="$LINTL_BIN" composer require --dev lintl/lintl \
    && LINTL_BIN="$LINTL_BIN" composer require lintl/runtime \
    && composer dump-autoload)

test -x "$WORK/plain/vendor/bin/lintl" \
    && echo "PASS: the installer plugin cached the binary at vendor/bin/lintl (LINTL_BIN override)" \
    || { echo "FAIL: vendor/bin/lintl missing — the installer plugin did not run"; exit 1; }

cat > "$WORK/plain/lintl.json" <<'JSON'
{"functions": ["components/*.lintl.php"]}
JSON
cat > "$WORK/plain/components/hello.lintl.php" <<'PHP'
<?php

namespace Smoke;

use Lintl\Html;

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
\Lintl\Dev\Loader::boot(__DIR__ . '/../cache', projectRoot: dirname(__DIR__));
echo \Smoke\hello('Plain PHP');
PHP

step "plain: first render (compiles via vendor/bin/lintl)"
out="$(php "$WORK/plain/public/index.php")"
assert_contains "$out" '<section class="hello">' "plain first render contains the component markup"
assert_contains "$out" '<h1>Hello, Plain PHP</h1>' "plain first render interpolates the prop"

step "plain: edit + refresh — recompiled with NO watcher"
# The cache key is mtime+size (§6.10): the edit changes the size and crosses
# a second boundary, so the stale check can actually observe it.
new_block='<p>v2 — edited on refresh; longer line so the size key turns over.</p>'
sed -i "s|<p>Compiled ahead of time; escaped by default.</p>|$new_block|" "$WORK/plain/components/hello.lintl.php"
sleep 1.1
out="$(php "$WORK/plain/public/index.php")"
assert_contains "$out" 'v2 — edited on refresh' "plain refresh re-compiled the edited component"
assert_contains "$out" '<h1>Hello, Plain PHP</h1>' "plain refresh still renders the prop"
test -d "$WORK/plain/cache" && ls "$WORK/plain/cache"/*.php > /dev/null \
    && echo "PASS: cache holds the compiled files"

# ---- plain: the T-31 prod lane (function components via the manifest) ------
step "plain: T-31 prod lane — build --manifest + dump-autoload, no dev loader"
(cd "$WORK/plain" \
    && "$LINTL_BIN" build components --out build --manifest)
test -f "$WORK/plain/build/hello.php" \
    && echo "PASS: the build wrote the compiled artifact" \
    || { echo "FAIL: build/hello.php missing"; exit 1; }
grep -q '"prod"' "$WORK/plain/lintl.json" \
    && echo "PASS: the manifest records the prod artifacts map" \
    || { echo "FAIL: lintl.json has no prod section"; exit 1; }
(cd "$WORK/plain" && composer dump-autoload --no-interaction > /dev/null 2>&1)
test -f "$WORK/plain/vendor/lintl/functions.php" \
    && echo "PASS: the plugin generated vendor/lintl/functions.php" \
    || { echo "FAIL: the plugin did not generate the files shim"; exit 1; }
cat > "$WORK/plain/public/prod.php" <<'PHP'
<?php

declare(strict_types=1);

require __DIR__ . '/../vendor/autoload.php';
// The prod entrypoint: NO Loader::boot, NO manual require — the function
// component exists because the Composer `files` autoload requires the
// compiled artifacts the manifest recorded (T-31).
echo \Smoke\hello('Prod');
PHP
out="$(php "$WORK/plain/public/prod.php")"
assert_contains "$out" '<section class="hello">' "plain prod render autoloads the function component"
assert_contains "$out" '<h1>Hello, Prod</h1>' "plain prod render interpolates the prop"

# ---- fresh Laravel skeleton -----------------------------------------------
if [ "$SKIP_LARAVEL" = "--skip-laravel" ]; then
    printf '\nSKIP: laravel half (--skip-laravel)\n'
    exit 0
fi

step "fresh Laravel skeleton at $WORK/laravel"
composer create-project laravel/laravel "$WORK/laravel" --no-interaction --prefer-dist --no-scripts > "$WORK/laravel-create.log" 2>&1 \
    || { echo "FAIL: composer create-project laravel/laravel (see $WORK/laravel-create.log)"; tail -5 "$WORK/laravel-create.log"; exit 1; }

(cd "$WORK/laravel" \
    && composer config repositories.lintl path "$REPO_ROOT/php/lintl" \
    && composer config repositories.runtime path "$REPO_ROOT/php/runtime" \
    && composer config minimum-stability dev \
    && composer config allow-plugins.lintl/lintl true \
    && LINTL_BIN="$LINTL_BIN" composer require --dev lintl/lintl \
    && LINTL_BIN="$LINTL_BIN" composer require lintl/runtime \
    && composer dump-autoload)

test -x "$WORK/laravel/vendor/bin/lintl" \
    && echo "PASS: the installer plugin cached the binary inside the Laravel skeleton" \
    || { echo "FAIL: vendor/bin/lintl missing in the Laravel skeleton"; exit 1; }

mkdir -p "$WORK/laravel/app/Lintl"
cat > "$WORK/laravel/lintl.json" <<'JSON'
{"functions": ["app/Lintl/*.lintl.php"]}
JSON
cat > "$WORK/laravel/app/Lintl/hello.lintl.php" <<'PHP'
<?php

namespace App\Lintl;

use Lintl\Html;

function hello(string $name): Html
{
    return <div class="laravel-hello">
        <h1>Hello, {$name}</h1>
        <p>JSX inside Laravel, no watcher attached.</p>
    </div>;
}
PHP
cat > "$WORK/laravel/lintl-smoke.php" <<'PHP'
<?php

declare(strict_types=1);

require __DIR__ . '/vendor/autoload.php';
\Lintl\Dev\Loader::boot(__DIR__ . '/bootstrap/cache/lintl', projectRoot: __DIR__);
echo \App\Lintl\hello('Laravel');
PHP

step "laravel: first render"
out="$(php "$WORK/laravel/lintl-smoke.php")"
assert_contains "$out" '<div class="laravel-hello">' "laravel first render contains the component markup"
assert_contains "$out" '<h1>Hello, Laravel</h1>' "laravel first render interpolates the prop"

step "laravel: edit + refresh — recompiled with NO watcher"
sed -i 's|<p>JSX inside Laravel, no watcher attached.</p>|<p>v2 — recompiled on refresh; longer line so the size key turns over.</p>|' "$WORK/laravel/app/Lintl/hello.lintl.php"
sleep 1.1
out="$(php "$WORK/laravel/lintl-smoke.php")"
assert_contains "$out" 'v2 — recompiled on refresh' "laravel refresh re-compiled the edited component"

step "laravel: the framework still boots with the loader wired"
out="$(php "$WORK/laravel/artisan" route:list 2>&1 || true)"
assert_contains "$out" "GET" "artisan route:list still runs"

# ---- laravel: the T-31 acceptance ------------------------------------------
# A fresh Laravel skeleton with a function component in app/Components/
# autoloads and renders on refresh without manual `require`: `lintl build
# --manifest` records the component, the Composer plugin injects the
# `files` autoload, and the prod entrypoint calls the function directly.
step "laravel: T-31 — app/Components function component via the manifest"
mkdir -p "$WORK/laravel/app/Components"
cat > "$WORK/laravel/app/Components/hello.lintl.php" <<'PHP'
<?php

namespace App\Components;

use Lintl\Html;

function hello(string $name): Html
{
    return <div class="laravel-t31">
        <h1>T-31, {$name}</h1>
        <p>Function components autoload in prod via the manifest.</p>
    </div>;
}
PHP
(cd "$WORK/laravel" \
    && "$LINTL_BIN" build app/Components --out bootstrap/cache/lintl --manifest)
test -f "$WORK/laravel/bootstrap/cache/lintl/hello.php" \
    && echo "PASS: the artifact landed in bootstrap/cache/lintl" \
    || { echo "FAIL: bootstrap/cache/lintl/hello.php missing"; exit 1; }
(cd "$WORK/laravel" && composer dump-autoload --no-interaction > /dev/null 2>&1)
test -f "$WORK/laravel/vendor/lintl/functions.php" \
    && echo "PASS: the plugin generated the files shim inside the Laravel skeleton" \
    || { echo "FAIL: no vendor/lintl/functions.php in the Laravel skeleton"; exit 1; }
grep -q "lintl/functions.php" "$WORK/laravel/vendor/composer/autoload_files.php" \
    && echo "PASS: autoload_files.php includes the shim" \
    || { echo "FAIL: autoload_files.php does not require the shim"; exit 1; }
cat > "$WORK/laravel/lintl-t31.php" <<'PHP'
<?php

declare(strict_types=1);

require __DIR__ . '/vendor/autoload.php';
// The T-31 acceptance entrypoint: NO Loader::boot, NO manual require.
echo \App\Components\hello('Laravel');
PHP
out="$(php "$WORK/laravel/lintl-t31.php")"
assert_contains "$out" '<div class="laravel-t31">' "laravel T-31 render autoloads the function component"
assert_contains "$out" '<h1>T-31, Laravel</h1>' "laravel T-31 render interpolates the prop"

step "laravel: T-31 — refresh: edit, rebuild, redump, re-render"
sed -i 's|<p>Function components autoload in prod via the manifest.</p>|<p>v2 — rebuilt through the manifest on refresh.</p>|' "$WORK/laravel/app/Components/hello.lintl.php"
(cd "$WORK/laravel" \
    && "$LINTL_BIN" build app/Components --out bootstrap/cache/lintl --manifest \
    && composer dump-autoload --no-interaction > /dev/null 2>&1)
out="$(php "$WORK/laravel/lintl-t31.php")"
assert_contains "$out" 'v2 — rebuilt through the manifest on refresh' "laravel T-31 refresh re-rendered the edited component"

printf '\nALL T-11 SMOKE CHECKS PASSED\n'
