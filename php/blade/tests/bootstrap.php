<?php

declare(strict_types=1);

// The bridge tests need the illuminate/view dev deps (composer autoload)
// and the lintl binary for the dev loader's compile lane — LINTL_TEST_BIN
// (the gate harness export) or the repo's release build.
if (is_file(__DIR__.'/../vendor/autoload.php')) {
    require __DIR__.'/../vendor/autoload.php';
}
$bin = getenv('LINTL_TEST_BIN') ?: __DIR__.'/../../../target/release/lintl';
if (is_file($bin)) {
    putenv('LINTL_BIN='.$bin);
}
