<?php

declare(strict_types=1);
// The §10 suspense demo (architecture doc §10): a stream-target page with a
// slow <Suspense> boundary, rendered through the flush runtime.
//
//   php scripts/suspense-demo.php          # capture mode: prints the bytes
//   php -S 127.0.0.1:8199 scripts/suspense-demo.php   # progressive over HTTP
//
// The chunk ORDER is the protocol: the shell and the fallback placeholder
// (`<div id="lt-1">…`) print first, the footer prints while the boundary is
// still pending, and only then the resolved content arrives inside
// `<template id="lt-1-r">…</template><script>lintlSwap("lt-1")</script>` —
// the swap script replaces the placeholder in place. The attacker payload
// in the resolved data prints escaped: the streaming path never bypasses
// the escapers.

require __DIR__.'/../php/runtime/src/functions.php';

use Lintl\Suspense;

use function Lintl\c;
use function Lintl\stream;

$payload = '<script>alert("pwned")</script>';

$page = (static function () use ($payload): Generator {
    yield '<!doctype html><html><body><h1>Suspense demo</h1>';
    yield new Suspense(
        'Loading the report…',
        static function () use ($payload): Generator {
            \usleep(20_000); // the "slow" async work
            yield '<ul><li>user: '.c($payload).'</li></ul>';
        },
    );
    yield '<p>footer — flushed while the boundary was still pending</p>';
    yield '</body></html>';
})();

echo stream($page);
