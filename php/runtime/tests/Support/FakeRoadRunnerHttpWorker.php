<?php

declare(strict_types=1);

namespace Lintl\Tests\Support;

use Spiral\RoadRunner\Http\HttpWorkerInterface;
use Spiral\RoadRunner\Http\Request;
use Spiral\RoadRunner\Payload;
use Spiral\RoadRunner\WorkerInterface;

/**
 * The RoadRunner HttpWorker fake (AdapterTransportTest): records
 * `respond()` arguments and drains the response generator exactly the way
 * the real worker does (each yielded value is one stream chunk).
 */
final class FakeRoadRunnerHttpWorker implements HttpWorkerInterface
{
    public int $status = 0;

    /** @var array<string, list<string>> */
    public array $headers = [];

    /** @var list<string> the yielded stream chunks, in order */
    public array $frames = [];

    /** The throwable pulled out of the generator (set when the page errors). */
    public ?\Throwable $pullError = null;

    #[\Override]
    public function getWorker(): WorkerInterface
    {
        return new class implements WorkerInterface
        {
            #[\Override]
            public function waitPayload(): ?Payload
            {
                return null;
            }

            #[\Override]
            public function respond(Payload $payload): void {}

            #[\Override]
            public function error(string $error): void {}

            #[\Override]
            public function stop(): void {}

            #[\Override]
            public function hasPayload(?string $class = null): bool
            {
                return false;
            }

            #[\Override]
            public function getPayload(?string $class = null): ?Payload
            {
                return null;
            }
        };
    }

    #[\Override]
    public function waitRequest(): ?Request
    {
        return null;
    }

    /**
     * @param  array<string, list<string>>  $headers
     * @param  \Generator<mixed, scalar|\Stringable, mixed, mixed>|string  $body
     */
    #[\Override]
    public function respond(int $status, string|\Generator $body, array $headers = [], bool $endOfStream = true): void
    {
        $this->status = $status;
        $this->headers = $headers;
        if (is_string($body)) {
            $this->frames = [$body];

            return;
        }
        $frames = [];
        try {
            foreach ($body as $chunk) {
                $frames[] = (string) $chunk;
            }
        } catch (\Throwable $e) {
            $this->pullError = $e;
        }
        $this->frames = $frames;
    }
}
