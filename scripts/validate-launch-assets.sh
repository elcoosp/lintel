#!/usr/bin/env bash
# The M3 launch-asset validator (T-19 / architecture doc §16): the launch
# assets must be reproducible and truthful BEFORE the maintainer publishes.
# Every check names its source of truth; a missing or stale asset fails
# with an actionable error — never a silent pass.
#
#   1. docs/demo.gif — the §16 launch GIF exists, is a real GIF, > 0 and
#      < 5 MB (the 20-second edit → refresh → compiled view, rendered from
#      docs/demo.tape by the CI launch-assets job or `vhs docs/demo.tape`).
#   2. docs/og.png — the social card is EXACTLY 1280x640 (identify, then
#      Python/PIL, then `file` — one of the three must answer).
#   3. docs/launch/posts.md — exactly 3 posts (## Post #1..#3), each
#      measured with `wc -m` and ≤ 280 characters (§16: a commit is
#      3-280 characters).
#   4. The numbers in the posts are REPRODUCIBLE: no `{N}` placeholder
#      survives in a post; the post #2 payload count equals the §7.2
#      threat-corpus case count (counted from ThreatCorpusTest.php, and
#      matched against the docs-site security page); the S1 bench number
#      in the launch copy equals bench/results' median_ns (§9.2 publishing
#      rule: no number appears in launch posts unless the results JSON
#      reproduces it).
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"
GIF="docs/demo.gif"
OG="docs/og.png"
POSTS="docs/launch/posts.md"
CORPUS="php/runtime/tests/ThreatCorpusTest.php"
fail=0
step() { printf '\n== %s ==\n' "$1"; }
bail() { echo "LAUNCH-ASSET FAIL: $1"; fail=1; }

step "1. docs/demo.gif — exists, a real GIF, < 5 MB"
if [ ! -f "$GIF" ]; then
    bail "$GIF is missing — render it: vhs docs/demo.tape (or let the CI launch-assets job produce it), then commit it"
elif ! head -c 6 "$GIF" | grep -q "^GIF8"; then
    bail "$GIF exists but is not a GIF (the magic bytes are not GIF8)"
else
    SIZE=$(wc -c < "$GIF")
    LIMIT=$((5 * 1024 * 1024))
    if [ "$SIZE" -le 0 ]; then
        bail "$GIF is empty"
    elif [ "$SIZE" -ge "$LIMIT" ]; then
        bail "$GIF is $SIZE bytes — the §16 budget is < 5 MB ($LIMIT); re-render smaller (fewer Sleeps / lower Width) or re-encode"
    else
        echo "present: $GIF — $SIZE bytes (< 5 MB)"
    fi
fi

step "2. docs/og.png — exactly 1280x640"
if [ ! -f "$OG" ]; then
    bail "$OG is missing — the §16 social card"
else
    DIMS=""
    if command -v identify >/dev/null 2>&1; then
        DIMS=$(identify -format '%wx%h' "$OG" 2>/dev/null) || DIMS=""
    fi
    if [ -z "$DIMS" ] && command -v python3 >/dev/null 2>&1; then
        DIMS=$(python3 - "$OG" <<'PY'
import sys
try:
    from PIL import Image
except ImportError:
    sys.exit(0)
im = Image.open(sys.argv[1])
print(f"{im.size[0]}x{im.size[1]}")
PY
) || DIMS=""
    fi
    if [ -z "$DIMS" ] && command -v file >/dev/null 2>&1; then
        DIMS=$(file "$OG" | grep -oE '[0-9]+ ?x ?[0-9]+' | head -1 | tr -d ' ') || DIMS=""
    fi
    if [ "$DIMS" = "1280x640" ]; then
        echo "present: $OG — exactly 1280x640"
    else
        bail "$OG measures '${DIMS:-unknown}' — the §16 social card must be exactly 1280x640; fix the render and re-export"
    fi
fi

step "3. docs/launch/posts.md — exactly 3 posts, each ≤ 280 characters (wc -m)"
if [ ! -f "$POSTS" ]; then
    bail "$POSTS is missing — the three §16 variants"
else
    for n in 1 2 3; do
        BLOCK=$(awk -v n="$n" '
            $0 ~ ("^## Post #" n " ") {inpost=1; next}
            /^## / {inpost=0}
            inpost && infence && /^```$/ {exit}
            inpost && !infence && /^```/ {infence=1; next}
            infence {print}
        ' "$POSTS")
        if [ -z "$BLOCK" ]; then
            bail "$POSTS has no '## Post #$n' section with a fenced post block"
            continue
        fi
        CHARS=$(printf '%s\n' "$BLOCK" | wc -m)
        CHARS=$((CHARS - 1)) # wc -m counts the appended newline
        if [ "$CHARS" -gt 280 ]; then
            bail "post #$n is $CHARS characters — over the 280-character §16 budget"
        elif [ "$CHARS" -lt 3 ]; then
            bail "post #$n is $CHARS characters — a commit is 3-280 characters"
        else
            echo "post #$n: $CHARS characters (≤ 280)"
        fi
    done
fi

step "4. the numbers are reproducible (no {N}, the corpus count, the bench number)"
# 4a. no placeholder survives inside a post block
PLACEHOLDERS=$(awk '
    /^## Post #[123] / {inpost=1; next}
    /^## / {inpost=0}
    inpost && infence && /^```$/ {infence=0; next}
    inpost && !infence && /^```/ {infence=1; next}
    infence {print}
' "$POSTS" | grep -c '{N}') || PLACEHOLDERS=0
if [ "$PLACEHOLDERS" -gt 0 ]; then
    bail "a post still carries a literal {N} placeholder — fill it from a reproducible source (bench/results/ or the threat corpus)"
else
    echo "no {N} placeholder survives in any post"
fi

# 4b. the threat-corpus case count, counted from the test file itself:
#     the provider-backed methods contribute their yield entries, every
#     other `test*` method is one case.
if [ ! -f "$CORPUS" ]; then
    bail "$CORPUS is missing — the §7.2 corpus is the post #2 source of truth"
else
    TEST_METHODS=$(grep -c 'public function test' "$CORPUS")
    PROVIDER_BACKED=$(grep -c "DataProvider(" "$CORPUS")
    PROVIDER_CASES=$(sed -n '/public static function provide/,/^    }/p' "$CORPUS" | grep -c 'yield ')
    CORPUS_COUNT=$((TEST_METHODS - PROVIDER_BACKED + PROVIDER_CASES))
    echo "threat corpus: $TEST_METHODS test methods, $PROVIDER_BACKED provider-backed (+$PROVIDER_CASES cases) = $CORPUS_COUNT cases"
    CITED=$(awk '
        /^## Post #2 / {inpost=1; next}
        /^## / {inpost=0}
        inpost && infence && /^```$/ {infence=0; next}
        inpost && !infence && /^```/ {infence=1; next}
        infence {print}
    ' "$POSTS" | grep -oE '[0-9]+ hostile payloads' | grep -oE '^[0-9]+') || CITED=""
    if [ -z "$CITED" ]; then
        bail "post #2 does not cite its payload count as '<N> hostile payloads' — the validator cannot tie it to the corpus"
    elif [ "$CITED" != "$CORPUS_COUNT" ]; then
        bail "post #2 cites $CITED hostile payloads but the §7.2 corpus has $CORPUS_COUNT cases — fix the post or the corpus"
    else
        echo "post #2's payload count matches the corpus: $CITED"
    fi
    SITE_PAGE="docs-site/pages/security.md"
    if [ -f "$SITE_PAGE" ]; then
        SITE_COUNT=$(grep -oE '[0-9]+ tests' "$SITE_PAGE" | head -1 | grep -oE '^[0-9]+') || SITE_COUNT=""
        if [ "$SITE_COUNT" = "$CORPUS_COUNT" ]; then
            echo "the docs-site security page states the same count: $SITE_COUNT"
        else
            bail "$SITE_PAGE states '${SITE_COUNT:-nothing} tests' but the corpus has $CORPUS_COUNT cases — the launch assets disagree"
        fi
    fi
fi

# 4c. the bench number: the S1 median the launch copy cites must equal the
#     results JSON (the §9.2 publishing rule).
RESULTS_JSON=$(ls bench/results/results-native-*.json 2>/dev/null | head -1)
if [ -z "$RESULTS_JSON" ]; then
    bail "bench/results/ carries no results-native-*.json — the launch copy's bench number has no source of truth"
else
    MEDIAN=$(python3 - "$RESULTS_JSON" <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))
print(data["settings"]["opcache-off"]["results"]["S1"]["lintel"]["median_ns"])
PY
) || MEDIAN=""
    if [ -z "$MEDIAN" ]; then
        bail "could not read S1 lintel median_ns (opcache-off) from $RESULTS_JSON"
    else
        CITED_MEDIAN=$(grep -oE 'S1 median: [0-9]+ ns' "$POSTS" | grep -oE '[0-9]+ ns' | grep -oE '^[0-9]+') || CITED_MEDIAN=""
        if [ "$CITED_MEDIAN" = "$MEDIAN" ]; then
            echo "launch copy cites the S1 median from $RESULTS_JSON: $CITED_MEDIAN ns"
        else
            bail "the launch copy cites '${CITED_MEDIAN:-nothing}' for the S1 median but $RESULTS_JSON says $MEDIAN ns — re-run make bench and re-derive the copy (§9.2 publishing rule)"
        fi
    fi
fi

printf '\n'
if [ "$fail" = 0 ]; then
    echo "launch assets: ALL OK — reproducible, sized, and measured"
    exit 0
fi
echo "launch assets: FAILED"
exit 1
