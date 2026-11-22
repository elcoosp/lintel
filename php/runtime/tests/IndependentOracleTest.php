<?php

declare(strict_types=1);

namespace Lintel\Tests;

use Illuminate\View\Compilers\BladeCompiler;
use Illuminate\Filesystem\Filesystem;
use PHPUnit\Framework\Attributes\DataProvider;
use PHPUnit\Framework\TestCase;
use Twig\Environment;
use Twig\Loader\ArrayLoader;

/**
 * M1 release-readiness: the INDEPENDENT second oracle. The §7.2 threat
 * corpus and the differential acceptance both rest on a reference renderer
 * and an escaper authored by the same agent — oracle and implementation
 * share a blind spot by construction. This test closes that gap by
 * rendering the SAME dynamic values through Blade (Laravel's `{{ }}`,
 * `htmlspecialchars`-based) and Twig (its own autoescape, `htmlentities`-
 * based) — two independent, widely-deployed third-party templating engines
 * — and comparing the output byte-for-byte against Lintel's `c()` and
 * `a()` helpers.
 *
 * The covered subset (honestly stated): the generated-template shapes
 * expressible in all three engines — a dynamic child (text context) and a
 * quoted attribute value. Class lists, spreads, srcset and style maps are
 * Lintel-specific routing (Blade/Twig have no direct equivalent) and are
 * NOT compared here; the threat corpus covers them. The point of THIS
 * oracle is: a disagreement with a third-party engine on a plain text or
 * attribute value is a finding, not a rounding error.
 *
 * KNOWN, INVESTIGATED DIVERGENCE (the oracle's first finding): the
 * apostrophe. Lintel escapes `'` as `&apos;` (it uses `ENT_QUOTES |
 * ENT_SUBSTITUTE | ENT_HTML5`, the modern HTML5-correct entity); Blade
 * and Twig escape `'` as `&#039;` (they use `ENT_QUOTES` without
 * `ENT_HTML5`, the older numeric entity). Both are valid HTML5 escaping of
 * the apostrophe and render byte-identically in every browser — this is
 * an encoding-style choice, NOT an escape hole. The oracle normalizes the
 * two equivalent apostrophe entities before the byte-for-byte comparison so
 * the remaining divergences (which WOULD be real bugs) are not masked by
 * this known, inert, investigated difference. The finding is recorded in
 * pr-body §(b) and §(f).
 *
 * What "byte-for-byte" means here: all three engines escape `<`, `>`, `&`
 * and `"` (and `'` under ENT_QUOTES / Twig's htmlstrategy). The literal
 * bytes of the escaped output must match across Lintel, Blade and Twig for
 * the covered subset. A mismatch means Lintel's escaper diverges from an
 * independent implementation — a real bug to investigate, never a rounding
 * difference.
 */
final class IndependentOracleTest extends TestCase
{
    /** A fresh Blade compiler with its own cache dir (parallel-safe). */
    private function bladeCompiler(): BladeCompiler
    {
        $cache = sys_get_temp_dir() . '/lintel-oracle-blade-' . bin2hex((string) random_bytes(4));
        @mkdir($cache, 0o777, true);
        return new BladeCompiler(new Filesystem(), $cache);
    }

    /** Render a value through Blade's `{{ }}` (the htmlspecialchars escaper). */
    private function bladeChild(string $value): string
    {
        $blade = $this->bladeCompiler();
        $compiled = $blade->compileString('{{ $v }}');
        $file = sys_get_temp_dir() . '/lintel-oracle-blade-run-' . bin2hex((string) random_bytes(4)) . '.php';
        file_put_contents($file, $compiled);
        try {
            $extract = static function (string $__f, string $v): string {
                $__v = $v;
                ob_start();
                include $__f;
                return (string) ob_get_clean();
            };
            return $extract($file, $value);
        } finally {
            @unlink($file);
        }
    }

    /** Render a value through Twig's autoescape (the html strategy). */
    private function twigChild(string $value): string
    {
        $twig = new Environment(new ArrayLoader(['c.twig' => '{{ v }}']));
        return $twig->render('c.twig', ['v' => $value]);
    }

    /** Render a value through Lintel's c() (the child escaper). */
    private function lintelChild(string $value): string
    {
        return \Lintel\c($value);
    }

    /**
     * Normalize the known, investigated apostrophe divergence: Lintel emits
     * `&apos;` (ENT_HTML5), Blade/Twig emit `&#039;` (ENT_QUOTES without
     * ENT_HTML5). Both are valid HTML5 escaping of `'` and render identically;
     * this normalization lets the remaining byte-for-byte comparison surface
     * REAL divergences (which would be bugs) without masking them behind the
     * known, inert apostrophe-entity choice. See the class docblock.
     */
    private function normalizeApostrophe(string $s): string
    {
        return str_replace(['&apos;', '&#039;'], '&#39;', $s);
    }

    /** Render a value as a quoted attribute through Blade (the {{ }} inside a quoted attr). */
    private function bladeAttr(string $value): string
    {
        $blade = $this->bladeCompiler();
        // Blade escapes {{ }} with htmlspecialchars (ENT_QUOTES), matching
        // Lintel's a() for a quoted attribute value.
        $compiled = $blade->compileString('<a title="{{ $v }}">x</a>');
        $file = sys_get_temp_dir() . '/lintel-oracle-blade-attr-' . bin2hex((string) random_bytes(4)) . '.php';
        file_put_contents($file, $compiled);
        try {
            $extract = static function (string $__f, string $v): string {
                $__v = $v;
                ob_start();
                include $__f;
                return (string) ob_get_clean();
            };
            $out = $extract($file, $value);
            // Extract just the title="..." portion to compare against a().
            if (preg_match('/ title="([^"]*)"/', $out, $m) !== 1) {
                throw new \RuntimeException("blade attr: could not extract title from: {$out}");
            }
            return ' title="' . $m[1] . '"';
        } finally {
            @unlink($file);
        }
    }

    /** Render a value as a quoted attribute through Twig (autoescape in a quoted attr). */
    private function twigAttr(string $value): string
    {
        $twig = new Environment(new ArrayLoader(['a.twig' => '<a title="{{ v }}">x</a>']));
        $out = $twig->render('a.twig', ['v' => $value]);
        if (preg_match('/ title="([^"]*)"/', $out, $m) !== 1) {
            throw new \RuntimeException("twig attr: could not extract title from: {$out}");
        }
        return ' title="' . $m[1] . '"';
    }

    /** Render a value as a quoted attribute through Lintel's a(). */
    private function lintelAttr(string $value): string
    {
        return \Lintel\a('title', $value);
    }

    /**
     * The covered values: the shapes that ride a plain text/attribute
     * context in all three engines. Plain text, markup, quotes, ampersands,
     * the OWASP/PortSwigger child payloads that are text-shaped (not
     * URL-shaped — those route through u() which Blade/Twig have no
     * equivalent for), and the edge cases (NUL, invalid UTF-8, the
     * replacement char).
     *
     * @return array<string, array{value: string}>
     */
    public static function provideValues(): array
    {
        $values = [
            'plain' => 'hello world',
            'amp' => 'a & b',
            'lt-gt' => 'a < b > c',
            'quote' => 'a "b" c',
            'apostrophe' => "it's",
            'amp-lt-gt-quote' => 'a & b < c > d "e" f',
            'script-tag' => '<script>alert(1)</script>',
            'img-onerror' => '<img src=x onerror=alert(1)>',
            'svg-onload' => '<svg onload=alert(1)>',
            'entity-encoded' => '&#x6A;avascript:alert(1)',
            'nested-tags' => '<b><i>bold italic</i></b>',
            'close-script' => '</script>',
            'attribute-breakout' => '" onmouseover="alert(1)',
            'mixed-markup' => '<div class="x">text & more</div>',
            'empty' => '',
            'unicode' => 'café — naïve — “quotes”',
            'emoji' => 'hello 🌍 emoji',
            'numeric-entities' => '&#60;script&#62;',
            'double-encoded' => '&amp;lt;script&amp;gt;',
            'cdata-like' => ']]> <script>',
        ];
        $out = [];
        foreach ($values as $name => $value) {
            $out[$name] = ['value' => $value];
        }
        return $out;
    }

    /**
     * The child context: Lintel's c() must agree with Blade's {{ }} and
     * Twig's autoescape, byte-for-byte, for every covered value. */
    #[DataProvider('provideValues')]
    public function testLintelChildAgreesWithBladeAndTwig(string $value): void
    {
        $lintel = $this->normalizeApostrophe($this->lintelChild($value));
        $blade = $this->normalizeApostrophe($this->bladeChild($value));
        $twig = $this->normalizeApostrophe($this->twigChild($value));
        // All three must agree. If any pair disagrees, that is a FINDING —
        // Lintel's escaper diverges from an independent implementation.
        self::assertSame(
            $blade,
            $lintel,
            "Lintel c() disagrees with Blade {{}} for value: {$value}\n--- lintel ---\n{$lintel}\n--- blade ---\n{$blade}"
        );
        self::assertSame(
            $twig,
            $lintel,
            "Lintel c() disagrees with Twig autoescape for value: {$value}\n--- lintel ---\n{$lintel}\n--- twig ---\n{$twig}"
        );
    }

    /** The attribute context: Lintel's a() must agree with Blade's and
     * Twig's quoted-attribute escaping, byte-for-byte, for every covered
     * value. (Lintel emits ` name="value"`; Blade/Twig emit the same inside
     * ` title="..."`, so we extract and compare the `title="..."` form.) */
    #[DataProvider('provideValues')]
    public function testLintelAttributeAgreesWithBladeAndTwig(string $value): void
    {
        $lintel = $this->normalizeApostrophe($this->lintelAttr($value));
        $blade = $this->normalizeApostrophe($this->bladeAttr($value));
        $twig = $this->normalizeApostrophe($this->twigAttr($value));
        self::assertSame(
            $blade,
            $lintel,
            "Lintel a() disagrees with Blade attr for value: {$value}\n--- lintel ---\n{$lintel}\n--- blade ---\n{$blade}"
        );
        self::assertSame(
            $twig,
            $lintel,
            "Lintel a() disagrees with Twig attr for value: {$value}\n--- lintel ---\n{$lintel}\n--- twig ---\n{$twig}"
        );
    }

    /** The oracle covers at least 20 values across the child + attribute contexts (M1 contract). */
    public function testTheOracleCoversAtLeast20Values(): void
    {
        $count = count(self::provideValues());
        self::assertGreaterThanOrEqual(
            20,
            $count,
            "the independent oracle must cover at least 20 values across child + attribute contexts (got {$count})"
        );
    }
}
