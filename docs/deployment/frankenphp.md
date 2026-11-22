# Production deployment: FrankenPHP

FrankenPHP embeds PHP inside Caddy — a resident worker mode with
goroutine-scheduled PHP execution and native HTTP streaming. The Lintel
lane is `FrankenPHPFlushTransport`
(`php/runtime/src/transports.php`): the same `echo + flush` streaming
shape the FPM lane uses, plus `X-Accel-Buffering: no` and a
buffering-level teardown on `begin()`, so every chunk reaches the client
as the render produces it.

## Prerequisites

- FrankenPHP (the official Docker image `dunglas/frankenphp` is the
  standard install; the Caddy builds with the `frankenphp` module are the
  non-Docker path). PHP ≥ 8.6 inside the image.
- `composer require lintel/runtime` (the helpers + the transport). The
  transport detects the runtime (`FrankenPHPFlushTransport::isAvailable()`
  checks `FRANKENPHP_WORKER` and `SERVER_SOFTWARE`).
- The Lintel compiler once, at deploy time.

## 1. Compile the pages (deploy step)

```sh
vendor/bin/lintel build --target=stream --out public/dist pages/
```

## 2. The Caddyfile

`Caddyfile`:

```caddy
{
    frankenphp {
        # worker mode: the script boots once per thread and serves
        # subsequent requests in a loop
        worker ./public/index.php 8
    }
}

:8080 {
    root * /app/public
    encode zstd br gzip
    php_worker frankenphp
    file_server
}
```

Run it:

```sh
docker run --rm -p 8080:8080 -v "$PWD:/app" dunglas/frankenphp run --config /app/Caddyfile
```

## 3. The worker script

`public/index.php` (worker mode: FrankenPHP includes this file per
request, and superglobals carry the request; the flush transport
differences are invisible to the page code):

```php
<?php declare(strict_types=1);

require __DIR__.'/../vendor/autoload.php';
require __DIR__.'/dist/Home.php';

use Lintel\FrankenPHPFlushTransport;

use function Lintel\stream_via;

stream_via(Home(name: 'Ada'), new FrankenPHPFlushTransport());
```

`begin()` sends `Content-Type` + `X-Accel-Buffering: no` and unwinds
every `ob_*` level; `write()` is `echo` + `flush()` — FrankenPHP pushes
each flush to the HTTP/2/3 connection immediately.

## 4. A minimal page

The same `pages/Home.lintel.php` as every other guide — the compiled
`Home()` returns the generator tree, `stream_via()` walks it:

```php
<?php

declare(strict_types=1);

function Home(string $name): Html
{
    return <div class="home">
        <h1>Hello {$name}</h1>
    </div>;
}
```

## 5. Verify

```sh
curl -s http://127.0.0.1:8080/
```

Expected:

```html
<div class="home"><h1>Hello Ada</h1></div>
```

Streaming check on a Suspense page — the fallback arrives before the
slow boundary resolves:

```sh
curl -sN http://127.0.0.1:8080/suspense | head -c 200
```

## Performance tuning

- `worker ./public/index.php N`: N = number of threads (typically CPU
  cores); each thread keeps the PHP state warm — the artifact `require`
  runs once per thread, per-request cost is the render.
- `encode zstd br gzip` compresses at the edge; for the streaming routes
  Caddy's incremental flushing keeps chunk boundaries (verify with the
  `-sN` curl above).
- Worker restarts: FrankenPHP restarts a worker on `max_requests`
  (configure in the `frankenphp` block) — mirror your FPM `max_requests`
  hygiene here.
- TLS/HTTP-3 comes from Caddy itself — no reverse proxy needed for the
  common case; keep `flush_interval -1` for streaming routes if a later
  Caddy version defaults to buffering.
- The dev loop remains the `lintel watch` daemon
  (`docs/daemon-protocol.md`); prod ships the compiled `dist/` tree —
  the watcher and the `.lintel-cache/` build cache never serve traffic.
