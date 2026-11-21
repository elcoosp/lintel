<?php

declare(strict_types=1);

namespace Lintel\Dev;

/**
 * The dev loader's compile-cache counters (§11 ecosystem: the Laravel
 * Debugbar collector and the Symfony profiler panel read them). One
 * immutable snapshot of the process-global counts `Loader::require()`
 * accumulates: `files` is every require served, `misses` the compilations
 * actually run (each miss spends wall time in `compile()`), `hits` the
 * cache-key serves, `compileTimeMs` the loader's own wall-clock compile
 * spend (hrtime, milliseconds). Panels snapshot the counters before and
 * after a render window — `Loader::resetMetrics()` zeroes them.
 */
final class LoaderMetrics
{
    public function __construct(
        public readonly int $files,
        public readonly int $hits,
        public readonly int $misses,
        public readonly float $compileTimeMs,
    ) {}

    /** @return array{files: int, hits: int, misses: int, compile_time_ms: float} */
    public function toArray(): array
    {
        return [
            'files' => $this->files,
            'hits' => $this->hits,
            'misses' => $this->misses,
            'compile_time_ms' => $this->compileTimeMs,
        ];
    }
}
