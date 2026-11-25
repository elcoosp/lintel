<?php

declare(strict_types=1);

// The bridge tests need the illuminate/view dev deps (composer autoload)
// and the lintel binary for the dev loader's compile lane — LINTEL_TEST_BIN
// (the gate harness export) or the repo's release build.
if (is_file(__DIR__.'/../vendor/autoload.php')) {
    require __DIR__.'/../vendor/autoload.php';
}
$bin = getenv('LINTEL_TEST_BIN') ?: __DIR__.'/../../../target/release/lintel';
if (is_file($bin)) {
    putenv('LINTEL_BIN='.$bin);
}
