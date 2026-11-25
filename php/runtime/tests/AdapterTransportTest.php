<?php

declare(strict_types=1);

namespace Lintel\Tests;

use Lintel\FrankenPHPFlushTransport;
use Lintel\Html;
use Lintel\ReactPHPFlushTransport;
use Lintel\RoadRunnerFlushTransport;
use Lintel\Suspense;
use Lintel\SwooleFlushTransport;
use Lintel\Tests\Support\FakeReactResponse;
use Lintel\Tests\Support\FakeRoadRunnerHttpWorker;
use Lintel\Tests\Support\FakeSwooleResponse;
use Lintel\Tests\Support\StreamingPage;
use PHPUnit\Framework\TestCase;

use function Lintel\stream_via;

/**
 * The §10 server adapters (architecture doc §10: "FPM (flush +
 * `X-Accel-Buffering: no`), FrankenPHP, RoadRunner, Swoole/OpenSwoole,
 * ReactPHP/AMPHP via adapters") — the transports the CI `adapters` step
 * drives. The Swoole/ReactPHP lanes run against structural fakes (the
 * extensions ship no autoloadable surface); the FrankenPHP lane is
 * exercised end-to-end over a real `php -S` HTTP server (the same
 * echo + flush shape the live FrankenPHP serves) and the RoadRunner lane
 * end-to-end against a real `rr serve` application server when the rr
 * binary is on PATH (CI: the php job's `adapters` step).
 */
final class AdapterTransportTest extends TestCase
{
    /** Swoole: begin() sends status + headers once, write() per chunk, end() last. */
    public function test_swoole_adapter_streams_status_headers_chunks_end(): void
    {
        $swoole = new FakeSwooleResponse;
        stream_via(StreamingPage::shell(), new SwooleFlushTransport($swoole));

        self::assertSame(
            ['status:200', 'header:Content-Type:text/html; charset=utf-8', 'header:X-Accel-Buffering:no',
                'write:<p>a</p>', 'write:<p>b</p>', 'write:<p>c</p>', 'end'],
            $swoole->calls,
        );
    }

    /** The fromSwoole factory adapts the raw ext-swoole response object. */
    public function test_swoole_factory_wraps_the_extension_response(): void
    {
        $swoole = new class
        {
            /** @var list<string> */
            public array $calls = [];

            public function status(int $code): void
            {
                $this->calls[] = 'status:'.$code;
            }

            public function header(string $k, string $v): void
            {
                $this->calls[] = 'header:'.$k.':'.$v;
            }

            public function write(string $d): void
            {
                $this->calls[] = 'write:'.$d;
            }

            public function end(): void
            {
                $this->calls[] = 'end';
            }
        };

        stream_via(StreamingPage::shell(), SwooleFlushTransport::fromSwoole($swoole));

        self::assertSame(
            ['status:200', 'header:Content-Type:text/html; charset=utf-8', 'header:X-Accel-Buffering:no',
                'write:<p>a</p>', 'write:<p>b</p>', 'write:<p>c</p>', 'end'],
            $swoole->calls,
        );
    }

    /** ReactPHP: writeHead on begin, one write per chunk, end last. */
    public function test_react_adapter_streams_write_head_chunks_end(): void
    {
        $react = new FakeReactResponse;
        stream_via(StreamingPage::shell(), new ReactPHPFlushTransport($react));

        self::assertSame(
            ['writeHead:200:Content-Type=text/html; charset=utf-8|X-Accel-Buffering=no',
                'write:<p>a</p>', 'write:<p>b</p>', 'write:<p>c</p>', 'end'],
            $react->calls,
        );
    }

    /** The fromReact factory adapts a raw React\Http\Response-style object. */
    public function test_react_factory_wraps_the_library_response(): void
    {
        $react = new class
        {
            /** @var list<string> */
            public array $calls = [];

            /** @param array<string, string> $headers */
            public function writeHead(int $status, array $headers): void
            {
                $flat = [];
                foreach ($headers as $k => $v) {
                    $flat[] = $k.'='.$v;
                }
                $this->calls[] = 'writeHead:'.$status.':'.implode('|', $flat);
            }

            public function write(string $d): void
            {
                $this->calls[] = 'write:'.$d;
            }

            public function end(): void
            {
                $this->calls[] = 'end';
            }
        };

        stream_via(StreamingPage::shell(), ReactPHPFlushTransport::fromReact($react));

        self::assertSame(
            ['writeHead:200:Content-Type=text/html; charset=utf-8|X-Accel-Buffering=no',
                'write:<p>a</p>', 'write:<p>b</p>', 'write:<p>c</p>', 'end'],
            $react->calls,
        );
    }

    /** FrankenPHP (echo + flush shape): the chunks reach the client in
     * order. Run in a subprocess — the adapter's begin() tears down every
     * output buffer, which is PHPUnit's own capture buffer here. */
    public function test_franken_php_adapter_echoes_chunks_flushed(): void
    {
        $script = <<<'PHP'
<?php declare(strict_types=1);
require {RUNTIME};
$chunk = static function (string $c): void { echo $c; };
$t = new Lintel\FrankenPHPFlushTransport();
$t->begin();
$t->write('<p>a</p>');
$t->write('<p>b</p>');
$t->write('<p>c</p>');
$t->end();
PHP;
        $script = str_replace('{RUNTIME}', var_export(\dirname(__DIR__).'/src/functions.php', true), $script);
        $path = sys_get_temp_dir().'/lintel-frankenphp-'.bin2hex((string) random_bytes(4)).'.php';
        file_put_contents($path, $script);
        try {
            $out = shell_exec(escapeshellarg(PHP_BINARY).' '.escapeshellarg($path).' 2>&1');
            self::assertSame('<p>a</p><p>b</p><p>c</p>', $out);
        } finally {
            @unlink($path);
        }
    }

    /** FrankenPHP over a real HTTP server: §10 headers + the suspense
     * protocol (placeholder first, escaped late payload, swap in place). */
    public function test_franken_php_adapter_over_http_server(): void
    {
        $this->assertStreamedPageOverHttpServer(
            'stream_via($page, new \\Lintel\\FrankenPHPFlushTransport());',
            'FrankenPHP adapter',
        );
    }

    /** RoadRunner: respond() maps each queued chunk to one stream chunk of
     * the worker response, headers ride on the response, the swap script
     * is its own chunk after the resolved template's close tag. */
    public function test_road_runner_adapter_responds_per_chunk_with_headers(): void
    {
        $worker = new FakeRoadRunnerHttpWorker;
        $transport = new RoadRunnerFlushTransport($worker, 200, ['Content-Type' => ['text/html; charset=utf-8']]);
        stream_via((static function (): \Generator {
            yield '<main>';
            yield new Suspense('wait…', static fn (): Html => Html::unsafeRaw('<p>done</p>'));
            yield '</main>';
        })(), $transport);
        $transport->respond();

        self::assertSame(200, $worker->status);
        self::assertSame(['Content-Type' => ['text/html; charset=utf-8']], $worker->headers);
        $script = \array_search('<script>lintelSwap("lt-1")</script>', $worker->frames, true);
        self::assertNotFalse($script, 'the swap script must be its own stream chunk');
        $template = \array_search('<template id="lt-1-r">', $worker->frames, true);
        self::assertNotFalse($template, 'the resolved template must precede the swap script');
        self::assertTrue($script > $template);
        self::assertSame('<script>lintelSwap("lt-1")</script>', $worker->frames[\count($worker->frames) - 1]);
        self::assertContains('<div id="lt-1">', $worker->frames);
        self::assertContains('<main>', $worker->frames);
    }

    /** RoadRunner end-to-end against a real `rr serve` application server:
     * the §10 suspense page streams through the worker response with the
     * swap script and an escaped late payload. Skipped when the rr binary
     * is not on PATH — CI runs it in the php job's `adapters` step. */
    public function test_road_runner_adapter_end_to_end_over_rr_server(): void
    {
        if (getenv('LINTEL_E2E_RR') !== '1' || trim((string) shell_exec('command -v rr 2>/dev/null')) === '') {
            self::markTestSkipped('the live RoadRunner lane needs the rr binary + LINTEL_E2E_RR=1 '
                .'(CI: the php job `adapters` step installs both)');
        }
        $this->assertStreamedPageOverHttpServer(<<<'PHP'
$rr = new \Spiral\RoadRunner\Http\HttpWorker(\Spiral\RoadRunner\Worker::create());
while ($rr->waitRequest() !== null) {
    $t = new \Lintel\RoadRunnerFlushTransport($rr, 200, [
        'Content-Type' => ['text/html; charset=utf-8'],
        'X-Accel-Buffering' => ['no'],
    ]);
    stream_via($page, $t);
    $t->respond();
}
PHP, 'RoadRunner adapter', true);
    }

    /** Environment probes: the RR lane is detected via the RR_MODE worker
     * env, Swoole via the extension (absent in the sandbox), FrankenPHP
     * via its worker env marker. */
    public function test_adapter_environment_probes(): void
    {
        self::assertSame(
            \extension_loaded('swoole') || \class_exists('Swoole\\Http\\Response'),
            SwooleFlushTransport::isAvailable(),
            'the probe must mirror the environment (the CI adapters-live job loads the ext; the php job does not)',
        );
        $had = getenv('RR_MODE');
        putenv('RR_MODE=http');
        try {
            self::assertTrue(RoadRunnerFlushTransport::isAvailable());
        } finally {
            putenv($had === false ? 'RR_MODE' : 'RR_MODE='.$had);
        }
        self::assertFalse(RoadRunnerFlushTransport::isAvailable());
        self::assertFalse(FrankenPHPFlushTransport::isAvailable());
        putenv('FRANKENPHP_WORKER=1');
        try {
            self::assertTrue(FrankenPHPFlushTransport::isAvailable());
        } finally {
            putenv('FRANKENPHP_WORKER');
        }
    }

    /**
     * Shared HTTP lane: build a §10 suspense page over a real HTTP server
     * (`php -S`, or `rr serve` when $drivePhp drives the RoadRunner
     * transport) and assert the streamed bytes. $drivePhp is the worker
     * script's driving code — the page is built, then this snippet flushes
     * it through the adapter under test.
     */
    private function assertStreamedPageOverHttpServer(string $drivePhp, string $label, bool $viaRR = false): void
    {
        $docroot = sys_get_temp_dir().'/lintel-adapter-'.bin2hex((string) random_bytes(4));
        mkdir($docroot, 0777, true);
        $autoload = \dirname(__DIR__).'/vendor/autoload.php';
        $body = false;
        $headers = [];

        for ($attempt = 0; $attempt < 10; $attempt++) {
            $port = 23000 + $attempt;
            $worker = <<<'PHP'
<?php declare(strict_types=1);
require {AUTOLOAD};

use Lintel\Suspense;

use function Lintel\c;
use function Lintel\stream_via;

$payload = '<script>alert("pwned")</script>';
$page = (static function () use ($payload): \Generator {
    yield '<!doctype html><html><body><h1>{LABEL}</h1>';
    yield new Suspense('loading…', static function () use ($payload): \Generator {
        \usleep(10_000); // the slow boundary
        yield '<p>user: ' . c($payload) . '</p>';
    });
    yield '<p>footer</p></body></html>';
})();
{DRIVE}
PHP;
            $worker = str_replace(['{AUTOLOAD}', '{LABEL}', '{DRIVE}'], [
                var_export($autoload, true), $label, $drivePhp,
            ], $worker);
            $path = $docroot.'/worker.php';
            file_put_contents($path, $worker);

            $proc = $viaRR ? $this->startRRServer($docroot, $port, $path) : $this->startPhpServer($docroot, $port);
            // rr recycles stream-mode workers per request: poll until a
            // GOOD page body arrives (empty/error responses while a fresh
            // worker boots are retried, the way the FPM lane retries).
            $deadline = microtime(true) + ($viaRR ? 8 : 2);
            while (microtime(true) < $deadline) {
                usleep(300_000);
                $context = stream_context_create(['http' => ['ignore_errors' => true, 'timeout' => 5]]);
                $fetched = @file_get_contents('http://127.0.0.1:'.$port.'/worker.php', false, $context);
                if ($fetched !== false && str_contains($fetched, '<h1>'.$label.'</h1>')) {
                    $body = $fetched;
                    $headers = http_get_last_response_headers() ?? [];
                    break;
                }
            }
            if (is_resource($proc)) {
                proc_terminate($proc);
                proc_close($proc);
            }
            if ($body !== false) {
                break;
            }
        }

        try {
            self::assertNotFalse($body, "the {$label} fixture server never served the page");
            self::assertStreamedPageResponse((string) $body, $headers, $label);
        } finally {
            @unlink($docroot.'/worker.php');
            @unlink($docroot.'/.rr.yaml');
            @unlink($docroot.'/rr.log');
            @unlink($docroot.'/rr.err');
            @rmdir($docroot);
        }
    }

    /** The §10 wire contract, shared by every live lane (php -S, rr,
     * the real FrankenPHP binary, the ext-swoole server): shell first,
     * placeholder before its resolved template, the late chunk escaped,
     * and `X-Accel-Buffering: no` + the content type on the wire.
     *
     * @param  list<string>  $headers  the last response headers from the fetch
     */
    private function assertStreamedPageResponse(string $body, array $headers, string $label): void
    {
        self::assertStringContainsString('<h1>'.$label.'</h1>', $body);
        self::assertStringContainsString('<div id="lt-1">', $body);
        self::assertStringContainsString('<template id="lt-1-r">', $body);
        self::assertTrue(
            strpos($body, '<div id="lt-1">') < strpos($body, '<template id="lt-1-r">'),
            'the placeholder must precede the resolved template over HTTP',
        );
        self::assertTrue(
            strpos($body, '<p>footer</p>') < strpos($body, '<template id="lt-1-r">'),
            "the shell must finish before the late-flushed chunk: {$body}",
        );
        self::assertStringContainsString('&lt;script&gt;alert(&quot;pwned&quot;)&lt;/script&gt;', $body);
        self::assertStringNotContainsString('<p>user: <script>alert("pwned")</script></p>', $body);
        $flat = implode("\n", $headers);
        self::assertStringContainsString('X-Accel-Buffering: no', $flat, $flat);
        self::assertStringContainsString('Content-Type: text/html; charset=utf-8', $flat, $flat);
    }

    /** The M2 live FrankenPHP lane: the REAL frankenphp binary (the
     * Caddy-based runtime, a static release build) serves the streamed
     * page natively — the echo + flush shape runs through its worker, and
     * the §10 wire contract must hold on the bytes it serves. Skipped
     * unless LINTEL_E2E_FRANKENPHP=1 AND the binary is on PATH
     * (CI: the adapters-live job downloads the v1.14.0 static build). */
    public function test_franken_php_adapter_end_to_end_over_the_real_binary(): void
    {
        if (getenv('LINTEL_E2E_FRANKENPHP') !== '1' || trim((string) shell_exec('command -v frankenphp 2>/dev/null')) === '') {
            self::markTestSkipped('the live FrankenPHP lane needs the frankenphp binary + LINTEL_E2E_FRANKENPHP=1 '
                .'(CI: the adapters-live job downloads the static v1.14.0 build)');
        }
        $label = 'FrankenPHP binary adapter';
        $docroot = sys_get_temp_dir().'/lintel-frankenphp-'.bin2hex((string) random_bytes(4));
        mkdir($docroot, 0777, true);
        $worker = str_replace(['{AUTOLOAD}', '{LABEL}'], [
            var_export(\dirname(__DIR__).'/vendor/autoload.php', true),
            $label,
        ], <<<'PHP'
<?php declare(strict_types=1);
require {AUTOLOAD};

use Lintel\Suspense;

use function Lintel\c;
use function Lintel\stream_via;

$payload = '<script>alert("pwned")</script>';
$page = (static function () use ($payload): \Generator {
    yield '<!doctype html><html><body><h1>{LABEL}</h1>';
    yield new Suspense('loading…', static function () use ($payload): \Generator {
        \usleep(10_000); // the slow boundary
        yield '<p>user: ' . c($payload) . '</p>';
    });
    yield '<p>footer</p></body></html>';
})();
stream_via($page, new \Lintel\FrankenPHPFlushTransport());
PHP);
        file_put_contents($docroot.'/index.php', $worker);

        $proc = null;
        $body = false;
        $headers = [];
        try {
            for ($attempt = 0; $attempt < 10; $attempt++) {
                $port = 23300 + $attempt;
                $proc = @proc_open(
                    ['frankenphp', 'php-server', '--root', $docroot, '--listen', '127.0.0.1:'.(string) $port],
                    [1 => ['file', $docroot.'/fp.log', 'w'], 2 => ['file', $docroot.'/fp.err', 'w']],
                    $pipes,
                );
                if (! is_resource($proc)) {
                    continue;
                }
                $deadline = microtime(true) + 8;
                while (microtime(true) < $deadline) {
                    \usleep(300_000);
                    $context = stream_context_create(['http' => ['ignore_errors' => true, 'timeout' => 5]]);
                    $fetched = @file_get_contents('http://127.0.0.1:'.$port.'/index.php', false, $context);
                    if ($fetched !== false && str_contains($fetched, '<h1>'.$label.'</h1>')) {
                        $body = $fetched;
                        $headers = http_get_last_response_headers() ?? [];
                        break;
                    }
                }
                if ($body !== false) {
                    break;
                }
                proc_terminate($proc);
                proc_close($proc);
                $proc = null;
            }
            self::assertNotFalse($body, 'the real frankenphp server never served the streamed page');
            self::assertStreamedPageResponse((string) $body, $headers, $label);
        } finally {
            if (is_resource($proc)) {
                proc_terminate($proc);
                proc_close($proc);
            }
            foreach (['/index.php', '/fp.log', '/fp.err'] as $file) {
                @unlink($docroot.$file);
            }
            @rmdir($docroot);
        }
    }

    /** The M2 live Swoole lane: a real Swoole\Http\Server (the extension,
     * booted in a subprocess) serves the streamed page through
     * SwooleFlushTransport::fromSwoole — status, headers
     * (X-Accel-Buffering: no included), per-chunk writes and end(). Skipped
     * when the extension is absent (CI: the adapters-live job installs it
     * via setup-php). */
    public function test_swoole_adapter_end_to_end_over_the_extension_server(): void
    {
        if (! SwooleFlushTransport::isAvailable()) {
            self::markTestSkipped('the live Swoole lane needs ext-swoole '
                .'(CI: the adapters-live job installs it via setup-php)');
        }
        $label = 'Swoole adapter';
        $docroot = sys_get_temp_dir().'/lintel-swoole-'.bin2hex((string) random_bytes(4));
        mkdir($docroot, 0777, true);
        $worker = str_replace(['{AUTOLOAD}', '{LABEL}'], [
            var_export(\dirname(__DIR__).'/vendor/autoload.php', true),
            $label,
        ], <<<'PHP'
<?php declare(strict_types=1);
require {AUTOLOAD};

$http = new \Swoole\Http\Server('127.0.0.1', {PORT});
$http->on('request', static function ($request, $response) {
    $payload = '<script>alert("pwned")</script>';
    $page = (static function () use ($payload): \Generator {
        yield '<!doctype html><html><body><h1>{LABEL}</h1>';
        yield new \Lintel\Suspense('loading…', static function () use ($payload): \Generator {
            \usleep(10_000); // the slow boundary
            yield '<p>user: ' . \Lintel\c($payload) . '</p>';
        });
        yield '<p>footer</p></body></html>';
    })();
    \Lintel\stream_via($page, \Lintel\SwooleFlushTransport::fromSwoole($response));
});
$http->start();
PHP);

        $proc = null;
        $body = false;
        $headers = [];
        try {
            for ($attempt = 0; $attempt < 10; $attempt++) {
                $port = 23400 + $attempt;
                $path = $docroot.'/worker.php';
                file_put_contents($path, str_replace('{PORT}', (string) $port, $worker));
                $proc = @proc_open([PHP_BINARY, $path], [
                    1 => ['file', $docroot.'/sw.log', 'w'],
                    2 => ['file', $docroot.'/sw.err', 'w'],
                ], $pipes);
                if (! is_resource($proc)) {
                    continue;
                }
                $deadline = microtime(true) + 8;
                while (microtime(true) < $deadline) {
                    \usleep(300_000);
                    $context = stream_context_create(['http' => ['ignore_errors' => true, 'timeout' => 5]]);
                    $fetched = @file_get_contents('http://127.0.0.1:'.$port.'/', false, $context);
                    if ($fetched !== false && str_contains($fetched, '<h1>'.$label.'</h1>')) {
                        $body = $fetched;
                        $headers = http_get_last_response_headers() ?? [];
                        break;
                    }
                }
                if ($body !== false) {
                    break;
                }
                proc_terminate($proc);
                proc_close($proc);
                $proc = null;
            }
            self::assertNotFalse($body, 'the ext-swoole server never served the streamed page');
            self::assertStreamedPageResponse((string) $body, $headers, $label);
        } finally {
            if (is_resource($proc)) {
                proc_terminate($proc);
                proc_close($proc);
            }
            foreach (['/worker.php', '/sw.log', '/sw.err'] as $file) {
                @unlink($docroot.$file);
            }
            @rmdir($docroot);
        }
    }

    /** @return resource|null */
    private function startPhpServer(string $docroot, int $port)
    {
        $proc = @proc_open(
            [PHP_BINARY, '-S', '127.0.0.1:'.$port, '-t', $docroot],
            [1 => ['pipe', 'w'], 2 => ['pipe', 'w']],
            $pipes,
        );
        if ($proc === false) {
            return null;
        }
        foreach ($pipes as $pipe) {
            fclose($pipe);
        }

        return $proc;
    }

    /** @return resource|null */
    private function startRRServer(string $docroot, int $port, string $workerPath)
    {
        $config = <<<YAML
version: "3"
server:
  command: "{PHP} {WORKER}"
  env:
    RR_MODE: http
  pool:
    num_workers: 4
http:
  address: 127.0.0.1:{$port}
YAML;
        $config = str_replace(['{PHP}', '{WORKER}'], [PHP_BINARY, $workerPath], $config);
        $cfgPath = $docroot.'/.rr.yaml';
        file_put_contents($cfgPath, $config);
        // rr's own lifecycle logs go to files: a closed pipe on stdout/stderr
        // destabilizes the server (it writes its startup banner eagerly).
        $proc = @proc_open(['rr', 'serve', '-c', $cfgPath], [
            1 => ['file', $docroot.'/rr.log', 'w'],
            2 => ['file', $docroot.'/rr.err', 'w'],
        ], $pipes);

        return $proc === false ? null : $proc;
    }
}
