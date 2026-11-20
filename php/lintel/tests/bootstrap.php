<?php

declare(strict_types=1);

// Composer autoloader wires the plugin/loader classes, composer/composer
// (the plugin API the tests exercise) and PHPUnit; the §6.8 runtime helpers
// are required directly — compiled fixtures call \Lintel\c()/a()/u() and
// the dev package deliberately does not depend on lintel/runtime (§8.1:
// the loader is the dev dependency, the runtime is the prod one).
if (!\is_file(__DIR__ . '/../vendor/autoload.php')) {
    fwrite(STDERR, "php/lintel: run `composer install` first (PHPUnit needs the dev tooling)\n");
    exit(1);
}
require __DIR__ . '/../vendor/autoload.php';
require __DIR__ . '/../../runtime/src/functions.php';
