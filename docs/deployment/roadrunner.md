# Production deployment: RoadRunner

RoadRunner runs the PHP application as a resident worker pool behind a
Go HTTP server — no FPM, no per-request process spawn. The Lintl lane
uses the `RoadRunnerFlushTransport`
(`php/runtime/src/transports.php`): RR's PHP worker API is pull-based, so
`stream_via()` runs the render first with the transport queueing every
chunk in flush order, and `respond()` then hands the queued chunks to the
worker as one midflight frame per yielded chunk — the flush order, the
`<template id="lt-N-r">` + swap-script protocol and the escaper contract
are byte-identical to the FPM lane. The progressive per-boundary *timing*
is measured and asserted by the Phase 11 lane
(`bench/lib/ProgressiveTiming.php`, wired into `bench/run.php`'s S6
section and CI's `bench` job): the placeholder for boundary N is flushed
before the resolved content for boundary N-1, and the wire order equals
the flush order — the invariant the lane fails the bench run on.

## Prerequisites

- RoadRunner ≥ 2025.1 (the CI verification lane pins
  `roadrunner-2025.1.15-linux-amd64`).
- PHP 8.6 with `ext-sockets`/`ext-pcntl` (the RR worker lifecycle).
- `composer require lintl/runtime spiral/roadrunner-http` — the
  `spiral/roadrunner-http` package the transport's `HttpWorkerInterface`
  comes from.
- The Lintl compiler once, at deploy time (`vendor/bin/lintl` via the
  installer plugin, or a release binary).

## 1. Compile the pages (deploy step)

```sh
vendor/bin/lintl build --target=stream --out public/dist pages/
```

## 2. The worker script

`worker.php`:

```php
<?php declare(strict_types=1);

require __DIR__.'/vendor/autoload.php';
require __DIR__.'/public/dist/Home.php';

use Lintl\RoadRunnerFlushTransport;

use function Lintl\stream_via;

// One long-lived worker loop: each request renders and responds in place.
$rr = new \Spiral\RoadRunner\Http\HttpWorker(\Spiral\RoadRunner\Worker::create());
while ($rr->waitRequest() !== null) {
    $transport = new RoadRunnerFlushTransport($rr, 200, [
        'Content-Type' => ['text/html; charset=utf-8'],
        'X-Accel-Buffering' => ['no'],
    ]);
    stream_via(Home(name: 'Ada'), $transport);
    $transport->respond();
}
```

The transport queues chunks during `stream_via()` and
`respond()` hands them to the worker — one stream chunk per queued chunk
(`respond(status, generator, headers)`, the pull-based worker API).

## 3. RoadRunner config

`.rr.yaml`:

```yaml
version: "3"

server:
  command: "php worker.php"
  env:
    RR_MODE: http

http:
  address: "0.0.0.0:8080"
  pool:
    num_workers: 8
    max_jobs: 0            # 0 = workers are never recycled
    allocate_timeout: 10s
    destroy_timeout: 5s
  # RR streams midflight frames as the generator yields them; no
  # response buffering to disable — this is the streaming-native lane.
  middleware: []
```

Run it:

```sh
rr serve -c .rr.yaml
```

## 4. A minimal page

The same `pages/Home.lintl.php` as the FPM guide compiles unchanged —
`Home()` returns the generator tree; only the transport differs between
lanes. That is the §10 sync-compat contract: the same source compiles to
string target (fully buffered) or stream target (progressive), and the
adapters own only HOW a chunk reaches the wire.

## 5. Verify

```sh
curl -s http://127.0.0.1:8080/
```

Expected:

```html
<div class="home"><h1>Hello Ada</h1></div>
```

Chunk-level check — RR writes one midflight frame per yielded chunk, so
`curl -sN` shows the fallback before a slow Suspense boundary resolves:

```sh
curl -sN http://127.0.0.1:8080/suspense
```

## Performance tuning

- `num_workers`: CPUs, not connections — each worker is a resident PHP
  process; 8 workers serve hundreds of concurrent streams (RR multiplexes
  over the pool).
- `max_jobs: 0` keeps workers warm across deploys; for leak protection
  set a high finite value (e.g. 1000) and let the pool recycle.
- The worker boots once: move the `require` of the compiled artifacts to
  the top of `worker.php` — with `--target=stream` artifacts and OPcache
  (`opcache.enable_cli=1` is NOT needed; RR workers use the regular
  SAPI) the per-request cost is the render alone.
- Health check: `rr http:reset` reloads the pool without dropping the
  listener; wire it to your deploy hook.
- Measure the win: T-12's daemon-vs-spawn numbers live in the same
  spirit — a resident worker pool removes the per-request process spawn
  FPM pays; the `lintl watch` dev daemon (`docs/daemon-protocol.md`)
  mirrors that design for the dev loop.
