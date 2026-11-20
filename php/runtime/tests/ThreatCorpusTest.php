<?php

declare(strict_types=1);

namespace Lintel\Tests;

use Lintel\Html;
use PHPUnit\Framework\Attributes\DataProvider;
use PHPUnit\Framework\TestCase;
use InvalidArgumentException;

use function Lintel\a;
use function Lintel\attrs;
use function Lintel\c;
use function Lintel\cls;
use function Lintel\e;
use function Lintel\srcset;
use function Lintel\sty;
use function Lintel\u;

/**
 * The §7.2 threat corpus (task-card T-07): every attack vector the
 * architecture doc lists must be neutralized by the runtime helpers, plus
 * the §7.1 escaping-matrix semantics the helpers own. Every assertion is
 * exact where the helper's output is deterministic.
 */
final class ThreatCorpusTest extends TestCase
{
    /** §7.2: whitespace inside the scheme (`java\tscript:`) is invisible to browsers, not to the probe. */
    #[DataProvider('provideSchemeObfuscations')]
    public function testWhitespaceObfuscatedSchemesAreBlocked(string $payload): void
    {
        self::assertSame(' href="about:invalid#blocked"', u('href', $payload));
    }

    /** @return iterable<string, array{string}> */
    public static function provideSchemeObfuscations(): iterable
    {
        yield 'tab in scheme' => ["java\tscript:alert(1)"];
        yield 'newline in scheme' => ["java\nscript:alert(1)"];
        yield 'nul in scheme' => ["java\0script:alert(1)"];
    }

    /** §7.2: `JaVaScRiPt:` — scheme checks are case-insensitive. */
    public function testMixedCaseSchemeIsBlocked(): void
    {
        self::assertSame(' href="about:invalid#blocked"', u('href', 'JaVaScRiPt:alert(1)'));
    }

    /** §7.2: `&#x6A;avascript:` — entity-encoded input is a string value, so `&` escapes and the vector goes inert. */
    public function testEntityEncodedSchemeIsEscapedNotExecuted(): void
    {
        $out = u('href', '&#x6A;avascript:alert(1)');
        self::assertSame(' href="&amp;#x6A;avascript:alert(1)"', $out);
    }

    /** §7.2: `data:text/html` is outside the scheme allowlist. */
    public function testDataSchemeIsBlocked(): void
    {
        $out = u('href', 'data:text/html;base64,PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg==');
        self::assertSame(' href="about:invalid#blocked"', $out);
    }

    /** §7.2: `vbscript:` is outside the scheme allowlist. */
    public function testVbscriptSchemeIsBlocked(): void
    {
        self::assertSame(' href="about:invalid#blocked"', u('href', 'vbscript:msgbox(1)'));
    }

    /** §7.1: the allowlist plus relative URLs pass through. */
    public function testAllowedSchemesAndRelativeUrlsPass(): void
    {
        foreach (['https://lintel.sh', 'http://lintel.sh', 'mailto:x@y.z', 'tel:+123', 'sms:+123', '/relative', 'img.png', '#anchor'] as $url) {
            self::assertSame(' href="' . $url . '"', u('href', $url), $url);
        }
        self::assertSame(' href="https://lintel.sh"', u('href', 'https://lintel.sh'));
        // The scheme check is case-insensitive, the emitted value is as written.
        self::assertSame(' href="HTTPS://LINTEL.SH"', u('href', 'HTTPS://LINTEL.SH'));
    }

    /** §7.2: attribute-breakout `" onmouseover=` — the quote escapes, the value stays inside the attribute. */
    public function testQuoteBreakoutCannotEscapeTheAttribute(): void
    {
        $out = a('title', '" onmouseover="alert(1)');
        self::assertSame(' title="&quot; onmouseover=&quot;alert(1)"', $out);
    }

    /** §7.2: `</script>` in JSON — as a child value the text escaper neutralizes it; JSON islands (§7.1) are a compiler-side emission and not yet lowered. */
    public function testScriptCloseTagAsChildIsEscaped(): void
    {
        $out = c('</script><script>alert(1)</script>');
        self::assertSame('&lt;/script&gt;&lt;script&gt;alert(1)&lt;/script&gt;', $out);
    }

    /** §7.2: NUL bytes must not defeat the escaping that surrounds them. */
    public function testNulBytesDoNotDefeatEscaping(): void
    {
        $child = c("a\x00b<script>alert(1)</script>");
        self::assertStringNotContainsString('<', $child);
        self::assertStringContainsString('&lt;script&gt;', $child);

        $attr = a('title', "\x00\" onmouseover=\"x");
        self::assertStringNotContainsString('" ', substr((string) $attr, 8));
    }

    /** §7.2: invalid UTF-8 is substituted (ENT_SUBSTITUTE), never emitted raw. */
    public function testInvalidUtf8IsSubstituted(): void
    {
        $out = c("\xC3\x28<script>");
        self::assertTrue(mb_check_encoding($out, 'UTF-8'), 'output must be valid UTF-8');
        self::assertStringContainsString("\u{FFFD}", $out);
        self::assertStringNotContainsString("\xC3\x28", $out);
        self::assertStringNotContainsString('<script>', $out);
    }

    /** §7.2: a spread with an `onclick` key is rejected outright — no JS-in-attribute from data. */
    public function testSpreadWithOnclickKeyIsRejected(): void
    {
        $this->expectException(InvalidArgumentException::class);
        attrs(['onclick' => 'alert(1)']);
    }

    /** §7.2: `<`-in-attribute-name injection via spread keys — names outside the grammar are rejected. */
    public function testAngleBracketAttributeNameIsRejected(): void
    {
        $this->expectException(InvalidArgumentException::class);
        attrs(['<script>alert(1)</script>' => 'x']);
    }

    /** §7.2 spread guard, case-insensitive: `OnClick` is still a handler. */
    public function testSpreadHandlerNamesAreCaseInsensitive(): void
    {
        $this->expectException(InvalidArgumentException::class);
        attrs(['OnClick' => 'alert(1)']);
    }

    /** §7.1: `Html::unsafeRaw` is the single greppable raw sink; it does not escape, by contract. */
    public function testUnsafeRawIsTheGreppableSink(): void
    {
        $raw = Html::unsafeRaw('<b>raw</b>');
        self::assertSame('<b>raw</b>', (string) $raw);
        self::assertSame('<b>raw</b>', c($raw));
    }

    /** Compiled output is global-namespace PHP and declares unqualified `Html` types (doc §6.9): the alias must resolve. */
    public function testGlobalHtmlAliasResolves(): void
    {
        self::assertTrue(class_exists('Html', false));
        self::assertSame('<i>x</i>', (new \Html('<i>x</i>'))->s);
        self::assertSame('<i>x</i>', c(new \Html('<i>x</i>')));
    }

    /** §7.1: arbitrary Stringable values (not `Html`) are still escaped — only `Html` is trusted. */
    public function testForeignStringableIsEscaped(): void
    {
        $value = new class implements \Stringable {
            public function __toString(): string
            {
                return '<img src=x onerror=alert(1)>';
            }
        };
        self::assertSame('&lt;img src=x onerror=alert(1)&gt;', c($value));
    }

    /** §6.8: the `c()` render matrix — every supported child type and the rejection case. */
    public function testChildRenderMatrix(): void
    {
        self::assertSame('7', c(7));
        self::assertSame('2.5', c(2.5));
        self::assertSame('', c(null));
        self::assertSame('', c(true));
        self::assertSame('', c(false));
        self::assertSame('a&amp;b', c('a&b'));
        self::assertSame('<b>ok</b>', c(Html::unsafeRaw('<b>ok</b>')));
        self::assertSame('ab', c(['a', 'b']));
        $this->expectException(InvalidArgumentException::class);
        c(new \stdClass());
    }

    /** §6.8: iterables render recursively through the same escaper. */
    public function testNestedIterablesRenderRecursively(): void
    {
        self::assertSame('a&lt;b&gt;', c(['a', '<b>']));
        self::assertSame('a&lt;b&gt;', c([['a'], '<b>']));
    }

    /** §6.8: attribute bool/null semantics — true renders a bare flag, false/null render nothing. */
    public function testAttributeBoolAndNullSemantics(): void
    {
        self::assertSame(' disabled', a('disabled', true));
        self::assertSame('', a('disabled', false));
        self::assertSame('', a('disabled', null));
        $this->expectException(InvalidArgumentException::class);
        a('title', ['array']);
    }

    /** §6.8: `class={…}` accepts strings, lists and `['cls' => bool]` maps. */
    public function testClassHelperShapes(): void
    {
        self::assertSame(' class="btn"', cls('btn'));
        self::assertSame(' class="btn active"', cls(['btn', 'active']));
        self::assertSame(' class="btn"', cls(['btn' => true, 'off' => false]));
        self::assertSame('', cls(null));
        self::assertSame('', cls([]));
        self::assertSame(' class="a&amp;b"', cls('a&b'));
    }

    /** §6.8: `style={[…]}` restricts values to the safe charset; hostile values are rejected. */
    public function testStyleHelperCharset(): void
    {
        self::assertSame(' style="color:red"', sty(['color' => 'red']));
        self::assertSame(' style="--x:1px"', sty(['--x' => '1px']));
        self::assertSame('', sty([]));
        $this->expectException(InvalidArgumentException::class);
        sty(['color' => 'red";"']);
    }

    /** §6.8: spread routing — each key goes through the right helper, hostile keys rejected first. */
    public function testSpreadRoutingPerKey(): void
    {
        $out = attrs(['href' => 'https://lintel.sh', 'class' => 'btn', 'style' => ['color' => 'red'], 'data-id' => '7']);
        self::assertSame(' href="https://lintel.sh" class="btn" style="color:red" data-id="7"', $out);
        $out = attrs(['hidden' => true, 'title' => null]);
        self::assertSame(' hidden', $out);
    }

    /** §7.1: `srcset` splits candidates and scheme-checks each URL (docs/srcset-alignment.md). */
    public function testSrcsetChecksEachCandidateUrl(): void
    {
        self::assertSame(' srcset="a.png 1x, b.png 2x"', srcset('srcset', 'a.png 1x, b.png 2x'));
        self::assertSame(
            ' srcset="safe.png 1x, about:invalid#blocked 2x"',
            srcset('srcset', "safe.png 1x, javascript:alert(1) 2x"),
        );
        self::assertSame(
            ' srcset="about:invalid#blocked 2x"',
            srcset('srcset', "java\tscript:alert(1) 2x"),
        );
        self::assertSame(
            ' srcset="https://lintel.sh/img.png 2x, about:invalid#blocked 4x"',
            srcset('srcset', 'https://lintel.sh/img.png 2x, data:text/html,x 4x'),
        );
    }

    /** §7.1: `srcset` bool/null semantics match the other attribute helpers. */
    public function testSrcsetEmptySemantics(): void
    {
        self::assertSame('', srcset('srcset', null));
        self::assertSame('', srcset('srcset', false));
        self::assertSame('', srcset('srcset', '  ,  '));
    }

    /** §6.8: `e()` is the standalone escaper (ENT_QUOTES | SUBSTITUTE | HTML5). */
    public function testEscaper(): void
    {
        self::assertSame('a&amp;b&lt;c&gt;d&quot;e&apos;f', e('a&b<c>d"e\'f'));
    }
}
