#!/usr/bin/env bash
# The docs/deployment/roadrunner.md guide, executed end-to-end: compile a
# page with the stream target, serve it through a real `rr serve` worker
# pool, curl the endpoint, assert the rendered HTML byte for byte.
#
# Needs on PATH (or env): LINTEL_BIN (the built compiler), PHP_BIN
# (PHP >= 8.1 with the RR worker extensions), rr (RoadRunner binary), and
# php/runtime composer-installed (spiral/roadrunner-http for the worker
# API) — the CI deployment job installs all four; the guide names the
# same steps a human runs.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LINTEL_BIN="${LINTEL_BIN:-$REPO_ROOT/target/release/lintel}"
PHP_BIN="${PHP_BIN:-php}"
RUNTIME_AUTOLOAD="$REPO_ROOT/php/runtime/vendor/autoload.php"

fail() { echo "deployment-smoke: FAIL: $*" >&2; exit 1; }

[ -x "$LINTEL_BIN" ] || fail "the lintel binary is missing at $LINTEL_BIN (build it: cargo build --release -p lintel-cli)"
command -v rr > /dev/null 2>&1 || fail "the rr binary is not on PATH (CI installs roadrunner-2025.1.15)"
[ -f "$RUNTIME_AUTOLOAD" ] || fail "php/runtime is not composer-installed (composer install --working-dir=php/runtime)"
"$PHP_BIN" -v > /dev/null 2>&1 || fail "PHP_BIN=$PHP_BIN does not run"

WORK="$(mktemp -d /tmp/lintel-deployment-rr.XXXXXX)"
trap 'rm -rf "$WORK"; [ -n "${RR_PID:-}" ] && kill "$RR_PID" 2>/dev/null' EXIT
mkdir -p "$WORK/pages" "$WORK/public"

# 1. The guide's minimal page (docs/deployment/roadrunner.md §4).
cat > "$WORK/pages/Home.lintel.php" <<'SOURCE'
<?php

declare(strict_types=1);

function Home(string $name): Html
{
    return <div class="home">
        <h1>Hello {$name}</h1>
    </div>;
}
SOURCE

# 2. Compile with the stream target (guide §1).
"$LINTEL_BIN" build --target=stream --out "$WORK/public/dist" "$WORK/pages" \
  || fail "the compile step failed"
[ -f "$WORK/public/dist/Home.php" ] || fail "the compiled artifact is missing"

# 3. The RR worker (guide §2) — the runtime autoload comes from the repo
# checkout's composer-installed php/runtime package.
cat > "$WORK/worker.php" <<PHP
<?php declare(strict_types=1);

require '$RUNTIME_AUTOLOAD';
require '$WORK/public/dist/Home.php';

use Lintel\RoadRunnerFlushTransport;

use function Lintel\stream_via;

\$rr = new \Spiral\RoadRunner\Http\HttpWorker(\Spiral\RoadRunner\Worker::create());
while (\$rr->waitRequest() !== null) {
    \$transport = new RoadRunnerFlushTransport(\$rr, 200, [
        'Content-Type' => ['text/html; charset=utf-8'],
        'X-Accel-Buffering' => ['no'],
    ]);
    stream_via(Home(name: 'Ada'), \$transport);
    \$transport->respond();
}
PHP

# 4. The RoadRunner config (guide §3).
cat > "$WORK/.rr.yaml" <<YAML
version: "3"
server:
  command: "$PHP_BIN $WORK/worker.php"
  env:
    RR_MODE: http
  pool:
    num_workers: 4
http:
  address: 127.0.0.1:18777
YAML

# 5. Serve and poll until the worker pool answers.
rr serve -c "$WORK/.rr.yaml" -o http.address=127.0.0.1:18777 \
  > "$WORK/rr.log" 2> "$WORK/rr.err" &
RR_PID=$!
EXPECTED='<div class="home"><h1>Hello Ada</h1></div>'
BODY=""
for _ in $(seq 1 40); do
  BODY="$(curl -s --max-time 5 http://127.0.0.1:18777/ 2>/dev/null)"
  if [ "$BODY" = "$EXPECTED" ]; then
    kill "$RR_PID" 2>/dev/null
    wait "$RR_PID" 2>/dev/null
    echo "deployment-smoke: PASS (rr served the compiled stream artifact: $BODY)"
    exit 0
  fi
  sleep 0.5
done

fail "rr never served the expected body (got: '$BODY'); rr.err: $(tail -5 "$WORK/rr.err" 2>/dev/null)"
