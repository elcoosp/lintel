#!/usr/bin/env bash
# The T-18 release dry-run (v0.1.0): validates .github/workflows/release.yml
# and the release prep BEFORE the maintainer tags v0.1.0, entirely
# offline-except-packaging:
#
#   1. actionlint parses every workflow (the workflow-lint gate, re-run here
#      so the release edits never rely on "CI will catch it").
#   2. The four publishable crates resolve to ONE workspace version.
#   3. The `cargo publish -p …` order in release.yml is topologically sound:
#      every crate's internal lintl-* dependency publishes earlier.
#   4. The CHANGELOG carries a valid `## [0.1.0]` release section with
#      content — the exact text the release job extracts for the release body
#      (the same awk is run here, so what you preview is what ships).
#   5. The composer publish targets exist: lintl/lintl + markup/markup,
#      named exactly as the workflow publishes them.
#   6. `cargo publish --dry-run` packages each crate standalone (the real
#      crates.io acceptance: the packaged Cargo.toml + the verification
#      build; `--allow-dirty` is a dry-run-only affordance — the real
#      publish runs on CI's clean checkout).
#   7. The workflow structurally carries what §12 promises: the five-binary
#      matrix, SHA256SUMS, build-provenance attestation and a DRAFT release.
#
# Usage: bash scripts/release-dry-run.sh [--no-package]
#   --no-package skips step 6 (when you only want the structural checks).
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

WORKFLOW=".github/workflows/release.yml"
CHANGELOG="CHANGELOG.md"
fail=0
step() { printf '\n== %s ==\n' "$1"; }
bail() { echo "DRY-RUN FAIL: $1"; fail=1; }

step "1. actionlint (every workflow)"
if ! command -v actionlint >/dev/null 2>&1; then
    bail "actionlint not on PATH (make workflows documents the user-space install)"
else
    actionlint || bail "actionlint found problems"
fi

step "2. one workspace version across the publishable crates"
if ! command -v cargo >/dev/null 2>&1; then
    bail "cargo not on PATH"
else
    META=$(cargo metadata --no-deps --format-version 1 2>/dev/null) || bail "cargo metadata failed"
    VERSIONS=$(python3 - "$META" <<'PY'
import json, sys
meta = json.loads(sys.argv[1])
names = {"lintl-core", "lintl-diag", "lintl-cli", "lintl-lsp"}
versions = {p["name"]: p["version"] for p in meta["packages"] if p["name"] in names}
print(json.dumps(versions))
PY
) || bail "could not parse cargo metadata"
    echo "crate versions: $VERSIONS"
    UNIQ=$(python3 -c "import json,sys; vs=set(json.loads(sys.argv[1]).values()); print(len(vs))" "$VERSIONS")
    if [ "$UNIQ" != "1" ]; then
        bail "the publishable crates do not share one version — bump [workspace.package].version, not per-crate"
    fi
    REL_VERSION=$(python3 -c "import json,sys; print(list(set(json.loads(sys.argv[1]).values()))[0])" "$VERSIONS")
    echo "release version: v$REL_VERSION"
    if [ -n "${RELEASE_DRY_RUN_TAG:-}" ]; then
        if [ "refs/tags/v$REL_VERSION" != "$RELEASE_DRY_RUN_TAG" ] && [ "v$REL_VERSION" != "${RELEASE_DRY_RUN_TAG#refs/tags/}" ]; then
            bail "tag $RELEASE_DRY_RUN_TAG does not match the workspace version v$REL_VERSION (the release job's guard will refuse it)"
        else
            echo "tag guard: $RELEASE_DRY_RUN_TAG matches v$REL_VERSION"
        fi
    fi
fi

step "3. publish order is topologically sound"
ORDER=$(grep -oE 'cargo publish .*-p (lintl-[a-z]+)' "$WORKFLOW" | sed -E 's/.*-p (lintl-[a-z]+)$/\1/') || ORDER=""
if [ -z "$ORDER" ]; then
    bail "release.yml carries no cargo publish steps — the crates.io leg is missing"
else
    echo "workflow publish order: $(echo "$ORDER" | tr '\n' ' ')"
    python3 - "$META" "$ORDER" <<'PY' || bail "publish order violates the dependency graph"
import json, sys
meta = json.loads(sys.argv[1])
order = sys.argv[2].split()
names = {"lintl-core", "lintl-diag", "lintl-cli", "lintl-lsp"}
deps = {}
for p in meta["packages"]:
    deps[p["name"]] = [d["name"] for d in p["dependencies"] if d["name"] in names]
pos = {name: i for i, name in enumerate(order)}
ok = True
for name in order:
    for dep in deps.get(name, []):
        if dep not in pos:
            print(f"  MISSING: {dep} (a dependency of {name}) is never published before it")
            ok = False
        elif pos[dep] >= pos[name]:
            print(f"  ORDER: {dep} must publish before {name}")
            ok = False
missing = names - set(order)
for name in sorted(missing):
    print(f"  MISSING: {name} is never published")
    ok = False
sys.exit(0 if ok else 1)
PY
    echo "publish order: OK (every dependency precedes its dependent)"
fi

step "4. CHANGELOG: the v0.1.0 release section"
SECTION=$(awk '/^## \[0\.1\.0\]/{flag=1; next} /^## /{flag=0} flag' "$CHANGELOG") || SECTION=""
if [ -z "$SECTION" ]; then
    bail "CHANGELOG.md has no '## [0.1.0]' section — the release body would be empty"
else
    BULLETS=$(printf '%s\n' "$SECTION" | grep -c '^- ') || BULLETS=0
    if [ "$BULLETS" -lt 1 ]; then
        bail "the '## [0.1.0]' section carries no bullet entries"
    fi
    CHARS=$(printf '%s' "$SECTION" | wc -m)
    echo "release body: '## [0.1.0]' — $BULLETS bullets, $CHARS chars (the release job attaches exactly this text)"
fi

step "5. composer publish targets exist"
for pkg in lintl/lintl markup/markup; do
    case "$pkg" in
        lintl/lintl) DIR="php/lintl" ;;
        markup/markup) DIR="php/markup" ;;
    esac
    NAME=$(python3 -c "import json; print(json.load(open('$DIR/composer.json'))['name'])" 2>/dev/null) || NAME=""
    if [ "$NAME" = "$pkg" ]; then
        echo "composer package: $pkg ($DIR/composer.json)"
    else
        bail "$DIR/composer.json declares '$NAME', the workflow publishes '$pkg'"
    fi
done
for pkg in lintl/lintl markup/markup; do
    if ! grep -q "$pkg" "$WORKFLOW"; then
        bail "release.yml never mentions $pkg — the composer publish leg is missing"
    fi
done

step "6. cargo publish --dry-run (standalone packaging, verification build included)"
if [ "${1:-}" = "--no-package" ]; then
    echo "skipped (--no-package)"
else
    # Pre-tag reality: a dependent crate resolves its sibling's registry
    # version, which only exists on crates.io AFTER the release job
    # publishes it (in dependency order). So exactly one failure class is
    # expected pre-tag — "no matching package named `lintl-X` found" /
    # "candidate versions found which didn't match" for an INTERNAL
    # dependency — and it is reported as deferred-to-the-release-job, never
    # silent. Every other packaging failure (a missing version requirement
    # on a path dep, a broken manifest, a failed verification build) is a
    # hard stop. Post-tag, with the crates live, all four verify fully.
    deferred=0
    for crate in lintl-diag lintl-core lintl-lsp lintl-cli; do
        OUT=$(cargo publish --dry-run --allow-dirty --locked -p "$crate" 2>&1)
        if [ $? -eq 0 ]; then
            echo "packaged standalone: $crate"
        elif printf '%s' "$OUT" | grep -qE "no matching package named .lintl-|candidate versions found which didn't match"; then
            echo "deferred (expected pre-tag): $crate — its internal dependency's new version is not on crates.io yet;"
            echo "  the release job's publish step dry-runs each crate immediately after its dependency publishes (diag → core → lsp → cli)"
            deferred=$((deferred + 1))
        else
            printf '%s\n' "$OUT"
            bail "$crate does not package standalone"
        fi
    done
    echo "packaging: $((4 - deferred))/4 fully verified, $deferred deferred to the release job (pre-tag chicken-and-egg, stated above)"
fi

step "7. workflow carries the §12 promises (matrix, checksums, provenance, draft)"
for needle in \
    "x86_64-unknown-linux-musl" "aarch64-unknown-linux-musl" \
    "x86_64-apple-darwin" "aarch64-apple-darwin" "x86_64-pc-windows-msvc" \
    "SHA256SUMS" "attest-build-provenance"; do
    if grep -q "$needle" "$WORKFLOW"; then
        echo "present: $needle"
    else
        bail "release.yml is missing: $needle"
    fi
done
if grep -qE '^\s+draft:\s*true' "$WORKFLOW"; then
    echo "present: draft: true (the release is created as a draft for the maintainer to review)"
else
    bail "release.yml does not create a DRAFT release (draft: true missing)"
fi

printf '\n'
if [ "$fail" = 0 ]; then
    echo "release dry-run: ALL OK — the v0.1.0 automation is ready for the tag"
    exit 0
fi
echo "release dry-run: FAILED — fix the steps above before tagging"
exit 1
