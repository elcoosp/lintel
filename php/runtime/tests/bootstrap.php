<?php

declare(strict_types=1);

// With vendor tooling (composer install) the composer autoloader wires the
// runtime (`files`) and the test namespace (`autoload-dev`); the PHAR-only
// path (no vendor/) requires the runtime directly.
if (is_file(__DIR__ . '/../vendor/autoload.php')) {
    require __DIR__ . '/../vendor/autoload.php';
} else {
    require __DIR__ . '/../src/functions.php';
}
