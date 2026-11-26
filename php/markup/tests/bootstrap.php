<?php

declare(strict_types=1);

// The markup package's vendor autoloader wires both halves of the e2e:
// the `\Markup\` runtime classes and (through the path-repo dev dep) the
// `\Lintl\` runtime the string-target comparison lane renders through.
if (is_file(__DIR__.'/../vendor/autoload.php')) {
    require __DIR__.'/../vendor/autoload.php';
} else {
    // PHAR-only path: the runtime source tree directly.
    require __DIR__.'/../../runtime/src/functions.php';
    require __DIR__.'/../src/functions.php';
}
