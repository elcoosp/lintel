<?php

declare(strict_types=1);

namespace Lintel\Tests;

use Lintel\CliFlushTransport;
use Lintel\FlushTransport;
use Lintel\Html;
use Lintel\Suspense;
use Lintel\Tests\Support\StreamingPage;
use PHPUnit\Framework\TestCase;

use function Lintel\stream_via;

/**
 * The §10 flush transports (architecture doc §10: "FPM (flush +
 * `X-Accel-Buffering: no`), FrankenPHP, RoadRunner, Swoole/OpenSwoole …
 * via adapters"): the transport seam, the CLI lane and the FPM lane —
 * the FPM adapter exercised over a real `php -S` HTTP server. The live
 * RoadRunner/Swoole/FrankenPHP lanes are the CI adapters job's business
 * (no such server exists in the sandbox).
 */
final class StreamingTransportTest extends TestCase
{
    /** The seam: begin before the first chunk, one write per chunk, end last. */
    public function test_transport_seam_lifecycle(): void
    {
        $events = [];
        $record = static function (string $event) use (&$events): void {
            $events[] = $event;
        };
        $transport = new class($record) implements FlushTransport
        {
            private \Closure $record;

            public function __construct(\Closure $record)
            {
                $this->record = $record;
            }

            public function begin(): void
            {
                ($this->record)('begin');
            }

            public function write(string $chunk): void
            {
                ($this->record)('w:'.$chunk);
            }

            public function end(): void
            {
                ($this->record)('end');
            }
        };

        stream_via(StreamingPage::shell(), $transport);

        self::assertSame(
            ['begin', 'w:<p>a</p>', 'w:<p>b</p>', 'w:<p>c</p>', 'end'],
            $events,
        );
    }

    /** The CLI adapter echoes every chunk to the output stream, flushed. */
    public function test_cli_adapter_echoes_chunks_flushed(): void
    {
        \ob_start();
        try {
            stream_via(StreamingPage::shell(), new CliFlushTransport);
        } finally {
            $out = \ob_get_clean();
        }
        self::assertNotFalse($out);
        self::assertSame('<p>a</p><p>b</p><p>c</p>', $out);
    }

    /** The FPM adapter over a real HTTP server (php -S): the §10 headers go
     * out, the placeholder precedes the resolved template, the payload that
     * arrived in the late-flushed chunk prints escaped. */
    public function test_fpm_adapter_over_http_server(): void
    {
        $docroot = sys_get_temp_dir().'/lintel-stream-'.bin2hex((string) random_bytes(4));
        mkdir($docroot, 0777, true);
        $runtime = \dirname(__DIR__).'/src/functions.php';
        $router = <<<'PHP'
<?php declare(strict_types=1);
require {RUNTIME};

use Lintel\FpmFlushTransport;
use Lintel\Suspense;

use function Lintel\c;
use function Lintel\stream_via;

$payload = '<script>alert("pwned")</script>';
$page = (static function () use ($payload): \Generator {
    yield '<!doctype html><html><body><h1>FPM adapter</h1>';
    yield new Suspense('loading…', static function () use ($payload): \Generator {
        \usleep(10_000); // the slow boundary
        yield '<p>user: ' . c($payload) . '</p>';
    });
    yield '<p>footer</p></body></html>';
})();
stream_via($page, new FpmFlushTransport());
PHP;
        $router = str_replace('{RUNTIME}', var_export($runtime, true), $router);
        $path = $docroot.'/router.php';
        file_put_contents($path, $router);

        $body = false;
        /** @var resource|null $server */
        $server = null;
        $headers = [];
        for ($attempt = 0; $attempt < 10; $attempt++) {
            $proc = @proc_open(
                [PHP_BINARY, '-S', '127.0.0.1:'.(22000 + $attempt), '-t', $docroot],
                [1 => ['pipe', 'w'], 2 => ['pipe', 'w']],
                $pipes,
            );
            if (! is_resource($proc)) {
                continue;
            }
            foreach ($pipes as $pipe) {
                fclose($pipe);
            }
            usleep(200_000);
            $context = stream_context_create(['http' => ['ignore_errors' => true]]);
            $body = @file_get_contents('http://127.0.0.1:'.(22000 + $attempt).'/router.php', false, $context);
            if ($body !== false) {
                $headers = http_get_last_response_headers() ?? [];
                $server = $proc;
                break;
            }
            proc_terminate($proc);
            proc_close($proc);
        }
        try {
            self::assertNotFalse($body, 'the php -S fixture server never served the page');
            self::assertStringContainsString('<h1>FPM adapter</h1>', $body);
            self::assertStringContainsString('<div id="lt-1">', $body);
            self::assertStringContainsString('<template id="lt-1-r">', $body);
            self::assertTrue(
                strpos((string) $body, '<div id="lt-1">') < strpos((string) $body, '<template id="lt-1-r">'),
                'the placeholder must precede the resolved template over HTTP',
            );
            self::assertTrue(
                strpos((string) $body, '<p>footer</p>') < strpos((string) $body, '<template id="lt-1-r">'),
                'the shell must finish before the late-flushed chunk: '.(string) $body,
            );
            self::assertStringContainsString('&lt;script&gt;alert(&quot;pwned&quot;)&lt;/script&gt;', $body);
            self::assertStringNotContainsString('<script>alert("pwned")</script>', $body);
            $flat = implode("\n", $headers);
            self::assertStringContainsString('X-Accel-Buffering: no', $flat, $flat);
            self::assertStringContainsString('Content-Type: text/html; charset=utf-8', $flat, $flat);
        } finally {
            if ($server !== null) {
                proc_terminate($server);
                proc_close($server);
            }
        }
    }

    /** A boundary resolved through the transport seam still swaps in place. */
    public function test_suspense_through_transport_seam(): void
    {
        \ob_start();
        try {
            stream_via((static function (): \Generator {
                yield '<main>';
                yield new Suspense('wait…', static fn (): Html => Html::unsafeRaw('<p>done</p>'));
                yield '</main>';
            })(), new CliFlushTransport);
        } finally {
            $out = \ob_get_clean();
        }
        self::assertNotFalse($out);
        self::assertTrue(
            strpos((string) $out, '<div id="lt-1">') !== false
            && strpos((string) $out, '<template id="lt-1-r">') > strpos((string) $out, '</main>'),
            $out,
        );
        self::assertStringContainsString('</template><script>lintelSwap("lt-1")</script>', $out);
    }
}
