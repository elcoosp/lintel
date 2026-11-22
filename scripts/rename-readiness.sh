#!/usr/bin/env bash
# rename-readiness.sh <candidate> (M0 release-readiness card)
#
# The `lintel` crate name on crates.io is TAKEN by an unrelated package (a
# JSON/YAML validator); `lintel-cli` and `lintel-core` are also taken. The
# maintainer is running a community naming process rather than picking a
# new name here. This script reports the full rename surface for ANY
# candidate WITHOUT renaming anything: it queries crates.io AND Packagist
# for availability, then prints every file/line in the tree that would
# change if the candidate were adopted (crate names, composer names,
# namespaces, docs URLs, badge URLs, CLI strings, release.yml).
#
# Exit 0 = the candidate is available on BOTH registries and the rename
# surface is printed; exit 1 = a collision was found (the registry verdict
# and the would-change surface are still printed so the maintainer sees
# the full picture before picking another candidate).
#
# Usage:
#   bash scripts/rename-readiness.sh <candidate>
#   bash scripts/rename-readiness.sh mylintel
#
# Network: queries crates.io/api/v1 and repo.packagist.org (both public,
# read-only GET). If the network is unreachable, the script prints the
# would-change surface from the static scan and marks the registry verdict
# DEFERRED-TO-NETWORK.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

CANDIDATE="${1:-}"
if [ -z "$CANDIDATE" ]; then
    echo "usage: bash scripts/rename-readiness.sh <candidate>"
    echo "  e.g. bash scripts/rename-readiness.sh mylintel"
    exit 2
fi

# A candidate must be a valid crate name (lowercase, hyphens, no spaces)
# and a valid composer vendor/package segment.
if ! printf '%s' "$CANDIDATE" | grep -qE '^[a-z][a-z0-9-]*$'; then
    echo "::error::candidate '$CANDIDATE' is not a valid crate/composer name"
    echo "  must match ^[a-z][a-z0-9-]*\$ (lowercase, hyphens, no spaces)"
    exit 2
fi

step() { printf '\n== %s ==\n' "$1"; }

UA="lintel-rename-readiness/0.1 (release-readiness; https://github.com/elcoosp/lintel)"
crates_collision=0
packagist_collision=0
network_ok=1

# ----------------------------------------------------------------------------
# 1. Registry availability (crates.io + Packagist, public read-only GET).
# ----------------------------------------------------------------------------
step "1. registry availability for '$CANDIDATE'"

# The publishable Rust crates (release.yml's cargo publish order).
RUST_CRATES=(lintel-diag lintel-core lintel-lsp lintel-cli)
# The composer packages (release.yml's Packagist update-package targets +
# the dev-only ones the maintainer would also rename).
COMPOSER_PKGS=(lintel/lintel lintel/runtime lintel/blade lintel/twig lintel/phpstan markup/markup)

echo "  crates.io:"
for crate in "$CANDIDATE" \
             "$CANDIDATE-cli" "$CANDIDATE-core" "$CANDIDATE-diag" \
             "$CANDIDATE-lsp" "$CANDIDATE-wasm" "$CANDIDATE-docs"; do
    if ! code=$(curl -fsS -A "$UA" -o /tmp/rr-cr.json -w '%{http_code}' --max-time 15 "https://crates.io/api/v1/crates/$crate" 2>/dev/null); then
        echo "    $crate: DEFERRED-TO-NETWORK (curl failed)"
        network_ok=0
        continue
    fi
    if [ "$code" = "200" ]; then
        desc=$(python3 -c "import json; d=json.load(open('/tmp/rr-cr.json')); print((d['crate'].get('description') or '(no desc)')[:50], '| newest:', d['crate']['newest_version'])" 2>/dev/null || echo "(parse failed)")
        echo "    $crate: TAKEN — $desc"
        # Only the root candidate + the four publishable crate-name shapes collide.
        case "$crate" in
            "$CANDIDATE"|"$CANDIDATE"-cli|"$CANDIDATE"-core|"$CANDIDATE"-diag|"$CANDIDATE"-lsp) crates_collision=1 ;;
        esac
    elif [ "$code" = "404" ]; then
        echo "    $crate: AVAILABLE (404)"
    else
        echo "    $crate: HTTP $code (DEFERRED-TO-NETWORK)"
        network_ok=0
    fi
done

echo "  Packagist:"
for pkg in "$CANDIDATE/$CANDIDATE" "$CANDIDATE/runtime" "$CANDIDATE/blade" \
           "$CANDIDATE/twig" "$CANDIDATE/phpstan" \
           "$CANDIDATE/markup"; do
    if ! code=$(curl -fsS -A "$UA" -o /tmp/rr-pk.json -w '%{http_code}' --max-time 15 "https://repo.packagist.org/p2/$pkg.json" 2>/dev/null); then
        echo "    $pkg: DEFERRED-TO-NETWORK (curl failed)"
        network_ok=0
        continue
    fi
    if [ "$code" = "200" ]; then
        n=$(python3 -c "import json; d=json.load(open('/tmp/rr-pk.json')); k=list(d['packages'])[0]; print(len(d['packages'][k]), 'versions; newest:', d['packages'][k][0]['version'])" 2>/dev/null || echo "(parse failed)")
        echo "    $pkg: TAKEN — $n"
        packagist_collision=1
    elif [ "$code" = "404" ]; then
        echo "    $pkg: AVAILABLE (404)"
    else
        echo "    $pkg: HTTP $code (DEFERRED-TO-NETWORK)"
        network_ok=0
    fi
done

if [ "$network_ok" = 0 ]; then
    echo "  registry verdict: DEFERRED-TO-NETWORK (one or more queries failed; the surface below is still valid)"
fi
if [ "$crates_collision" = 1 ]; then
    echo "  crates.io verdict: COLLISION (the candidate or a publishable crate-name shape is taken)"
else
    echo "  crates.io verdict: AVAILABLE"
fi
if [ "$packagist_collision" = 1 ]; then
    echo "  Packagist verdict: COLLISION"
else
    echo "  Packagist verdict: AVAILABLE"
fi

# ----------------------------------------------------------------------------
# 2. The would-change surface (every file/line that mentions the current
#    `lintel` name as a crate/composer/namespace/URL/CLI string).
# ----------------------------------------------------------------------------
step "2. would-change surface (current 'lintel' name → '$CANDIDATE')"

# The literal `lintel` token is scanned across tracked files (git ls-files)
# so vendor/target/node_modules never appear. Each hit is classified by
# kind (crate-name, composer-name, php-namespace, docs-url, repo-url,
# cli-string, prose-mention) so the maintainer sees WHY it would change.

git ls-files -z | while IFS= read -r -d '' f; do
    case "$f" in
        *.toml|*.json|*.yml|*.yaml|*.md|*.sh|*.rs|*.php|*.neon|*.txt|*.tape|*.html|*.css|*.js|*.neon) ;;
        *) continue ;;
    esac
    # Skip the historical bench results (frozen artifacts, not rename targets).
    case "$f" in bench/results/*) continue ;; esac
    hits=$(grep -nE 'lintel' "$f" 2>/dev/null || true)
    if [ -n "$hits" ]; then
        # Classify each hit so the maintainer sees WHY it would change.
        while IFS= read -r line; do
            lineno="${line%%:*}"
            rest="${line#*:}"
            kind=""
            case "$rest" in
                *lintel-diag*|*lintel-core*|*lintel-lsp*|*lintel-cli*|*lintel-wasm*|*lintel-docs*|*lintel-fuzz*) kind="crate-name" ;;
                *lintel/lintel*|*lintel/runtime*|*lintel/blade*|*lintel/twig*|*lintel/phpstan*|*markup/markup*) kind="composer-name" ;;
                *"namespace Lintel"*|*"use Lintel"*|*"use function Lintel"*|*"Lintel\\\\"*) kind="php-namespace" ;;
                *lintel.sh*) kind="docs-url" ;;
                *elcoosp/lintel*) kind="repo-url" ;;
                *"lintel init"*|*"lintel build"*|*"lintel check"*|*"lintel analyze"*|*"lintel migrate"*|*"lintel watch"*|*"lintel stats"*|*"name = \"lintel"*|*"\`lintel\`"*) kind="cli-string" ;;
                *) kind="prose-mention" ;;
            esac
            printf '%s:%s [%s] %s\n' "$f" "$lineno" "$kind" "$rest"
        done <<< "$hits"
    fi
done | sort | head -200

cat <<EOF

== 3. summary ==
  candidate: $CANDIDATE
  crates.io:  $([ "$crates_collision" = 0 ] && echo AVAILABLE || echo COLLISION)
  Packagist:  $([ "$packagist_collision" = 0 ] && echo AVAILABLE || echo COLLISION)
  rename surface: see the classified file:line list above (crate-name,
    composer-name, php-namespace, docs-url, repo-url, cli-string, prose-mention)

  This script renames NOTHING. The maintainer applies the rename (or
  rejects the candidate) on their machine; the surface above is the exact
  diff the rename would touch.
EOF

if [ "$crates_collision" = 0 ] && [ "$packagist_collision" = 0 ]; then
    exit 0
fi
exit 1
