<?php

/**
 * The progressive-timing invariant's CLI entry (Phase 11 M2): run the
 * RoadRunner-shaped progressive benchmark once, print the facts as one
 * JSON object on stdout, exit 0 — or exit 1 with the failure on stderr
 * when the progressive invariant fails (the harness throws on a broken
 * flusher; this entry turns that into a gate the CI bench job and the
 * Rust integration test can drive).
 *
 * `php bench/lib/progressive_invariant.php`
 */

declare(strict_types=1);

require __DIR__ . '/../vendor/autoload.php';

use Lintel\Bench\ProgressiveTiming;

try {
    $facts = ProgressiveTiming::run();
} catch (\Throwable $e) {
    \fwrite(STDERR, 'progressive invariant FAILED: ' . $e->getMessage() . "\n");
    exit(1);
}

\fwrite(STDOUT, \json_encode($facts, \JSON_PRETTY_PRINT | \JSON_THROW_ON_ERROR) . "\n");
exit(0);
