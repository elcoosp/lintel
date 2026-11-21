<?php

declare(strict_types=1);

// The bridge tests need the twig dev deps (composer autoload) and the
// lintel binary for the dev loader's compile lane.
if (is_file(__DIR__ . '/../vendor/autoload.php')) {
    require __DIR__ . '/../vendor/autoload.php';
}
$bin = getenv('LINTEL_TEST_BIN') ?: __DIR__ . '/../../../target/release/lintel';
if (is_file($bin)) {
    putenv('LINTEL_BIN=' . $bin);
}
