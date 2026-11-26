<?php

declare(strict_types=1);

namespace Lintl\Tests;

use Lintl\Html;
use Lintl\Suspense;
use Lintl\Tests\Support\StreamingPage;
use PHPUnit\Framework\TestCase;

use function Lintl\c;
use function Lintl\stream;

/**
 * The §10 streaming & suspense runtime (architecture doc §10,
 * docs/stream-alignment.md): the flush loop yields chunks progressively,
 * a <Suspense> boundary flushes its fallback placeholder in place, then
 * flushes the resolved content out-of-order inside a <template> plus a
 * swap script that replaces the placeholder.
 *
 * Chunk order — not wall-clock — is the contract: every assertion reads
 * the captured chunk sequence, so the tests are deterministic on any CI.
 */
final class StreamingSuspenseTest extends TestCase
{
    /** @return list<string> */
    private static function capture(mixed $tree): array
    {
        $chunks = [];
        stream($tree, static function (string $chunk) use (&$chunks): void {
            $chunks[] = $chunk;
        });

        return $chunks;
    }

    /** The shell flushes progressively: each chunk arrives as its own flush. */
    public function test_shell_chunks_flush_progressively(): void
    {
        $chunks = self::capture(StreamingPage::shell());
        self::assertSame(['<p>a</p>', '<p>b</p>', '<p>c</p>'], $chunks);
    }

    /** Capture mode (no sink) returns the same bytes the sink saw. A tree
     * generator is single-use, so each render builds its own. */
    public function test_capture_mode_equals_sink_chunks(): void
    {
        self::assertSame('<p>a</p><p>b</p><p>c</p>', stream(StreamingPage::shell()));
        self::assertSame(\implode('', self::capture(StreamingPage::shell())), stream(StreamingPage::shell()));
    }

    /** §10 step 1: the fallback flushes in place, immediately, inside the placeholder div. */
    public function test_placeholder_flushes_before_the_shell_continues(): void
    {
        $chunks = self::capture(StreamingPage::oneBoundary());
        $joined = \implode('', $chunks);

        $placeholderAt = \strpos($joined, '<div id="lt-1">Loading…</div>');
        $footerAt = \strpos($joined, '<footer>');
        $templateAt = \strpos($joined, '<template id="lt-1-r">');

        self::assertNotFalse($placeholderAt, $joined);
        self::assertNotFalse($footerAt, $joined);
        self::assertNotFalse($templateAt, $joined);
        self::assertTrue($placeholderAt < $footerAt, "placeholder must precede the sibling shell: {$joined}");
        self::assertTrue($footerAt < $templateAt, "resolved content must flush after the shell finished: {$joined}");
    }

    /** §10 step 3: the resolved boundary flushes a template + swap script, out-of-order. */
    public function test_resolved_boundary_flushes_template_plus_swap_script(): void
    {
        $joined = \implode('', self::capture(StreamingPage::oneBoundary()));

        self::assertStringContainsString('<template id="lt-1-r">', $joined);
        self::assertStringContainsString('<p>Resolved</p>', $joined);
        self::assertStringContainsString('</template><script>lintlSwap("lt-1")</script>', $joined);
        self::assertStringContainsString('function lintlSwap(', $joined, 'the one-time swap runtime ships with the first resolved boundary');
    }

    /** §10: a genuinely slow boundary — the work blocks before resolving. The
     * placeholder is flushed BEFORE the resolved content + swap script. */
    public function test_slow_boundary_placeholder_flushes_before_resolved_content(): void
    {
        $chunks = self::capture(StreamingPage::slowBoundary(30_000));
        $joined = \implode('', $chunks);

        self::assertGreaterThan(0, \strpos($joined, '<div id="lt-1">Loading…</div>'));
        // Every shell chunk precedes every resolved chunk in the sequence.
        $resolvedIndex = null;
        foreach ($chunks as $index => $chunk) {
            if (\str_contains($chunk, '<template id="lt-1-r">')) {
                $resolvedIndex = $index;
                break;
            }
        }
        self::assertNotNull($resolvedIndex);
        foreach (\array_slice($chunks, 0, $resolvedIndex) as $chunk) {
            self::assertStringNotContainsString('<template', $chunk, "a shell chunk carried resolved content: {$chunk}");
        }
    }

    /** §10 out-of-order: the second boundary resolves first and flushes first. */
    public function test_fast_boundary_resolves_before_slow_sibling_out_of_order(): void
    {
        $joined = \implode('', self::capture(StreamingPage::twoBoundaries()));

        $divA = \strpos($joined, '<div id="lt-1">');
        $divB = \strpos($joined, '<div id="lt-2">');
        $tplA = \strpos($joined, '<template id="lt-1-r">');
        $tplB = \strpos($joined, '<template id="lt-2-r">');

        self::assertTrue($divA !== false && $divB !== false && $tplA !== false && $tplB !== false, $joined);
        self::assertTrue($divA < $divB, 'placeholders flush in document order');
        self::assertTrue($tplB < $tplA, "the fast boundary (lt-2) must resolve and flush before the slow one (lt-1): {$joined}");
    }

    /** The swap runtime script is emitted once per render, not once per boundary. */
    public function test_swap_script_is_emitted_once(): void
    {
        $joined = \implode('', self::capture(StreamingPage::twoBoundaries()));
        self::assertSame(1, \substr_count($joined, 'function lintlSwap('));
    }

    /** §10 compat: the sync target runs boundaries inline — no fallback, no swap. */
    public function test_sync_target_runs_suspense_inline(): void
    {
        $boundary = new Suspense('Loading…', static fn (): Html => Html::unsafeRaw('<p>Inline</p>'));
        self::assertSame('<p>Inline</p>', c($boundary));
        // Inline rendering still escapes: the resolved value is data.
        self::assertSame('&lt;b&gt;evil&lt;/b&gt;', c(new Suspense('x', static fn (): string => '<b>evil</b>')));
    }

    /** A streamed subtree inside a resolved boundary flushes its chunks progressively. */
    public function test_resolved_generator_subtree_streams_inside_template(): void
    {
        $chunks = self::capture(StreamingPage::resolvedSubtree());
        $joined = \implode('', $chunks);
        self::assertStringContainsString('<template id="lt-1-r"><p>one</p><p>two</p></template>', $joined);
        // The swap runtime ships once, the template opens, the subtree's
        // chunks flush one by one, the template closes, the swap call runs.
        $normalized = \array_map(
            static fn (string $chunk): string => \str_contains($chunk, 'function lintlSwap(') ? 'DEF' : $chunk,
            $chunks,
        );
        self::assertSame(
            [
                '<main>',
                '<div id="lt-1">',
                'Loading…',
                '</div>',
                '</main>',
                'DEF',
                '<template id="lt-1-r">',
                '<p>one</p>',
                '<p>two</p>',
                '</template>',
                '<script>lintlSwap("lt-1")</script>',
            ],
            $normalized,
        );
    }

    /** Unknown chunk values are rejected exactly like c() rejects them. */
    public function test_foreign_object_chunk_is_rejected(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        stream((static function (): \Generator {
            yield '<p>x</p>';
            yield new \stdClass;
        })());
    }

    /** The chunk matrix mirrors c()'s Traversable contract: markup chunks pass
     * raw, data rides through c() before it becomes a chunk, Html is the
     * trusted sink, null/bool render empty, scalars stringify. */
    public function test_chunk_value_matrix(): void
    {
        $tree = (static function (): \Generator {
            yield '<p>';
            yield c('a&b'); // data string — escaped through the helper, like compiled output
            yield '</p>';
            yield '<p>';
            yield 'raw markup chunk'; // compiler-produced markup passes raw
            yield '</p>';
            yield Html::unsafeRaw('<hr>'); // trusted markup
            yield 7;
            yield null;
            yield true;
        })();
        self::assertSame('<p>a&amp;b</p><p>raw markup chunk</p><hr>7', stream($tree));
    }

    /** A boundary discovered inside resolved content gets its own out-of-order flush. */
    public function test_nested_boundary_inside_resolved_content(): void
    {
        $joined = \implode('', self::capture(StreamingPage::nestedBoundary()));
        self::assertStringContainsString('<template id="lt-1-r">', $joined);
        self::assertStringContainsString('<div id="lt-2">nested…</div>', $joined);
        self::assertStringContainsString('<template id="lt-2-r">', $joined);
        self::assertTrue(
            \strpos($joined, '<div id="lt-2">') > \strpos($joined, '<template id="lt-1-r">'),
            'the nested placeholder belongs inside the outer resolved template',
        );
        self::assertTrue(
            \strpos($joined, '<template id="lt-2-r">') > \strpos($joined, '<div id="lt-2">'),
            $joined,
        );
    }

    /** Work throwing propagates: a boundary failure is a render failure. */
    public function test_work_exception_propagates(): void
    {
        $this->expectException(\LogicException::class);
        stream((static function (): \Generator {
            yield '<p>x</p>';
            yield new Suspense('fb', static fn (): string => throw new \LogicException('boom'));
        })());
    }
}
