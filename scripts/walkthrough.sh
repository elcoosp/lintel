#!/usr/bin/env bash
# T-20 (architecture doc §15/§16): the timed 10-minute "new developer"
# walkthrough. A stranger on a fresh machine: install → init → edit →
# build → render over HTTP. The timer wraps the whole flow; acceptance is
# "installs and renders a component in ≤ 10 min on each OS".
set -eu

SOURCE="${BASH_SOURCE[0]:-install.sh}"
DEMO_HOME="$(mktemp -d)"
trap 'rm -rf "$DEMO_HOME"' EXIT
export HOME="$DEMO_HOME"
export PATH="$DEMO_HOME/.local/bin:/usr/bin:/bin:$PATH"

START="$(date +%s)"

echo "== step 1: install (curl | sh with the checksum verified) =="
LINTEL_RELEASE_ROOT="${LINTEL_RELEASE_ROOT:-file:///tmp/dist}" \
LINTEL_BIN_DIR="$HOME/.local/bin" \
    bash "$(dirname "$0")/../install.sh"

echo; echo "== step 2: scaffold the demo =="
mkdir -p "$HOME/project" && cd "$HOME/project"
lintel init

echo; echo "== step 3: edit the component =="
sed -i 's|<h1>Hello,|<h1>Rendered by Lintel —|' hello.lintel.php
grep -n "Rendered by Lintel" hello.lintel.php

echo; echo "== step 4: rebuild in place (edit → refresh) =="
lintel build --in-place hello.lintel.php

echo; echo "== step 5: load the runtime and render over HTTP =="
# Published flow: `composer require lintel/runtime` (the autoload chain
# registers the helpers). The pre-release dry run vendors the runtime
# source instead — same classes, same helpers. The runtime is more than
# one file: functions.php require_once's its sibling transports.php, so
# the whole src/ directory is copied, never just functions.php (copying
# the single file leaves transports.php behind and the render 500s).
if [ -n "${LINTEL_RUNTIME_PATH:-}" ]; then
    src_dir="$(dirname "$LINTEL_RUNTIME_PATH")"
    cp "$src_dir"/*.php .
fi
cat > index.php <<'PHP'
<?php
$vendor = __DIR__ . '/vendor/autoload.php';
if (is_file($vendor)) {
    require $vendor;
} else {
    require __DIR__ . '/functions.php';
}
require __DIR__ . '/hello.php';  // the compiled demo echoes its page
PHP
php -S 127.0.0.1:8089 index.php >/dev/null 2>&1 &
SERVER=$!
sleep 1
BODY="$(curl -fsSL http://127.0.0.1:8089/)"
kill $SERVER
echo "rendered: $BODY"
case "$BODY" in
    *"Rendered by Lintel"*) echo "RENDER OK: the edited component came back over HTTP" ;;
    *) echo "RENDER FAIL"; exit 1 ;;
esac

END="$(date +%s)"
ELAPSED=$((END - START))
echo
echo "WALKTHROUGH ELAPSED: ${ELAPSED}s (acceptance: <= 600s)"
[ "$ELAPSED" -le 600 ]
