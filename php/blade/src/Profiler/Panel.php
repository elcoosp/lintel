<?php

declare(strict_types=1);

namespace Lintl\Blade\Profiler;

use Lintl\Dev\Loader;
use Lintl\SuspenseFlusher;

/**
 * The §11 Laravel profiler panel's data source (§17.1-Q2's remaining
 * Laravel surface): snapshots the dev loader's compile-cache counters
 * (`Loader::metrics()`) and the runtime's flushed-suspense-boundary
 * counter (`SuspenseFlusher::$boundariesFlushed`). `reset()` zeroes both
 * so a controller action can bracket exactly the render window it wants
 * the panel to show. The Debugbar adapter ({@see DebugbarCollector})
 * renders this same snapshot.
 */
final class Panel
{
    /**
     * The metrics since the last reset: `files` = requires served by the
     * dev loader, `cache_hits`/`cache_misses` = compile-cache serves vs
     * compilations run, `compile_time_ms` = wall-clock compile spend,
     * `suspense_flushed` = boundaries whose fallback placeholder flushed.
     *
     * @return array{files: int, cache_hits: int, cache_misses: int, compile_time_ms: float, suspense_flushed: int}
     */
    public function metrics(): array
    {
        $metrics = Loader::metrics();

        return [
            'files' => $metrics->files,
            'cache_hits' => $metrics->hits,
            'cache_misses' => $metrics->misses,
            'compile_time_ms' => \round($metrics->compileTimeMs, 3),
            'suspense_flushed' => SuspenseFlusher::$boundariesFlushed,
        ];
    }

    /** Zero the counters, bracketing the render window the panel measures. */
    public function reset(): void
    {
        Loader::resetMetrics();
        SuspenseFlusher::$boundariesFlushed = 0;
    }
}
