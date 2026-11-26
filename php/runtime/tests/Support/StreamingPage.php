<?php

declare(strict_types=1);

namespace Lintl\Tests\Support;

use Lintl\Html;
use Lintl\Suspense;

use function Lintl\c;

/**
 * Page builders for the §10 streaming/suspense e2e (StreamingSuspenseTest).
 * Each builder returns a stream-target render tree exactly the shape the
 * compiler emits (`docs/stream-alignment.md`): a generator yielding
 * markup-literal chunks and `c()`-escaped data chunks, plus Suspense
 * boundary values where the boundary sits.
 */
final class StreamingPage
{
    /** A boundary-free shell: three literal chunks. */
    public static function shell(): \Generator
    {
        yield '<p>a</p>';
        yield '<p>b</p>';
        yield '<p>c</p>';
    }

    /** One boundary between shell chunks; the resolved value is trusted markup. */
    public static function oneBoundary(): \Generator
    {
        yield '<header>shell</header>';
        yield new Suspense('Loading…', static fn (): Html => Html::unsafeRaw('<p>Resolved</p>'));
        yield '<footer>done</footer>';
    }

    /** A slow boundary: the work blocks for $us microseconds before resolving. */
    public static function slowBoundary(int $us): \Generator
    {
        yield '<header>shell</header>';
        yield new Suspense('Loading…', static function () use ($us): Html {
            \usleep($us);

            return Html::unsafeRaw('<p>Slow</p>');
        });
        yield '<footer>done</footer>';
    }

    /** Two boundaries: lt-1 suspends twice (slow), lt-2 resolves immediately. */
    public static function twoBoundaries(): \Generator
    {
        yield '<main>';
        yield new Suspense('A…', static function (): Html {
            \Fiber::suspend();
            \Fiber::suspend();

            return Html::unsafeRaw('<p>A</p>');
        });
        yield '<hr>';
        yield new Suspense('B…', static fn (): Html => Html::unsafeRaw('<p>B</p>'));
        yield '</main>';
    }

    /** The resolved content is itself a streamed subtree. */
    public static function resolvedSubtree(): \Generator
    {
        yield '<main>';
        yield new Suspense('Loading…', static function (): \Generator {
            yield '<p>one</p>';
            yield '<p>two</p>';
        });
        yield '</main>';
    }

    /** A boundary discovered while walking the outer boundary's resolved subtree. */
    public static function nestedBoundary(): \Generator
    {
        yield '<main>';
        yield new Suspense('outer…', static function (): \Generator {
            yield '<p>outer</p>';
            yield new Suspense('nested…', static fn (): Html => Html::unsafeRaw('<p>nested-done</p>'));
            yield '<p>outer-end</p>';
        });
        yield '</main>';
    }

    /** A full §10 demo page: shell, one slow boundary with escaped dynamic data. */
    public static function demo(): \Generator
    {
        yield '<!doctype html><html><body><h1>Suspense demo</h1>';
        yield new Suspense(
            'Loading the report…',
            static function (): \Generator {
                // The "async" work: a blocking call, then streamed markup.
                \usleep(20_000);
                yield '<ul>';
                yield '<li>user: '.c('<script>alert(1)</script>').'</li>';
                yield '</ul>';
            },
        );
        yield '<p>footer — flushed while the boundary was still pending</p>';
        yield '</body></html>';
    }
}
