#!/usr/bin/env bash
# The v1.0 readiness claims sweep (the Phase 12 M4 card): the user-facing
# docs — README, the docs/ contract and deployment pages, the docs-site
# pages and the CLI --help text (its source literal) — must not carry
# stale pre-1.0 / v0.x / "not built yet" maturity claims now that the
# feature set shipped. A hit outside the explicit allowlist fails the
# sweep; the allowlist is where a genuinely-open note stays, each entry
# named with its reason. The roadmap (docs/architecture.md §17.1) and the
# status reconciliation (§21) are excluded by name — future-facing
# language there is the tracked backlog itself, and the CHANGELOG's
# released sections are historical record.
#
# Not in this sweep: the honest open-backlog phrasing ("future work",
# "stays backlog") in the contract pages — those notes describe seams
# that are genuinely not built (stream-boundary scaffolding, attribute
# traversal, the remaining LNT pages) and are tracked, not stale.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fail=0

PATTERN='pre-1\.0|pre 1\.0|v0\.x|not built yet|not started|stays backlog|experimental'

# The explicit allowlist: exact lines that stay, each with its reason.
ALLOW="$REPO_ROOT/scripts/claims-allowlist.txt"
allow_filter() {
    if [ -s "$ALLOW" ]; then
        grep -vxF -f "$ALLOW"
    else
        cat
    fi
}

echo "== v1.0 readiness claims sweep =="
while IFS= read -r -d '' f; do
    hits=$(grep -nE "$PATTERN" "$f" 2>/dev/null | allow_filter || true)
    if [ -n "$hits" ]; then
        echo "STALE CLAIM: $f"
        echo "$hits"
        fail=1
    fi
done < <(cd "$REPO_ROOT" && {
    printf '%s\0' README.md SECURITY.md install.sh
    printf '%s\0' crates/lintel-cli/src/lib.rs
    find docs -name '*.md' ! -name 'architecture.md' -print0
    find docs-site/pages -name '*.md' -print0
})

if [ "$fail" = 0 ]; then
    echo "claims sweep: OK (no stale maturity claims outside the allowlist)"
else
    echo "claims sweep: FAILED — update the claim or add an allowlist line with a reason"
fi
exit $fail
