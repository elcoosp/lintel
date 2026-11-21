#!/usr/bin/env bash
# The PHP gates (§3): php -l over every tracked PHP file, composer validate
# over every composer.json, both PHPUnit suites, PHPStan level max.
# Subcommands run one gate (lint|check|test|stan); no argument runs all.
#
# LINTEL_TEST_BIN (optional): the built lintel binary, enabling the
# lintel package's real-compiler PHPUnit lane.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fail=0

lint() {
    echo "== php -l =="
    local bad=0
    # `.lintel.php` sources are compiler INPUT (the JSX language), not
    # PHP — the same exclusion phpstan.neon documents for the bridge's
    # fixtures; `php -l` lints the tracked PHP, never Lintel sources.
    while IFS= read -r -d '' f; do
        if ! php -l "$f" > /dev/null 2>&1; then
            echo "LINT FAIL: $f"
            php -l "$f"
            bad=1
        fi
    done < <(cd "$REPO_ROOT" && {
        git ls-files -z -- '*.php' | grep -zvE '\.lintel\.php$'
        find bench -name '*.php' -not -path '*/vendor/*' -print0 2>/dev/null
        find php \( -path '*/vendor' -o -name '*.lintel.php' \) -prune -o -name '*.php' -not -path '*/.phpunit.cache/*' -print0 2>/dev/null
        find scripts -name '*.php' -print0 2>/dev/null
        find crates -name '*.php' -print0 2>/dev/null
    })
    if [ "$bad" = 0 ]; then echo "php -l: OK"; fi
    return $bad
}

check() {
    echo "== composer validate =="
    local rc=0
    for dir in php/runtime php/lintel php/lintel-phpstan php/markup php/blade php/twig bench; do
        if composer validate --working-dir="$REPO_ROOT/$dir" --no-check-all --no-interaction > /dev/null 2>&1; then
            echo "composer validate $dir: OK"
        else
            echo "composer validate $dir: FAIL"
            composer validate --working-dir="$REPO_ROOT/$dir" --no-check-all --no-interaction
            rc=1
        fi
    done
    return $rc
}

test() {
    echo "== phpunit (php/lintel) =="
    LINTEL_TEST_BIN="${LINTEL_TEST_BIN:-$REPO_ROOT/target/release/lintel}" \
        php "$REPO_ROOT/php/lintel/vendor/bin/phpunit" --configuration "$REPO_ROOT/php/lintel/phpunit.xml" || fail=1
    echo "== phpunit (php/runtime) =="
    php "$REPO_ROOT/php/runtime/vendor/bin/phpunit" --configuration "$REPO_ROOT/php/runtime/phpunit.xml" || fail=1
    echo "== phpunit (php/lintel-phpstan, the §17.1-Q1 bridge) =="
    LINTEL_TEST_BIN="${LINTEL_TEST_BIN:-$REPO_ROOT/target/release/lintel}" \
        php "$REPO_ROOT/php/lintel-phpstan/vendor/bin/phpunit" --configuration "$REPO_ROOT/php/lintel-phpstan/phpunit.xml" || fail=1
    echo "== phpunit (php/markup, the rfc-convergence runtime) =="
    LINTEL_TEST_BIN="${LINTEL_TEST_BIN:-$REPO_ROOT/target/release/lintel}" \
        php "$REPO_ROOT/php/markup/vendor/bin/phpunit" --configuration "$REPO_ROOT/php/markup/phpunit.xml" || fail=1
    echo "== phpunit (php/blade, the §11 Laravel bridge) =="
    LINTEL_TEST_BIN="${LINTEL_TEST_BIN:-$REPO_ROOT/target/release/lintel}" \
        php "$REPO_ROOT/php/blade/vendor/bin/phpunit" --configuration "$REPO_ROOT/php/blade/phpunit.xml" || fail=1
    echo "== phpunit (php/twig, the §11 Symfony bridge) =="
    LINTEL_TEST_BIN="${LINTEL_TEST_BIN:-$REPO_ROOT/target/release/lintel}" \
        php "$REPO_ROOT/php/twig/vendor/bin/phpunit" --configuration "$REPO_ROOT/php/twig/phpunit.xml" || fail=1
}

stan() {
    echo "== phpstan (level max over php/) =="
    php "$REPO_ROOT/php/lintel/vendor/bin/phpstan" analyse \
        --configuration "$REPO_ROOT/phpstan.neon" --no-progress --memory-limit=1G || fail=1
}

gate="${1:-}"
case "$gate" in
    # A subcommand must propagate its own failure — CI calls one gate per
    # step (`bash scripts/php-gates.sh lint`), so an exit-0-on-red here
    # would blind the whole php job.
    lint) lint || fail=1 ;;
    check) check || fail=1 ;;
    test) test ;;
    stan) stan || fail=1 ;;
    *)
        lint || fail=1
        check || fail=1
        test
        stan
        ;;
esac

if [ "$fail" = 0 ]; then
    echo "php gates: ALL OK"
    exit 0
fi
echo "php gates: FAILED"
exit 1
