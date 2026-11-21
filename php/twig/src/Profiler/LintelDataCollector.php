<?php

declare(strict_types=1);

namespace Lintel\Twig\Profiler;

use Lintel\Dev\Loader;
use Lintel\SuspenseFlusher;
use Symfony\Component\HttpFoundation\Request;
use Symfony\Component\HttpFoundation\Response;
use Symfony\Component\HttpKernel\DataCollector\DataCollector;

/**
 * The §11 Symfony Web Profiler panel's data collector (§17.1-Q2's
 * remaining Symfony surface): captures the same metrics the Laravel
 * panel shows — files compiled, cache hits/misses, compilation time,
 * suspense boundaries flushed — from the dev loader's counters and the
 * runtime's flusher counter, over exactly the request window the
 * profiler profiles. `reset()` zeroes the counters (the profiler toolbar's
 * reset consumes it). The bundle template (`Resources/views/lintel.html.twig`)
 * renders the snapshot inside `@WebProfiler/Profiler/panel_layout.html.twig`.
 */
final class LintelDataCollector extends DataCollector
{
    /** @var array{files: int, cache_hits: int, cache_misses: int, compile_time_ms: float, suspense_flushed: int}
     * The typed snapshot `metrics()` hands out (mirrored into the
     * parent's `$data` so the profiler's serialization sees it). */
    private array $snapshot = [
        'files' => 0,
        'cache_hits' => 0,
        'cache_misses' => 0,
        'compile_time_ms' => 0.0,
        'suspense_flushed' => 0,
    ];

    /** Snapshot the counters at response time — the window the panel
     * shows spans everything since the last `reset()` (an app bracketing
     * requests zeroes them at kernel.request; a long-running worker shows
     * the counters since ITS last reset). */
    public function collect(Request $request, Response $response, ?\Throwable $exception = null): void
    {
        $metrics = Loader::metrics();

        $this->snapshot = [
            'files' => $metrics->files,
            'cache_hits' => $metrics->hits,
            'cache_misses' => $metrics->misses,
            'compile_time_ms' => \round($metrics->compileTimeMs, 3),
            'suspense_flushed' => SuspenseFlusher::$boundariesFlushed,
        ];
        $this->data = $this->snapshot;
    }

    /** The panel id and the collector name. */
    public function getName(): string
    {
        return 'lintel';
    }

    /** @return array{files: int, cache_hits: int, cache_misses: int, compile_time_ms: float, suspense_flushed: int} */
    public function metrics(): array
    {
        return $this->snapshot;
    }

    /** Zero the counters (the profiler toolbar reset). */
    public function reset(): void
    {
        Loader::resetMetrics();
        SuspenseFlusher::$boundariesFlushed = 0;
        $this->snapshot = [
            'files' => 0,
            'cache_hits' => 0,
            'cache_misses' => 0,
            'compile_time_ms' => 0.0,
            'suspense_flushed' => 0,
        ];
        $this->data = $this->snapshot;
    }
}
