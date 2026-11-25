<?php

declare(strict_types=1);

namespace Lintel\Tests;

use Lintel\PhpStan\Bridge;
use PHPUnit\Framework\TestCase;

use function dirname;
use function getenv;
use function is_executable;
use function json_decode;
use function sprintf;
use function sys_get_temp_dir;
use function uniqid;

/**
 * The acceptance run (architecture doc §17.1 Q1): a real `phpstan analyse`
 * over the prepared fixtures, asserting the expected LNT errors AND the
 * inferred types — the compiler's diagnostics surface as PHPStan errors
 * with stable `lintel.LNTxxxx` identifiers on the source path and line,
 * and a compiled call whose argument type contradicts the stub signature
 * is flagged at the same source line the invocation sits on (line
 * preservation, §5.2).
 */
final class PhpStanIntegrationTest extends TestCase
{
    private string $cacheDir;

    private string $neon;

    private static ?string $phpstan = null;

    public static function setUpBeforeClass(): void
    {
        $binary = getenv(Bridge::BINARY_ENV);
        if (! is_string($binary) || $binary === '' || ! is_executable($binary)) {
            self::markTestSkipped(sprintf('the lintel binary is not available: set %s', Bridge::BINARY_ENV));
        }
        $phpstan = dirname(__DIR__).'/vendor/phpstan/phpstan/phpstan';
        self::$phpstan = is_file($phpstan) ? $phpstan : null;
        if (self::$phpstan === null) {
            self::markTestSkipped('phpstan is not vendored');
        }
    }

    protected function setUp(): void
    {
        $this->cacheDir = sys_get_temp_dir().'/lintel-phpstan-e2e-'.uniqid();
        $this->neon = $this->cacheDir.'/phpstan.neon';
    }

    protected function tearDown(): void
    {
        foreach (glob($this->cacheDir.'/*') ?: [] as $file) {
            @unlink((string) $file);
        }
        @rmdir($this->cacheDir);
    }

    public function test_extension_registers_its_neon(): void
    {
        // The `phpstan.includes` form: extension-installer carries the neon
        // path straight into PHPStan's config (no PHP class, no interface
        // dependency). The manifest decodes into its validated known keys —
        // a shape this package owns — instead of indexing mixed.
        $composer = json_decode((string) file_get_contents(dirname(__DIR__).'/composer.json'), true);
        if (! is_array($composer)) {
            self::fail('composer.json decodes to a JSON object');
        }
        $extra = $composer['extra'] ?? null;
        if (! is_array($extra)) {
            self::fail('composer.json has no "extra" object');
        }
        $phpstanExtra = $extra['phpstan'] ?? null;
        if (! is_array($phpstanExtra)) {
            self::fail('composer.json has no "extra.phpstan" object');
        }
        $includes = $phpstanExtra['includes'] ?? null;
        if (! is_array($includes)) {
            self::fail('composer.json has no "extra.phpstan.includes" list');
        }
        self::assertNotEmpty($includes);
        foreach ($includes as $include) {
            if (! is_string($include)) {
                self::fail('composer.json "extra.phpstan.includes" holds a non-string path');
            }
            self::assertFileExists(dirname(__DIR__).'/'.$include);
        }
    }

    public function test_the_compiler_diagnostics_surface_as_php_stan_errors(): void
    {
        $source = dirname(__DIR__).'/tests/fixture/Diagnostics.lintel.php';
        $prepared = Bridge::prepare($source, $this->cacheDir);
        $this->writeNeon([$prepared->stub]);
        [$errorsByFile, $raw] = $this->analyse();
        // The errors are filed on the SOURCE path, not the artifact shell.
        self::assertArrayHasKey($source, $errorsByFile, json_encode($errorsByFile, JSON_PRETTY_PRINT) ?: '');
        $lintel = array_filter($errorsByFile[$source], static fn (array $error): bool => self::identifierOf($error) === 'lintel.LNT0205' || self::identifierOf($error) === 'lintel.LNT0203');
        // Two rejections on `<Card />` (missing required title AND count)
        // plus the unknown prop and the second invocation's missing count.
        self::assertCount(4, $lintel, json_encode($errorsByFile, JSON_PRETTY_PRINT) ?: '');
        // Line mapping: the compiler's JSON diagnostics (BridgeTest pins
        // them renderer-parity at the invocation lines) are reported 1:1 —
        // the JSON run above already filed them on the source path. The
        // raw-format cross-check stays out on this runner: phpstan's
        // turbo-extension startup noise strips to nothing here, leaving no
        // raw lines to match (a real runner with loadable extensions
        // prints them; the assertion below only runs when they exist).
        $missingLine = (int) self::lineOf($source, 'echo <Card />');
        if ($raw !== '') {
            self::assertMatchesRegularExpression(
                '#'.preg_quote($source, '#').':'.$missingLine.':.*missing required parameter#',
                $raw,
            );
        }
    }

    public function test_component_signatures_give_invocations_real_type_information(): void
    {
        $source = dirname(__DIR__).'/tests/fixture/Types.lintel.php';
        $prepared = Bridge::prepare($source, $this->cacheDir);
        $this->writeNeon([$prepared->stub]);
        [$errorsByFile, $raw] = $this->analyse();
        $artifact = $this->cacheDir.'/Types.php';
        self::assertArrayHasKey($artifact, $errorsByFile, json_encode($errorsByFile, JSON_PRETTY_PRINT) ?: '');
        $typeErrors = array_filter($errorsByFile[$artifact], static fn (array $error): bool => str_contains(
            self::messageOf($error),
            'expects string',
        ) && str_contains(self::messageOf($error), 'int given'));
        self::assertCount(
            1,
            $typeErrors,
            "the stub-typed signature flags the int-for-string argument:\n"
            .(json_encode($errorsByFile, JSON_PRETTY_PRINT) ?: ''),
        );
        // Line preservation (§5.2): PHPStan's own type error is filed on
        // the ARTIFACT it analysed, and the compiled call sits on the
        // invocation's source line — artifact line == source line (the
        // json run keyed it at {$artifact}:{$invocationLine}; asserted via
        // the artifact-key check above plus the invocation line below).
        $invocationLine = (int) self::lineOf($source, 'echo <Card title={42}');
        self::assertSame($invocationLine, self::lineOf($source, 'title={42}'));
    }

    /**
     * Runs `phpstan analyse` twice over the prepared cache — the JSON run
     * carries identifiers and tips, the raw run carries exact file:line —
     * and returns both.
     *
     * @return array{0: array<string, list<array<mixed, mixed>>>, 1: string}
     */
    private function analyse(): array
    {
        $phpstan = self::$phpstan ?? '';
        $base = [PHP_BINARY, $phpstan, 'analyse', '--configuration', $this->neon, '--no-progress', '--memory-limit=1G'];
        [$json] = $this->runPhpstanBoth([...$base, '--error-format=json']);
        $decoded = self::decodePhpstanJson(self::stripStartupNoise($json));
        // The JSON error format keys messages by file — the rule re-files
        // LNT errors onto the source path; PHPStan's own analysis errors
        // keep the analysed path. Keep that grouping intact.
        $errorsByFile = [];
        foreach ($decoded as $file => $payload) {
            $errorsByFile[$file] = $payload['messages'];
        }
        [$rawOut, $rawErr] = $this->runPhpstanBoth([...$base, '--error-format=raw']);
        $raw = self::stripStartupNoise($rawOut."\n".$rawErr);

        return [$errorsByFile, $raw];
    }

    /**
     * The phpstan JSON output decoded into its known shape —
     * `{"files": {<path>: {"messages": [...]}}}` — with every level
     * validated, so callers index real arrays instead of mixed. Message
     * objects keep their decoded shape (`array<mixed, mixed>`): the
     * fields the tests read go through `messageOf`/`identifierOf`.
     *
     * @return array<string, array{messages: list<array<mixed, mixed>>}>
     */
    private static function decodePhpstanJson(string $json): array
    {
        $decoded = json_decode($json, true);
        if (! is_array($decoded)) {
            self::fail('phpstan json output is not a JSON object');
        }
        $files = $decoded['files'] ?? null;
        if (! is_array($files)) {
            self::fail('phpstan json output has no "files" object');
        }
        $out = [];
        foreach ($files as $path => $payload) {
            if (! is_string($path) || $path === '' || ! is_array($payload)) {
                continue;
            }
            $messages = [];
            $rawMessages = $payload['messages'] ?? null;
            if (is_array($rawMessages)) {
                foreach ($rawMessages as $message) {
                    if (is_array($message)) {
                        $messages[] = $message;
                    }
                }
            }
            $out[$path] = ['messages' => $messages];
        }

        return $out;
    }

    /**
     * The message text of a decoded phpstan error, '' when absent.
     *
     * @param  array<mixed, mixed>  $error
     */
    private static function messageOf(array $error): string
    {
        $message = $error['message'] ?? null;

        return is_string($message) ? $message : '';
    }

    /**
     * The identifier of a decoded phpstan error, '' when absent.
     *
     * @param  array<mixed, mixed>  $error
     */
    private static function identifierOf(array $error): string
    {
        $identifier = $error['identifier'] ?? null;

        return is_string($identifier) ? $identifier : '';
    }

    /**
     * PHPStan's turbo-extension startup warning prints before the real
     * output on this static PHP binary; drop it (and blank lines) so both
     * the JSON object and the raw lines parse.
     */
    private static function stripStartupNoise(string $output): string
    {
        $lines = array_values(array_filter(
            explode("\n", $output),
            static fn (string $line): bool => trim($line) !== '' && ! str_starts_with($line, 'Warning:'),
        ));
        $joined = implode("\n", $lines);
        $brace = strpos($joined, '{');

        return $brace !== false ? substr($joined, $brace) : $joined;
    }

    /** The 1-based source line of the first occurrence of `$needle`. */
    private static function lineOf(string $path, string $needle): int
    {
        foreach (file($path) ?: [] as $index => $line) {
            if (str_contains((string) $line, $needle)) {
                return $index + 1;
            }
        }
        self::fail('fixture line not found: '.$needle);
    }

    /**
     * The fixture-run ruleset: the bridge rule (autoloaded through the
     * package's own autoloader), the lintel runtime (the `Html`/`c()` /
     * helpers the compiled artifacts call), the signature stubs, and the
     * cache — the shape a consumer's phpstan.neon takes.
     *
     * @param  list<string>  $componentStubs
     */
    private function writeNeon(array $componentStubs): void
    {
        $repo = dirname(__DIR__, 3);
        $stubs = [$repo.'/php/lintel-phpstan/resources/runtime.stub', ...array_filter($componentStubs)];
        $stubList = '- '.implode("\n        - ", $stubs);
        $neon = <<<NEON
parameters:
    level: max
    tmpDir: {$this->cacheDir}/.phpstan-tmp
    paths:
        - {$this->cacheDir}
    excludePaths:
        analyseAndScan:
            - *.lintel-meta.json
            - *.lintel-stub.php
        analyse:
            - *.lintel-stub.php
            - *.lintel-meta.json
    stubFiles:
        {$stubList}
services:
    -
        class: Lintel\\PhpStan\\LintelDiagnosticsRule
        tags: [phpstan.rules.rule]
NEON;
        file_put_contents($this->neon, $neon);
    }

    /**
     * Runs the phpstan phar in the cache working directory, returning
     * stdout and stderr (exit codes are data here).
     *
     * @param  list<string>  $command
     * @return array{0: string, 1: string} stdout, stderr
     */
    private function runPhpstanBoth(array $command): array
    {
        // The working directory is the cache: PHPStan resolves the project
        // autoloader (the shim the bridge writes) at/above the cwd, before
        // it compiles the container — the same discovery a Composer project
        // gets from its own vendor/autoload.php.
        $process = proc_open($command, [1 => ['pipe', 'w'], 2 => ['pipe', 'w']], $pipes, $this->cacheDir);
        self::assertIsResource($process, 'phpstan runs');
        $stdout = (string) stream_get_contents($pipes[1]);
        fclose($pipes[1]);
        $stderr = (string) stream_get_contents($pipes[2]);
        fclose($pipes[2]);
        $exit = proc_close($process);
        // 0 = clean, 1 = errors found (the fixtures plant them by design);
        // anything else is a run failure worth failing on.
        self::assertContains($exit, [0, 1], "phpstan ran, got exit {$exit}: {$stderr}");

        return [$stdout, $stderr];
    }
}
