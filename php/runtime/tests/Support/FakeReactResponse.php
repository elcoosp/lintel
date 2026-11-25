<?php

declare(strict_types=1);

namespace Lintel\Tests\Support;

use Lintel\ReactResponse;

/**
 * The structural fake for the `Lintel\ReactResponse` contract
 * (AdapterTransportTest): records the adapter's calls in order.
 */
final class FakeReactResponse implements ReactResponse
{
    /** @var list<string> */
    public array $calls = [];

    #[\Override]
    public function writeHead(int $status, array $headers): void
    {
        $flat = [];
        foreach ($headers as $k => $v) {
            $flat[] = $k.'='.$v;
        }
        $this->calls[] = 'writeHead:'.$status.':'.implode('|', $flat);
    }

    #[\Override]
    public function write(string $chunk): void
    {
        $this->calls[] = 'write:'.$chunk;
    }

    #[\Override]
    public function end(): void
    {
        $this->calls[] = 'end';
    }
}
