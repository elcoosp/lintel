<?php

declare(strict_types=1);
use Lintel\Dev\Loader;

/**
 * The starter's entrypoint. `composer create-project lintel/starter my-app`
 * && `cd my-app && php -S localhost:8000` serves this file. It:
 *   1. loads the Composer autoloader (the installer plugin generated the
 *      lintel binary shim at vendor/bin/lintel and the runtime functions)
 *   2. boots the dev loader (compiles .lintel.php sources on demand — NO
 *      watcher, NO Rust toolchain; the installer plugin fetched the
 *      prebuilt binary on `composer install`)
 *   3. renders the App\Page component with two cards
 *
 * The dev loader is the no-watcher lane (architecture §6.10, §12): a
 * request recompiles only changed sources (the cache key is mtime+size),
 * so the first request is a cold compile and subsequent requests serve
 * the cached PHP. `lintel build` (the `composer build` script) pre-builds
 * for prod (the T-31 manifest lane — no dev loader, no manual require).
 */
require __DIR__.'/../vendor/autoload.php';

Loader::boot(__DIR__.'/../cache', projectRoot: dirname(__DIR__));

echo \App\Page('Hello from the Lintel starter', [
    ['title' => 'The Lintel docs', 'url' => 'https://elcoosp.github.io/lintel/'],
    ['title' => 'The GitHub repo', 'url' => 'https://github.com/elcoosp/lintel'],
]);
