# Production deployment: Swoole / OpenSwoole

Swoole keeps the whole HTTP server inside PHP coroutines — one resident
process, coroutine-scheduled request handlers, no FPM at all. The Lintel
lane is `SwooleFlushTransport`
(`php/runtime/src/transports.php`): status + headers on `begin()`, one
`write()` per chunk, `end()` closes — chunks reach the already-open
response object as the render produces them, so the Suspense protocol's
out-of-order flushes arrive progressively with no queueing layer.

## Prerequisites

- PHP 8.6 with `ext-swoole` (or `ext-openswoole`) — the transport detects
  the runtime (`SwooleFlushTransport::isAvailable()`); the response
  surface is validated up front by `fromSwoole()`, which throws before
  any byte is written when the object is not a Swoole HTTP response.
- `composer require lintel/runtime` (the helpers + the transport).
- The Lintel compiler once, at deploy time.

## 1. Compile the pages (deploy step)

```sh
vendor/bin/lintel build --target=stream --out public/dist pages/
```

## 2. The server script

`server.php`:

```php
<?php declare(strict_types=1);

require __DIR__.'/vendor/autoload.php';
require __DIR__.'/public/dist/Home.php';

use Lintel\SwooleFlushTransport;
use Swoole\Http\Server;

use function Lintel\stream_via;

$server = new Server('0.0.0.0', 8080, SWOOLE_PROCESS, SWOOLE_SOCK_TCP);

$server->on('request', function ($request, $response): void {
    // fromSwoole() validates the response surface up front (the ext ships
    // no autoloadable interface — the duck type is the contract).
    $transport = SwooleFlushTransport::fromSwoole($response);
    stream_via(Home(name: 'Ada'), $transport);
});

$server->start();
```

`begin()` sets `Content-Type` and `X-Accel-Buffering: no` (nginx in front
of Swoole must not buffer the stream) and tears down any output buffering
levels before the first chunk.

Run it:

```sh
php server.php
```

## 3. Coroutine settings

Swoole ini (`php.ini` or `-d` flags):

```ini
swoole.enable_coroutine=1
swoole.use_shortname=0
; stream hooks let blocking PHP I/O inside a component coroutine-schedule
swoole.enable_library=1
```

Keep the request handler pure render + I/O-through-coroutines: the
transport's `write()` is synchronous on the response object, so chunks
land in flush order regardless of scheduling.

## 4. A minimal page

The same `pages/Home.lintel.php` as the other guides — the compiled
`Home()` returns the generator tree and `stream_via()` walks it:

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

Streaming check on a Suspense page — the fallback arrives first:

```sh
curl -sN http://127.0.0.1:8080/suspense | head -c 200
```

## Performance tuning

- Worker count: `SWOOLE_PROCESS` mode with `worker_num` = CPU cores; the
  render is CPU-bound, the wire writes are handled by the event loop.
- `enable_coroutine` + the runtime hooks let the slow part of a Suspense
  boundary (`usleep`, DB I/O) yield the event loop instead of blocking
  the worker — that is what makes out-of-order flushes progressive.
- Reuse the compiled artifacts at boot (the `require` above runs once);
  per-request work is the render only.
- Put nginx in front only for TLS/limits and keep
  `proxy_buffering off` (the transport already sends
  `X-Accel-Buffering: no`).
- The dev loop stays on the `lintel watch` daemon
  (`docs/daemon-protocol.md`); prod runs the compiled tree — the watcher
  and the `.lintel-cache/` build cache never serve traffic.
