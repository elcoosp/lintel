<?php

declare(strict_types=1);

namespace Lintel\Tests;

use Lintel\PhpStan\Bridge;
use PHPUnit\Framework\TestCase;

use function count;
use function dirname;
use function explode;
use function file;
use function file_get_contents;
use function getenv;
use function is_executable;
use function json_decode;
use function json_encode;
use function mkdir;
use function proc_close;
use function proc_open;
use function sprintf;
use function str_replace;
use function stream_get_contents;
use function strstr;
use function trim;
use function uniqid;

use const JSON_PRETTY_PRINT;

/**
 * The §7.3-Y2 acceptance (architecture doc §7.3-Y2, `docs/type-elision.md`),
 * driven end to end: the analyze runner (`bin/analyze`) prepares the
 * fixture through the real compiler binary, runs the PHPStan type pass
 * (`LintelTypesCollector`), writes the types report, and — with `--compile`
 * — runs the compiler's `lintel analyze` lane. Every elision rule ships (a)
 * a golden of the emitted PHP and (b) a threat-corpus case proving the
 * elision cannot reintroduce injection: the compiled artifact EXECUTES
 * under PHP with attacker-shaped payloads, and nothing raw reaches the
 * output.
 */
final class AnalyzeElisionTest extends TestCase
{
    private const FIXTURE = __DIR__ . '/fixture/Elision.lintel.php';

    /** The runner's compiled PHP (its `--compile` stdout). */
    private string $compiled;

    /** @var list<array<mixed, mixed>> the runner's types report, decoded */
    private array $report = [];

    public static function setUpBeforeClass(): void
    {
        $binary = getenv(Bridge::BINARY_ENV);
        if (! is_string($binary) || $binary === '' || ! is_executable($binary)) {
            self::markTestSkipped(sprintf('the lintel binary is not available: set %s', Bridge::BINARY_ENV));
        }
        if (! is_file(dirname(__DIR__) . '/vendor/phpstan/phpstan/phpstan')) {
            self::markTestSkipped('phpstan is not vendored');
        }
    }

    protected function setUp(): void
    {
        // The full path, once per test: runner → report → compiled PHP.
        // The report stays in the cache (`--out`) — never a fixture
        // sidecar in the tree.
        $cache = sys_get_temp_dir() . '/lintel-elision-' . uniqid();
        [$stdout, $stderr, $exit] = self::runCommand([
            PHP_BINARY,
            dirname(__DIR__) . '/bin/analyze',
            self::FIXTURE,
            '--cache=' . $cache,
            '--out=' . $cache . '/Elision.lintel-types.json',
            '--compile',
        ]);
        self::assertSame(0, $exit, "the analyze runner ran: {$stdout}{$stderr}");
        // The runner prints the report path as its first line, then the
        // per-type entries, then the compiled PHP.
        $reportLine = strstr($stdout, "\n", true) ?: '';
        self::assertStringStartsWith('analyze report: ', $reportLine, $stdout);
        // The line is `<path> (N child types, M elidable)` — the path ends
        // at the summary separator (paths may contain spaces).
        $withSummary = (string) substr($reportLine, 16);
        $reportPath = trim((string) (strstr($withSummary, ' (', true) ?: $withSummary));
        $decoded = json_decode((string) file_get_contents($reportPath), true);
        if (! is_array($decoded)) {
            self::fail('the report decodes to a JSON object');
        }
        self::assertSame(1, $decoded['version'] ?? null, 'the report carries version 1');
        $children = $decoded['children'] ?? null;
        if (! is_array($children)) {
            self::fail('the report carries a `children` list');
        }
        $report = [];
        foreach ($children as $entry) {
            if (is_array($entry)) {
                $report[] = $entry;
            }
        }
        $this->report = $report;
        $this->compiled = (string) strstr($stdout, '<?php');
    }

    public function testTheReportCarriesTheProvenTypesPerChild(): void
    {
        $types = [];
        foreach ($this->report as $entry) {
            $expr = $entry['expr'] ?? null;
            $type = $entry['type'] ?? null;
            if (! is_string($expr) || ! is_string($type)) {
                continue;
            }
            $types[$expr] = $type;
        }
        // The allowlist proofs: the source's typed `int`/`float` params,
        // the is_numeric guard's numeric-string, and the constant's literal.
        self::assertSame('int', $types['$count'] ?? 'missing', json_encode($this->report, JSON_PRETTY_PRINT) ?: '');
        self::assertSame('float', $types['$price'] ?? 'missing', json_encode($this->report, JSON_PRETTY_PRINT) ?: '');
        self::assertSame('numeric-string', $types['$code'] ?? 'missing', json_encode($this->report, JSON_PRETTY_PRINT) ?: '');
        self::assertSame('literal', $types['MODE'] ?? 'missing', json_encode($this->report, JSON_PRETTY_PRINT) ?: '');
        // The conservative verdicts the report still carries: an unproven
        // child and a bool (reported, never elidable — c() renders bools
        // as '' while raw interpolation renders '1').
        self::assertSame('mixed', $types['$label'] ?? 'missing', 'unproven children report');
        self::assertSame('mixed', $types['$unit'] ?? 'missing', 'unproven children report');
        self::assertSame('bool', $types['$on'] ?? 'missing', 'bool reports as non-elidable');
    }

    /** (a) The emitted-PHP goldens: elided only where proven. */
    public function testTheCompiledOutputElidesExactlyTheProvenChildren(): void
    {
        // int/float/numeric-string fold to the raw expression, the literal
        // to its escaped value; mixed/bool keep `\Lintel\c()`.
        self::assertStringContainsString(
            "new \\Lintel\\Html('<p>' . \$count . ' ' . \\Lintel\\c(\$label) . '</p>')",
            $this->compiled,
            $this->compiled,
        );
        self::assertStringContainsString(
            "new \\Lintel\\Html('<span>' . \$price . ' ' . \\Lintel\\c(\$unit) . '</span>')",
            $this->compiled,
            $this->compiled,
        );
        self::assertStringContainsString(
            "new \\Lintel\\Html('<td>' . \$code . '</td>')",
            $this->compiled,
            $this->compiled,
        );
        self::assertStringContainsString(
            "new \\Lintel\\Html('<b>AT&amp;T</b>')",
            $this->compiled,
            $this->compiled,
        );
        self::assertStringContainsString('\\Lintel\\c($on)', $this->compiled, $this->compiled);
        // Line preservation holds over the whole artifact.
        self::assertSame(
            count(file(self::FIXTURE) ?: []),
            count(explode("\n", rtrim($this->compiled, "\n"))),
            'the analyze compile preserves lines',
        );
    }

    /**
     * (b) The threat corpus: the compiled artifact executes under PHP with
     * attacker-shaped payloads — the elided children render markup-free
     * (proven scalars cannot carry markup) and every kept helper escapes.
     */
    public function testTheCompiledArtifactCannotReintroduceInjection(): void
    {
        $dir = sys_get_temp_dir() . '/lintel-elision-' . uniqid();
        @mkdir($dir);
        $artifact = $dir . '/Elision.php';
        file_put_contents($artifact, $this->compiled);
        $runtime = dirname(__DIR__, 2) . '/runtime/src/functions.php';
        $harness = <<<'PHP'
<?php
declare(strict_types=1);
require '__RUNTIME__';
require '__ARTIFACT__';
$renders = [
    'counts' => \Lintel\Fixtures\Counts(42, '<script>alert(1)</script>'),
    'price' => \Lintel\Fixtures\Price(1.5, '" onmouseover="alert(1)"'),
    'numeric_attacker' => \Lintel\Fixtures\Numeric('<img src=x onerror=alert(1)>'),
    'numeric_user_input' => \Lintel\Fixtures\Numeric($argv[1]),
    'literal' => \Lintel\Fixtures\Literal(),
    'flags' => \Lintel\Fixtures\Flags(true),
    'flags_false' => \Lintel\Fixtures\Flags(false),
];
echo json_encode($renders), "\n";
echo 'c(true)=', \Lintel\c(true), "\n";
echo "c('42')=", \Lintel\c('42'), "\n";
PHP;
        $harness = str_replace(['__RUNTIME__', '__ARTIFACT__'], [$runtime, $artifact], $harness);
        $harnessPath = $dir . '/harness.php';
        file_put_contents($harnessPath, $harness);
        [$stdout] = self::runCommand([PHP_BINARY, $harnessPath, '42']);
        $renders = json_decode(trim(strstr($stdout, "\n", true) ?: ''), true);
        self::assertIsArray($renders, "the harness rendered: {$stdout}");
        // Each component returns `\Lintel\Html` — json_encode carries its
        // public `s` property; unwrap it.
        $rendered = static fn (mixed $item): string => self::rendered($item);
        // int child + attacker string sibling: digits render raw (they
        // cannot carry markup), the string stays escaped.
        self::assertSame(
            '<p>42 &lt;script&gt;alert(1)&lt;/script&gt;</p>',
            $rendered($renders['counts']),
        );
        // float + attacker attribute-breakout sibling: PHP's float form
        // renders, the quote breakout escapes to &quot;.
        self::assertSame(
            '<span>1.5 &quot; onmouseover=&quot;alert(1)&quot;</span>',
            $rendered($renders['price']),
        );
        // numeric-string guard: the markup payload fails is_numeric and
        // takes the rejected branch — the elided child never sees it.
        self::assertSame('<p>bad</p>', $rendered($renders['numeric_attacker']));
        // ... and the payload that IS numeric renders digits only — a
        // numeric-string cannot carry markup, that is the proof.
        self::assertSame('<td>42</td>', $rendered($renders['numeric_user_input']));
        // literal: the constant is escaped at compile time and cannot be
        // attacker-controlled at runtime.
        self::assertSame('<b>AT&amp;T</b>', $rendered($renders['literal']));
        // bool stays wrapped: c(true) is '' — a raw '1' interpolation
        // would change the output, so the helper keeps it (render parity).
        self::assertSame('<i></i>', $rendered($renders['flags']));
        self::assertSame('<i></i>', $rendered($renders['flags_false']));
        // Parity evidence: what c() renders for the same values the
        // elision's runtime forms produce.
        self::assertStringContainsString('c(true)=' . \PHP_EOL, $stdout, 'c(true) is empty');
        self::assertStringContainsString("c('42')=42", $stdout, 'c(numeric-string) is the identity');
    }

    /** The rendered markup of one decoded `\Lintel\Html` JSON object. */
    private static function rendered(mixed $item): string
    {
        if (! is_array($item)) {
            self::fail('the harness rendered a non-object shape');
        }
        $s = $item['s'] ?? null;
        return is_string($s) ? $s : '';
    }

    /**
     * @param list<string> $command
     * @return array{0: string, 1: string, 2: int} stdout, stderr, exit
     */
    private static function runCommand(array $command): array
    {
        $process = proc_open($command, [1 => ['pipe', 'w'], 2 => ['pipe', 'w']], $pipes);
        self::assertIsResource($process, 'the process runs');
        $stdout = (string) stream_get_contents($pipes[1]);
        fclose($pipes[1]);
        $stderr = (string) stream_get_contents($pipes[2]);
        fclose($pipes[2]);
        $exit = proc_close($process);
        return [$stdout, $stderr, $exit];
    }
}
