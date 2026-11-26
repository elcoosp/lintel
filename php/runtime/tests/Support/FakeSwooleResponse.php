<?php

declare(strict_types=1);

namespace Lintl\Tests\Support;

use Lintl\SwooleResponse;

/**
 * The structural fake for the `Lintl\SwooleResponse` contract
 * (AdapterTransportTest): records the adapter's calls in order.
 */
final class FakeSwooleResponse implements SwooleResponse
{
    /** @var list<string> */
    public array $calls = [];

    #[\Override]
    public function status(int $code): void
    {
        $this->calls[] = 'status:'.$code;
    }

    #[\Override]
    public function header(string $key, string $value): void
    {
        $this->calls[] = 'header:'.$key.':'.$value;
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
