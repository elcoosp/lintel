#!/usr/bin/env bash
# M3 release-readiness: the emitted-URL integrity gate. Every URL the
# PRODUCT emits must resolve — fail the gate on any dead link.
#
# The product emits:
#   - the 13 LNT docs URLs (Code::docs_url, default LINTEL_DOCS_BASE/e/LNTxxxx)
#   - the README badge/asset URLs (jsdelivr CDN, GitHub)
#   - the docs-site internal links (the link check in `make docs-site` already
#     covers these; this gate covers the EXTERNAL + emitted URLs)
#   - the composer.json support/source URLs
#
# Usage: bash scripts/emitted-url-gate.sh
#   LINTEL_DOCS_BASE=<base> bash scripts/emitted-url-gate.sh  (CI sets this
#   to the Pages base_url; locally it defaults to the build's file:// path)
#
# Exit 0 = every emitted URL resolves; exit 1 = a dead link was found.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# The docs base: CI sets LINTEL_DOCS_BASE to the Pages base_url. Locally,
# point at the built docs-site/build/ directory (file://) so the gate runs
# without a deploy.
DOCS_BASE="${LINTEL_DOCS_BASE:-}"
if [ -z "$DOCS_BASE" ]; then
    # Build the docs-site locally if not already built, then resolve /e/ pages
    # against the file tree.
    if [ ! -d docs-site/build ]; then
        make docs-site >/dev/null 2>&1 || { echo "FAIL: could not build docs-site"; exit 1; }
    fi
    DOCS_BASE="file://$REPO_ROOT/docs-site/build"
fi
DOCS_BASE="${DOCS_BASE%/}"

step() { printf '\n== %s ==\n' "$1"; }
fail=0

# Check a URL resolves (HTTP 200 for http(s), file exists for file://).
check_url() {
    local url="$1" label="$2"
    if [[ "$url" == file://* ]]; then
        local path="${url#file://}"
        # /e/LNT0101/ -> /e/LNT0101/index.html
        if [ -d "$path" ]; then path="$path/index.html"; fi
        if [ -f "$path" ]; then
            printf '  OK: %s — %s\n' "$label" "$url"
        else
            printf '  DEAD: %s — %s (file not found)\n' "$label" "$url"
            fail=1
        fi
    elif [[ "$url" == https://* || "$url" == http://* ]]; then
        local code
        code=$(curl -sS -A "lintel-url-gate/0.1" -o /dev/null -w '%{http_code}' -L --max-time 20 "$url" 2>/dev/null || echo "000")
        if [ "$code" = "200" ]; then
            printf '  OK: %s — %s\n' "$label" "$url"
        else
            printf '  DEAD: %s — %s (HTTP %s)\n' "$label" "$url" "$code"
            fail=1
        fi
    else
        printf '  SKIP: %s — %s (not http(s) or file://)\n' "$label" "$url"
    fi
}

step "1. the 13 LNT docs URLs (Code::docs_url)"
# Every code the product emits must have a page at DOCS_BASE/e/LNTxxxx/.
for code in LNT0101 LNT0102 LNT0103 LNT0104 LNT0105 LNT0201 LNT0202 LNT0203 LNT0204 LNT0205 LNT0301 LNT0302 LNT0401; do
    check_url "$DOCS_BASE/e/$code/" "$code docs page"
done

step "2. the README badge + asset URLs"
# The README's jsdelivr CDN + GitHub URLs.
check_url "https://cdn.jsdelivr.net/gh/elcoosp/lintel@main/docs/logo.png" "README logo (jsdelivr)"
check_url "https://cdn.jsdelivr.net/gh/elcoosp/lintel@main/docs/demo.gif" "README demo gif (jsdelivr)"
check_url "https://img.shields.io/badge/version-0.1.0-blue?style=flat-square" "README version badge"
check_url "https://img.shields.io/badge/license-MIT_OR_Apache--2.0-blue?style=flat-square" "README license badge"
check_url "https://github.com/elcoosp/lintel/actions/workflows/ci.yml" "README CI badge (GitHub)"

step "3. the composer.json support/source URLs (the published packages)"
for pkg in php/lintel php/runtime php/blade php/twig php/lintel-phpstan php/markup; do
    if [ -f "$pkg/composer.json" ]; then
        # The support/source URLs are GitHub URLs derived from the repo;
        # check the repo URL resolves.
        check_url "https://github.com/elcoosp/lintel" "composer package: $pkg"
    fi
done

step "4. the install.sh release-asset URL"
check_url "https://github.com/elcoosp/lintel/releases" "the releases page (install.sh default)"

printf '\n'
if [ "$fail" = 0 ]; then
    echo "emitted-URL gate: ALL OK — every URL the product emits resolves"
    exit 0
fi
echo "emitted-URL gate: FAILED — a dead link was found (see above)"
exit 1
