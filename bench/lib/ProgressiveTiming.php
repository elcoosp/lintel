<?php

/**
 * The RoadRunner progressive per-boundary timing lane (Phase 11 M2; the
 * deferred "progressive per-boundary timing on RR is CI-lane work" note
 * in the §10 header comment and docs/deployment/roadrunner.md).
 *
 * What the lane measures: a page with three `<Suspense>` boundaries
 * renders through the real `RoadRunnerFlushTransport` (the §10 RR lane,
 * queueing every chunk in flush order and handing one stream chunk per
 * frame in `respond()`), with each boundary's work modeling deterministic
 * I/O latency (slices of `usleep` interleaved with `Fiber::suspend()`).
 * Every transport write records an `hrtime(true)` mark, so the lane
 * reports
 *
 *  - `ttfb_placeholder_ns` — the time to the FIRST placeholder flush
 *    (`<div id="lt-1">` — §10 step 1: the fallback flushes immediately),
 *  - `ttfb_resolved_ns` — the time to the FIRST resolved chunk
 *    (`<template id="lt-1-r">` + its content),
 *  - `total_ns` — the time to the fully flushed page (the last chunk),
 *
 * and asserts the progressive invariant inside the harness: **the
 * placeholder for boundary N is flushed before the resolved content for
 * boundary N-1**, resolution itself stays in order for the deterministic
 * latencies, and the RR wire order (`respond()` frames) equals the flush
 * order. A broken flusher FAILS the lane (it refuses to produce numbers
 * for a violated contract) instead of reporting timings.
 *
 * The honest RR semantic this pins: the PHP worker is pull-based —
 * `stream_via()` renders to completion while the transport queues, and
 * `respond()` streams the queued chunks; the flush *order* (and therefore
 * the placeholder-before-resolved protocol) is byte-identical to the FPM
 * lane, and the flush *times* are the per-chunk marks a client-driven
 * pull would replay in that order.
 */

declare(strict_types=1);

namespace Lintl\Bench;

use Lintl\FlushTransport;
use Lintl\Html;
use Lintl\RoadRunnerFlushTransport;
use Lintl\Suspense;
use Spiral\RoadRunner\Http\HttpWorkerInterface;
use Spiral\RoadRunner\Http\Request;
use Spiral\RoadRunner\WorkerInterface;

use function Lintl\stream_via;

/**
 * Times every write AND delegates to the real `RoadRunnerFlushTransport`
 * (final class — composed, not subclassed): the RR transport does the
 * queueing, the wrapper records one hrtime mark per flushed chunk.
 */
final class TimedRoadRunnerFlushTransport implements FlushTransport
{
    /** @var list<array{0: int, 1: string}> the (hrtime, chunk) marks, flush order */
    private array $marks = [];

    public function __construct(
        private readonly RoadRunnerFlushTransport $inner,
        private readonly \Closure $clock,
    ) {}

    #[\Override]
    public function begin(): void
    {
        $this->inner->begin();
    }

    #[\Override]
    public function write(string $chunk): void
    {
        $this->marks[] = [($this->clock)(), $chunk];
        $this->inner->write($chunk);
    }

    #[\Override]
    public function end(): void
    {
        $this->inner->end();
    }

    /** @return list<array{0: int, 1: string}> */
    public function marks(): array
    {
        return $this->marks;
    }

    /** Send the queue through the worker response (the RR pull phase). */
    public function respond(): void
    {
        $this->inner->respond();
    }
}

/** The RR-shaped recording worker: one recorded frame per yielded chunk. */
final class RecordingHttpWorker implements HttpWorkerInterface
{
    /** @var list<string> the yielded stream chunks, in pull order */
    public array $frames = [];

    public function waitRequest(): ?Request
    {
        return null;
    }

    public function getWorker(): WorkerInterface
    {
        throw new \LogicException('the recording bench worker is never driver-aware');
    }

    public function respond(int $status, string|\Generator $body, array $headers = [], bool $endOfStream = true): void
    {
        if (\is_string($body)) {
            $this->frames = [$body];

            return;
        }
        foreach ($body as $chunk) {
            $this->frames[] = (string) $chunk;
        }
    }
}

final class ProgressiveTiming
{
    /** The boundary works' slice count per boundary (deterministic latencies). */
    private const SLICES = [3, 6, 9];

    private const SLICE_US = 4000;

    /**
     * Run one measurement. Returns the facts array (metrics, marks, wire
     * frames, invariant booleans); throws when the progressive invariant
     * itself fails — the benchmark refuses to time a broken flusher.
     *
     * @return array<string, mixed>
     */
    public static function run(): array
    {
        $page = self::page();
        $worker = new RecordingHttpWorker;
        $start = \hrtime(true);
        $transport = new TimedRoadRunnerFlushTransport(
            new RoadRunnerFlushTransport($worker, 200, ['Content-Type' => ['text/html; charset=utf-8']]),
            static fn (): int => \hrtime(true),
        );
        stream_via($page, $transport);
        $transport->respond();
        $total = \hrtime(true) - $start;

        return self::facts($transport->marks(), $worker->frames, $total);
    }

    /**
     * The measured page: shell + three boundaries whose works model
     * deterministic latency (SLICES × SLICE_US per boundary, round-robin
     * pumped — resolution order lt-1, lt-2, lt-3).
     */
    private static function page(): \Generator
    {
        yield '<!doctype html><html><body><h1>Progressive</h1>';
        foreach (self::SLICES as $i => $slices) {
            yield '<section>';
            yield new Suspense(
                "Loading {$i}…",
                static function () use ($i, $slices): Html {
                    for ($slice = 0; $slice < $slices; $slice++) {
                        \usleep(self::SLICE_US);
                        \Fiber::suspend();
                    }

                    return Html::unsafeRaw("<p>resolved {$i}</p>");
                },
            );
            yield '</section>';
        }
        yield '<footer>done</footer></body></html>';
    }

    /**
     * Reduce the marks + wire frames into the facts, asserting the
     * progressive invariant.
     *
     * @param  list<array{0: int, 1: string}>  $marks
     * @param  list<string>  $frames
     * @return array<string, mixed>
     */
    private static function facts(array $marks, array $frames, int $totalNs): array
    {
        if ($marks === []) {
            throw new \LogicException('the RR lane flushed no chunks');
        }
        $count = \count(self::SLICES);
        $placeholderAt = [];
        $resolvedAt = [];
        $placeholderTime = [];
        $resolvedTime = [];
        $firstNs = $marks[0][0];
        $lastNs = $marks[0][0];
        foreach ($marks as $index => [$time, $chunk]) {
            $firstNs = \min($firstNs, $time);
            $lastNs = \max($lastNs, $time);
            foreach (\range(1, $count) as $n) {
                if (\str_contains($chunk, '<div id="lt-'.$n.'">')) {
                    $placeholderAt[$n] ??= $index;
                    $placeholderTime[$n] ??= $time;
                }
                if (\str_contains($chunk, '<template id="lt-'.$n.'-r">')) {
                    $resolvedAt[$n] ??= $index;
                    $resolvedTime[$n] ??= $time;
                }
            }
        }

        // Every boundary flushed exactly one placeholder and exactly one
        // resolved template — the §10 protocol, per boundary.
        foreach (\range(1, $count) as $n) {
            if (! isset($placeholderAt[$n], $resolvedAt[$n])) {
                throw new \LogicException("boundary lt-$n did not flush its placeholder + resolved pair");
            }
        }

        // THE invariant: the placeholder for boundary N flushes before the
        // resolved content for boundary N-1 (every placeholder flushes in
        // §10 step 1, before any fiber pumps).
        $progressive = [];
        for ($n = 2; $n <= $count; $n++) {
            $ok = $placeholderAt[$n] < $resolvedAt[$n - 1];
            $progressive[] = ['boundary' => $n, 'before_resolved_of' => $n - 1, 'ok' => $ok];
            if (! $ok) {
                throw new \LogicException(
                    "the progressive invariant failed: placeholder lt-$n flushed at "
                    .$placeholderAt[$n].' but resolved lt-'.($n - 1).' flushed at '.$resolvedAt[$n - 1]
                );
            }
        }

        // Deterministic latencies resolve in document order.
        for ($n = 2; $n <= $count; $n++) {
            if ($resolvedAt[$n] < $resolvedAt[$n - 1]) {
                throw new \LogicException("lt-$n resolved before lt-".($n - 1).' under ordered latencies');
            }
        }

        // The three timing facts order: placeholder ≤ first resolved ≤ total.
        $ttfbPlaceholder = $placeholderTime[1] - $firstNs;
        $ttfbResolved = $resolvedTime[1] - $firstNs;
        $flushTotal = $lastNs - $firstNs;
        if ($ttfbPlaceholder > $ttfbResolved || $ttfbResolved > $flushTotal) {
            throw new \LogicException('the timing facts are out of order: '
                ."placeholder $ttfbPlaceholder ns, resolved $ttfbResolved ns, total $flushTotal ns");
        }

        // The RR wire order equals the flush order: the queued chunks the
        // transport writes are the frames the worker pulls, one per chunk.
        $flushChunks = \array_map(static fn (array $mark): string => $mark[1], $marks);
        if ($flushChunks !== $frames) {
            throw new \LogicException('the RR wire order diverged from the flush order');
        }

        return [
            'boundaries' => $count,
            'slice_us' => self::SLICE_US,
            'ttfb_placeholder_ns' => $ttfbPlaceholder,
            'ttfb_resolved_ns' => $ttfbResolved,
            'total_ns' => $flushTotal,
            'respond_total_ns' => $totalNs,
            'chunks' => \count($marks),
            'invariants' => [
                'placeholder_n_before_resolved_n_minus_1' => true,
                'resolution_in_order' => true,
                'wire_order_equals_flush_order' => true,
            ],
            'progressive' => $progressive,
            'placeholder_at' => $placeholderAt,
            'resolved_at' => $resolvedAt,
        ];
    }
}
