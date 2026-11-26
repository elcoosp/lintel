# Production deployment: PHP-FPM + nginx

The classic shared-hosting lane: nginx terminates HTTP, PHP-FPM executes
the pre-compiled Lintl artifacts, and streaming flows through the
`FpmFlushTransport` (`php/runtime/src/transports.php` — `X-Accel-Buffering:
no` plus a `flush()` per chunk). Nothing in this lane needs the Rust
compiler at runtime: prod ships **compiled `.php` only** (the §0.2
decision), built once at deploy time by `lintl build`.

## Prerequisites

- PHP 8.6+ with FPM (the runtime package needs `>=8.6`),
  `ext-mbstring` recommended for the dev tooling.
- nginx 1.20+.
- The Lintl compiler binary once, at deploy time (`cargo install
  lintl-cli`, a GitHub release download, or the `vendor/bin/lintl` path
  the `lintl/lintl` installer plugin caches locally — §12).
- `composer require lintl/runtime` (the escaping helpers the compiled
  markup calls).

## 1. Compile the pages (deploy step)

```sh
# stream target for streaming pages, string target for fully-buffered ones
vendor/bin/lintl build --target=stream --out public/dist pages/
vendor/bin/lintl build --out public/dist pages-static/
```

`pages/Home.lintl.php` becomes `public/dist/Home.php` (`.lintl.php` →
`.php`); the runtime helpers ride `vendor/autoload.php`. The compile is a
pure function of source + target, so the step is cacheable in CI
(`.lintl-cache/` keys artifacts by content + target).

## 2. nginx config

`/etc/nginx/conf.d/lintl.conf`:

```nginx
server {
    listen 80;
    server_name lintl.example;
    root /var/www/app/public;

    location / {
        try_files $uri /index.php$is_args$args;
    }

    location ~ \.php$ {
        include fastcgi_params;
        fastcgi_pass unix:/run/php-fpm/lintl.sock;
        fastcgi_param SCRIPT_FILENAME $realpath_root$fastcgi_script_name;
        fastcgi_read_timeout 60s;
        # Suspense boundaries resolve out-of-order: nginx must not buffer
        # the stream (the transport also sends X-Accel-Buffering: no).
        fastcgi_buffering off;
        proxy_buffering off;
    }
}
```

`fastcgi_buffering off` (server-wide) and the header the transport sends
are belt-and-braces — either alone unblocks the stream.

## 3. FPM pool + OPcache

`/etc/php-fpm.d/lintl.conf` (pool essentials):

```ini
[lintl]
user = www-data
listen = /run/php-fpm/lintl.sock
pm = dynamic
pm.max_children = 16
pm.start_servers = 4
pm.min_spare_servers = 2
pm.max_spare_servers = 8
php_admin_value[opcache.file_cache] = /var/cache/opcache
php_admin_flag[log_errors] = on
```

OPcache is the single biggest win for this lane (the artifacts are plain
PHP files). Production tuning in `php.ini`:

```ini
opcache.enable=1
opcache.validate_timestamps=0        ; artifacts are immutable per deploy
opcache.revalidate_freq=0
opcache.memory_consumption=192
opcache.max_accelerated_files=20000
opcache.jit_buffer_size=64M          ; PHP 8.6 JIT: measure on your pages
```

**Preload** the hot artifacts and the runtime helpers once at FPM start
(`opcache.preload=public/preload.php`, `opcache.preload_user=www-data`):

```php
<?php
// public/preload.php — compiled once, required by every request.
require __DIR__.'/../vendor/autoload.php';
foreach (glob(__DIR__.'/dist/*.php') ?: [] as $artifact) {
    require $artifact;
}
```

Preloading removes the per-request include cost entirely; the dev
loader's per-request compile cache (`.lintl-cache/`) is a dev-lane
mechanism and has no place in this pool — prod requires the shipped
`dist/*.php` artifacts directly.

## 4. A minimal page

`pages/Home.lintl.php`:

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

`public/index.php` (the entry the nginx `try_files` lands on):

```php
<?php declare(strict_types=1);

require __DIR__.'/../vendor/autoload.php';
require __DIR__.'/dist/Home.php';

use function Lintl\stream_via;

stream_via(Home(name: 'Ada'), new \Lintl\FpmFlushTransport());
```

The artifact's `Home()` returns the generator tree the stream lowering
emits; `stream_via()` walks it and the transport pushes every chunk to
the wire as it is produced (suspense boundaries included, §10's
out-of-order protocol).

## 5. Verify

```sh
curl -s http://lintl.example/ -H 'Accept: text/html'
```

Expected:

```html
<div class="home"><h1>Hello Ada</h1></div>
```

Streaming check — the chunks must arrive progressively (two reads with a
boundary between them on a Suspense page):

```sh
curl -sN http://lintl.example/suspense | head -c 200
```

`-N` disables curl's own buffering; with `fastcgi_buffering off` the
fallback HTML arrives before the slow boundary resolves.

## Performance tuning

- `pm.max_children`: one child per ~20 MB of expected RSS within the
  box's RAM; the streaming lanes hold a worker for the stream's full
  duration, so size for slowest page, not average.
- Keep `opcache.validate_timestamps=0` and rebuild OPcache through
  preload on each deploy (an FPM reload picks the new artifacts up).
- `output_buffering=0` for the pool — any level the transport does not
  explicitly tear down delays the first byte.
- Compress at the edge (`gzip`/`brotli` in nginx); the chunk boundaries
  do not survive compression in-flight, so let nginx buffer per-response
  for compression only on non-streaming routes if you need it.
- The `lintl watch` daemon and the `.lintl-cache/` build cache are
  dev-lane (`docs/daemon-protocol.md`); a prod image ships the compiled
  tree and never runs the watcher.
