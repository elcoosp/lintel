<?php declare(strict_types=1);
namespace Lintel;

use Spiral\RoadRunner\Http\HttpWorkerInterface;

/**
 * §10 server adapters (architecture doc §10: "FPM …, FrankenPHP,
 * RoadRunner, Swoole/OpenSwole, ReactPHP/AMPHP via adapters").
 *
 * The adapters plug the [`FlushTransport`] seam into the real streaming
 * PHP servers. Chunk CONTENT is never touched here: every dynamic value
 * was already escaped by the compiler-emitted `c()/a()/u()/…` calls (or by
 * the stream lowering that yields through the same helpers), so streaming
 * can never bypass the escapers — the adapters only own HOW a chunk
 * reaches the wire.
 *
 * Swoole and ReactPHP push chunks onto an already-open response object;
 * FrankenPHP streams through the same `echo + flush` shape the FPM lane
 * uses; RoadRunner's PHP worker is pull-based (`HttpWorker::respond()`
 * drains a generator, one midflight frame per yielded chunk), so its
 * transport queues the flush-ordered chunks during `stream_via()` and
 * `respond()` hands them to the worker afterwards — the flush order, the
 * suspense protocol and the swap script are preserved exactly. The
 * progressive per-boundary timing is measured and asserted by the Phase 11
 * lane (`bench/lib/ProgressiveTiming.php`, CI's `bench` job): placeholder
 * N before resolved N-1, wire order == flush order.
 */

/** The structural surface of `Swoole\Http\Response` the Swoole lane needs.
 * ext-swoole ships no autoloadable interface, so the lane defines its own. */
interface SwooleResponse
{
    public function status(int $code): void;
    public function header(string $key, string $value): void;
    public function write(string $chunk): void;
    public function end(): void;
}

/** The structural surface of ReactPHP's `React\Http\Response` stream API. */
interface ReactResponse
{
    /** @param array<string, string> $headers */
    public function writeHead(int $status, array $headers): void;
    public function write(string $chunk): void;
    public function end(): void;
}

/** The Swoole/OpenSwoole lane (§10): status + headers on begin, one
 * `write()` per chunk, `end()` closes. Construct with a
 * [`SwooleResponse`] (or adapt a raw response object via `fromSwoole()`). */
final class SwooleFlushTransport implements FlushTransport
{
    public function __construct(private readonly SwooleResponse $response) {}

    /** Adapt a raw ext-swoole `Swoole\Http\Response` (or OpenSwoole's).
     * The object surface is validated up front; a mismatch throws before
     * any byte is written. */
    public static function fromSwoole(object $response): self
    {
        foreach (['status', 'header', 'write', 'end'] as $method) {
            if (!\method_exists($response, $method)) {
                throw new \InvalidArgumentException(
                    'not a Swoole HTTP response: missing ' . $method . '()',
                );
            }
        }

        return new self(new class($response) implements SwooleResponse {
            public function __construct(private readonly object $response) {}

            // @phpstan-ignore method.notFound (ext-swoole's response ships no autoloadable interface — the duck type is the contract, validated in fromSwoole())
            public function status(int $code): void { $this->response->status($code); }

            // @phpstan-ignore method.notFound (see above)
            public function header(string $key, string $value): void { $this->response->header($key, $value); }

            // @phpstan-ignore method.notFound (see above)
            public function write(string $chunk): void { $this->response->write($chunk); }

            // @phpstan-ignore method.notFound (see above)
            public function end(): void { $this->response->end(); }
        });
    }

    /** The runtime is present when the Swoole extension is loaded. */
    public static function isAvailable(): bool
    {
        return \extension_loaded('swoole') || \class_exists('Swoole\Http\Response');
    }

    public function begin(): void
    {
        $this->response->status(200);
        $this->response->header('Content-Type', 'text/html; charset=utf-8');
        // nginx in front of Swoole must not buffer the stream (§10).
        $this->response->header('X-Accel-Buffering', 'no');
    }

    public function write(string $chunk): void
    {
        $this->response->write($chunk);
    }

    public function end(): void
    {
        $this->response->end();
    }
}

/** The ReactPHP lane (§10): `writeHead()` on begin, one `write()` per
 * chunk, `end()` closes — `React\Http\HttpServer`'s streaming response
 * shape. Construct with a [`ReactResponse`] (or adapt a raw response via
 * `fromReact()`). */
final class ReactPHPFlushTransport implements FlushTransport
{
    public function __construct(private readonly ReactResponse $response) {}

    /** Adapt a raw `React\Http\Response`-shaped object (the surface the
     * `HttpServer` request callback receives). Validated up front. */
    public static function fromReact(object $response): self
    {
        foreach (['writeHead', 'write', 'end'] as $method) {
            if (!\method_exists($response, $method)) {
                throw new \InvalidArgumentException(
                    'not a ReactPHP HTTP response: missing ' . $method . '()',
                );
            }
        }

        return new self(new class($response) implements ReactResponse {
            public function __construct(private readonly object $response) {}

            // @phpstan-ignore method.notFound (React\Http\Response ships no structural interface — the duck type is the contract, validated in fromReact())
            public function writeHead(int $status, array $headers): void { $this->response->writeHead($status, $headers); }

            // @phpstan-ignore method.notFound (see above)
            public function write(string $chunk): void { $this->response->write($chunk); }

            // @phpstan-ignore method.notFound (see above)
            public function end(): void { $this->response->end(); }
        });
    }

    /** The runtime is present when the react/http response class is autoloadable. */
    public static function isAvailable(): bool
    {
        return \class_exists('React\Http\Response');
    }

    public function begin(): void
    {
        $this->response->writeHead(200, [
            'Content-Type' => 'text/html; charset=utf-8',
            'X-Accel-Buffering' => 'no',
        ]);
    }

    public function write(string $chunk): void
    {
        $this->response->write($chunk);
    }

    public function end(): void
    {
        $this->response->end();
    }
}

/** The FrankenPHP lane (§10): the same `echo + flush` streaming shape the
 * FPM lane uses, with `X-Accel-Buffering: no` so nothing buffers the
 * stream ahead of the worker. */
final class FrankenPHPFlushTransport implements FlushTransport
{
    /** The runtime is present inside a FrankenPHP worker. */
    public static function isAvailable(): bool
    {
        $software = $_SERVER['SERVER_SOFTWARE'] ?? '';
        $software = \is_string($software) ? $software : '';

        return \getenv('FRANKENPHP_WORKER') !== false || \str_contains($software, 'FrankenPHP');
    }

    public function begin(): void
    {
        \header('Content-Type: text/html; charset=utf-8');
        \header('X-Accel-Buffering: no');
        while (\ob_get_level() > 0) {
            \ob_end_flush();
        }
    }

    public function write(string $chunk): void
    {
        echo $chunk;
        \flush();
    }

    public function end(): void
    {
        \flush();
    }
}

/**
 * The RoadRunner lane (§10): `rr serve` HTTP workers stream through
 * `HttpWorkerInterface::respond(status, generator, headers)` — one
 * midflight frame per yielded chunk. The PHP worker API is pull-based,
 * so `stream_via()` runs first with the transport queueing every chunk in
 * flush order, and `respond()` then hands the queued chunks to the worker
 * as stream chunks: the flush order, the `<template id="lt-N-r">` + swap
 * script protocol and the escaper contract are byte-identical to the FPM
 * lane; the progressive per-boundary flush timing is measured and
 * asserted by the Phase 11 M2 lane (the bench S6 section:
 * placeholder N before resolved N-1, wire order == flush order).
 *
 * Headers use the RR worker's shape — `array<string, list<string>>`.
 */
final class RoadRunnerFlushTransport implements FlushTransport
{
    /** @var list<string> the flush-ordered chunks */
    private array $queue = [];
    private bool $closed = false;

    /** @param array<string, list<string>> $headers */
    public function __construct(
        private readonly HttpWorkerInterface $worker,
        private readonly int $status = 200,
        private readonly array $headers = ['Content-Type' => ['text/html; charset=utf-8']],
    ) {}

    /** The runtime is present inside an RR HTTP worker (RR_MODE=http). */
    public static function isAvailable(): bool
    {
        return \getenv('RR_MODE') === 'http';
    }

    public function begin(): void {}

    public function write(string $chunk): void
    {
        $this->queue[] = $chunk;
    }

    public function end(): void
    {
        $this->closed = true;
    }

    /** Send the queued stream through the worker response. Every chunk
     * becomes one stream chunk of the RR HTTP response (the swap script
     * included, after its resolved template). */
    public function respond(): void
    {
        if (!$this->closed) {
            throw new \LogicException('respond() requires a completed stream_via() run');
        }
        $this->worker->respond($this->status, $this->chunks(), $this->headers);
    }

    /** @return \Generator<int, string> one queued chunk per stream chunk */
    private function chunks(): \Generator
    {
        yield from $this->queue;
    }
}
