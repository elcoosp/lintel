<?php

declare(strict_types=1);

namespace Lintel\Blade\Tests;

use Lintel\Blade\Bridge;
use Lintel\Blade\Profiler\DebugbarCollector;
use Lintel\Blade\Profiler\Panel;
use Lintel\Dev\Loader;
use Lintel\Html;
use Lintel\Suspense;
use PHPUnit\Framework\TestCase;

use function Lintel\stream;

/**
 * The §11 profiler surface (§17.1-Q2's remaining Laravel surface): the
 * panel and the Debugbar collector capture the real metrics of a real
 * render window — files compiled, cache hits/misses, compilation time,
 * and suspense boundaries flushed — off the dev loader's counters and
 * the runtime's flusher counter. Each test compiles a component of its
 * own name (the function table is process-global, so a shared fixture
 * would be served by `function_exists` before the loader ever ran).
 */
final class ProfilerPanelTest extends TestCase
{
    /** A real compilation run: the render compiles (a miss with measured
     * spend), and the loader's cache-key lane serves a re-require as a
     * hit — a declaration-free source (compiles to its own comment), so
     * the second require cannot redeclare. */
    public function test_collector_captures_compile_metrics(): void
    {
        $name = 'Pnl'.bin2hex((string) random_bytes(4));
        $cacheDir = $this->workspace($name);
        $bridge = new Bridge($cacheDir, [$cacheDir]);
        $panel = new Panel;
        $panel->reset();

        $bridge->render($name, ['name' => 'P']);
        $m = $panel->metrics();
        self::assertSame(1, $m['files']);
        self::assertSame(1, $m['cache_misses'], 'the first render compiles');
        self::assertSame(0, $m['cache_hits']);
        self::assertGreaterThan(0.0, $m['compile_time_ms'], 'the compile spend is measured');

        $notes = $cacheDir.'/Notes.lintel.php';
        \file_put_contents($notes, '<?php // comment-only source, no declarations');
        Loader::require($notes, $cacheDir);
        Loader::require($notes, $cacheDir);
        $m = $panel->metrics();
        self::assertSame(3, $m['files']);
        self::assertSame(2, $m['cache_misses'], 'the notes source compiled once');
        self::assertSame(1, $m['cache_hits'], 'the second serve hit the cache key');
    }

    /** A real streamed render with a boundary increments the counter. */
    public function test_collector_counts_suspense_boundaries_flushed(): void
    {
        $panel = new Panel;
        $panel->reset();

        $tree = [
            '<ul>',
            new Suspense(fallback: Html::unsafeRaw('<li>loading</li>'), work: static fn (): Html => Html::unsafeRaw('<li>data</li>')),
            '</ul>',
        ];
        $out = stream($tree);

        self::assertStringContainsString('<li>loading</li>', $out);
        self::assertStringContainsString('<li>data</li>', $out);
        self::assertSame(1, $panel->metrics()['suspense_flushed'], 'one boundary flushed');
    }

    /** reset() zeroes the counters so a window measures exactly itself. */
    public function test_reset_zeroes_counters(): void
    {
        $name = 'Pnl'.bin2hex((string) random_bytes(4));
        $cacheDir = $this->workspace($name);
        (new Bridge($cacheDir, [$cacheDir]))->render($name);
        self::assertGreaterThan(0, (new Panel)->metrics()['files']);

        $panel = new Panel;
        $panel->reset();
        $m = $panel->metrics();
        self::assertSame(0, $m['files']);
        self::assertSame(0, $m['cache_hits']);
        self::assertSame(0, $m['cache_misses']);
        self::assertSame(0, $m['suspense_flushed']);
        self::assertSame(0.0, $m['compile_time_ms']);
    }

    /** The Debugbar adapter: name, widget spec, and the metric snapshot. */
    public function test_debugbar_collector_exposes_the_panel(): void
    {
        $name = 'Pnl'.bin2hex((string) random_bytes(4));
        $cacheDir = $this->workspace($name);
        $panel = new Panel;
        $panel->reset();
        (new Bridge($cacheDir, [$cacheDir]))->render($name, ['name' => 'D']);

        $collector = new DebugbarCollector($panel);
        self::assertSame('lintel', $collector->getName());

        $widgets = $collector->getWidgets();
        self::assertArrayHasKey('lintel', $widgets);
        self::assertSame('PhpDebugBar.Widgets.VariableListWidget', $widgets['lintel']['widget']);
        self::assertSame('lintel', $widgets['lintel']['map']);

        $data = $collector->collect();
        self::assertSame(1, $data['files']);
        self::assertSame(1, $data['cache_misses']);
        self::assertGreaterThan(0.0, $data['compile_time_ms']);
        self::assertSame(0, $data['suspense_flushed']);
    }

    /** A workspace with one `<Name>.lintel.php` component, compiled fresh. */
    private function workspace(string $name): string
    {
        $dir = sys_get_temp_dir().'/lintel-panel-'.bin2hex((string) random_bytes(4));
        \mkdir($dir, 0777, true);
        \file_put_contents($dir.'/'.$name.'.lintel.php', <<<LINTEL
        <?php
        function {$name}(string \$name = '') {
            return <b class="badge">Hello, {\$name}</b>;
        }
        LINTEL);

        return $dir;
    }
}
