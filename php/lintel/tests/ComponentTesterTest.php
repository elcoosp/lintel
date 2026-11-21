<?php

declare(strict_types=1);

namespace Lintel\Tests;

use Lintel\Test\ComponentTester;
use PHPUnit\Framework\ExpectationFailedException;
use PHPUnit\Framework\TestCase;

use function dirname;
use function getenv;
use function is_executable;
use function sprintf;

/**
 * The §17.1-Q3 testing-utilities acceptance: the `ComponentTester` renders
 * a REAL component — compiled on the fly by the real `lintel` binary, the
 * same compile path the §6.10 dev loader drives — and asserts against its
 * output through the §6.8 runtime's `\Lintel\c()`. The hostile-payload
 * cases are the point of the helper: a payload that reaches the markup
 * through a test prop must surface escaped, proving the compiled
 * component (not the harness) owns the escaping contract.
 */
final class ComponentTesterTest extends TestCase
{

    public static function setUpBeforeClass(): void
    {
        // The real compiler binary, the same lane the bridge suites use
        // (`Bridge::BINARY_ENV`): CI's php job builds it and exports
        // LINTEL_TEST_BIN before the suites run.
        $binary = getenv('LINTEL_TEST_BIN');
        if (! is_string($binary) || $binary === '' || ! is_executable($binary)) {
            self::markTestSkipped(sprintf(
                'the lintel binary is not available: set %s (CI builds it; locally: cargo build -p lintel-cli)',
                'LINTEL_TEST_BIN',
            ));
        }
    }

    /**
     * One suite-wide tester: the PHP process compiles each unique source
     * once (the compiled functions persist for the whole run — the same
     * constraint a real test process has, and the helper's documented
     * contract), so the tester instance is shared across tests.
     */
    private static ?ComponentTester $tester = null;

    private static function tester(): ComponentTester
    {
        $binary = getenv('LINTEL_TEST_BIN');
        self::$tester ??= new ComponentTester(is_string($binary) && $binary !== '' ? $binary : null);

        return self::$tester;
    }

    public function testRenderCompilesAndRendersARealComponent(): void
    {
        $html = $this->tester()->render(
            'function Badge(string $name): Html { return <b class="badge">{$name}</b>; }',
            ['name' => 'Ada'],
        );
        self::assertSame('<b class="badge">Ada</b>', $html);
    }

    public function testPropsBindAsNamedArgumentsAcrossTypes(): void
    {
        $html = $this->tester()->render(
            'function Page(string $title, int $page, bool $dense = false): Html'
            . ' { return <p data-page={$page}>{$title}</p>; }',
            ['title' => 'Overview', 'page' => 3],
        );
        self::assertSame('<p data-page="3">Overview</p>', $html);
    }

    public function testHostileChildPayloadSurfacesEscaped(): void
    {
        // The attacker controls a CHILD prop: the compiled component must
        // escape it — the helper proves the escaping, never bypasses it.
        $html = $this->tester()->render(
            'function HostileBadge(string $name): Html { return <b>{$name}</b>; }',
            ['name' => '<script>alert(1)</script>'],
        );
        self::assertSame(
            '<b>&lt;script&gt;alert(1)&lt;/script&gt;</b>',
            $html,
        );
    }

    public function testHostileAttributeAndUrlPayloadsStayContained(): void
    {
        // Quote-breakout in an attribute value and a javascript: URL: the
        // §4.4 attribute routing must hold inside the test harness too.
        $html = $this->tester()->render(
            'function SafeLink(string $href, string $label): Html'
            . ' { return <a href={$href}>{$label}</a>; }',
            ['href' => 'javascript:alert(1)', 'label' => '" onmouseover="alert(1)"'],
        );
        self::assertSame(
            '<a href="about:invalid#blocked">&quot; onmouseover=&quot;alert(1)&quot;</a>',
            $html,
        );
    }

    public function testAssertRendersPassesAndFailsLoudly(): void
    {
        $tester = $this->tester();
        $source = 'function Chip(string $t): Html { return <em>{$t}</em>; }';
        $tester->assertRenders('<em>ok</em>', $source, ['t' => 'ok']);
        try {
            $tester->assertRenders('<em>other</em>', $source, ['t' => 'ok']);
            self::fail('a wrong expectation must fail the assertion');
        } catch (ExpectationFailedException $e) {
            self::assertStringContainsString('unexpected markup', (string) $e->getMessage());
        }
    }

    public function testASecondRenderOfTheSameSourceReusesTheCompiledComponent(): void
    {
        $tester = $this->tester();
        $source = 'function Counter(int $n): Html { return <span>{$n}</span>; }';
        // A second render of the SAME source must not re-require the
        // artifact (PHP cannot redeclare functions) — both calls render.
        self::assertSame('<span>7</span>', $tester->render($source, ['n' => 7]));
        self::assertSame('<span>8</span>', $tester->render($source, ['n' => 8]));
    }

    public function testTwoDistinctSourcesMayNotShadowOneComponentName(): void
    {
        $tester = $this->tester();
        $first = 'function Panel(string $body): Html { return <div>{$body}</div>; }';
        $shadow = 'function Panel(string $body): Html { return <aside>{$body}</aside>; }';
        self::assertSame('<div>one</div>', $tester->render($first, ['body' => 'one']));
        try {
            $tester->render($shadow, ['body' => 'two']);
            self::fail('a shadowed component name must be refused before the redeclare fatal');
        } catch (\RuntimeException $e) {
            self::assertStringContainsString('already declared in this PHP process', (string) $e->getMessage());
        }
        // The first component still renders through its original body.
        self::assertSame('<div>two</div>', $tester->render($first, ['body' => 'two']));
    }

    public function testACompileFailureSurfacesTheCompilerDiagnostics(): void
    {
        try {
            // An unclosed element: a parse error, not a runtime failure.
            $this->tester()->render('function Broken(): Html { return <div>oops; }');
            self::fail('a parse error must throw');
        } catch (\RuntimeException $e) {
            $message = (string) $e->getMessage();
            self::assertStringContainsString('failed to compile', $message);
            self::assertStringContainsString('LNT', $message, 'the compiler diagnostics ride along');
        }
    }

    public function testANamespacedComponentResolvesThroughItsNamespace(): void
    {
        $html = $this->tester()->render(
            'namespace Lintel\\Tester\\Fixtures;'
            . ' use Lintel\\Html;'
            . ' function Tile(string $label): Html { return <i>{$label}</i>; }',
            ['label' => 'ns'],
        );
        self::assertSame('<i>ns</i>', $html);
    }

    public function testASourceWithoutAComponentIsRefused(): void
    {
        try {
            // A lowercase helper is not a component (§4.7) — the helper
            // points at the expected shape instead of compiling silently.
            $this->tester()->render('function helper(string $x): string { return $x; }');
            self::fail('a source without a component must be refused');
        } catch (\RuntimeException $e) {
            self::assertStringContainsString('declares no component', (string) $e->getMessage());
        }
    }

    public function testAssertRendersContainingPinsOneFragment(): void
    {
        $this->tester()->assertRendersContaining(
            '&lt;img src=x&gt;',
            'function Note(string $body): Html { return <p>{$body}</p>; }',
            ['body' => '<img src=x>'],
        );
    }

    public function testTheDefaultBinaryResolvesThroughTheLoaderOrder(): void
    {
        // Constructor-less construction uses the §6.10 resolution order
        // (LINTEL_BIN → composer cache → PATH); here it resolves the
        // explicitly exported test binary — the same discovery a consumer
        // project gets without configuration.
        $binary = getenv('LINTEL_TEST_BIN');
        $binary = is_string($binary) ? $binary : '';
        self::assertNotSame('', $binary, 'the class setUp guarantees the binary is exported');
        putenv('LINTEL_BIN=' . $binary);
        try {
            $tester = new ComponentTester();
            self::assertSame(
                '<u>fallback</u>',
                $tester->render('function Mark(): Html { return <u>fallback</u>; }'),
            );
        } finally {
            putenv('LINTEL_BIN');
        }
    }

    /**
     * The Phase 9 M3 acceptance: `renderStreaming()` compiles through the
     * §10 stream lowering and consumes the generator via `\Lintel\stream()`
     * — the fully consumed stream output must equal the string target's
     * byte for byte (the §10 sync-compat contract), hostile payload and
     * nested markup included. One PHP process declares a component name
     * once (and under one target — the stream artifact declares the same
     * function name as the string artifact), so the parity fixture is the
     * same component body under two names.
     */
    public function testRenderStreamingMatchesStringRender(): void
    {
        $body = 'function %s(string $label, int $n): Html'
            . ' { return <b class="badge" data-n={$n}><i>pre</i>{$label}</b>; }';
        $props = ['label' => 'Ada <script>alert(1)</script>', 'n' => 3];

        $string = $this->tester()->render(sprintf($body, 'ParityBadge'), $props);
        $streamed = $this->tester()->renderStreaming(sprintf($body, 'ParityBadgeStream'), $props);

        self::assertSame($string, $streamed, 'the consumed stream equals the string render');
        self::assertStringContainsString(
            '&lt;script&gt;alert(1)&lt;/script&gt;',
            $streamed,
            'the stream lane escapes like the string lane',
        );
    }

    /** The one-target-per-process constraint is loud: streaming a source
     * whose component name the string target already declared refuses
     * before the fatal. */
    public function testRenderStreamingRefusesANameDeclaredByTheStringTarget(): void
    {
        $this->tester()->render(
            'function Twin(string $x): Html { return <i>{$x}</i>; }',
            ['x' => 'a'],
        );
        $this->expectException(\RuntimeException::class);
        $this->expectExceptionMessage('under one compile target');
        $this->tester()->renderStreaming(
            'function Twin(string $x): Html { return <i>{$x}</i>; }',
            ['x' => 'a'],
        );
    }
}
