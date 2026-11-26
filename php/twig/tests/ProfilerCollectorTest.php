<?php

declare(strict_types=1);

namespace Lintl\Twig\Tests;

use Lintl\Dev\Loader;
use Lintl\Html;
use Lintl\Suspense;
use Lintl\Twig\Extension;
use Lintl\Twig\Profiler\LintlDataCollector;
use PHPUnit\Framework\TestCase;
use Symfony\Component\HttpFoundation\Request;
use Symfony\Component\HttpFoundation\Response;
use Twig\Environment;
use Twig\Loader\ArrayLoader;

use function Lintl\stream;

/**
 * The §11 Symfony Web Profiler panel's data source (§17.1-Q2's remaining
 * Symfony surface): the collector captures the same metrics the Laravel
 * panel shows — files compiled, cache hits/misses, compilation time,
 * suspense boundaries flushed — from a real compilation run and a real
 * streamed render, collected through the real profiler `collect()` lane.
 */
final class ProfilerCollectorTest extends TestCase
{
    /** A real compilation run collected through `collect()`: files, hits,
     * misses and measured compile time. The hit half re-requires a
     * declaration-free source (compiles to its own comment) — a component
     * re-require would redeclare. */
    public function test_collector_captures_compile_metrics(): void
    {
        $name = 'Pfc'.bin2hex((string) random_bytes(4));
        $cacheDir = $this->workspace($name);
        $collector = new LintlDataCollector;
        $collector->reset();

        $this->renderInTwig($cacheDir, $name);
        $collector->collect(new Request, new Response);
        $m = $collector->metrics();

        self::assertSame(1, $m['files']);
        self::assertSame(1, $m['cache_misses'], 'the render compiled');
        self::assertSame(0, $m['cache_hits']);
        self::assertGreaterThan(0.0, $m['compile_time_ms'], 'the compile spend is measured');

        $notes = $cacheDir.'/Notes.lintl.php';
        \file_put_contents($notes, '<?php // comment-only source, no declarations');
        Loader::require($notes, $cacheDir);
        Loader::require($notes, $cacheDir);
        $collector->collect(new Request, new Response);
        $m = $collector->metrics();
        self::assertSame(3, $m['files']);
        self::assertSame(1, $m['cache_hits'], 'the second serve hit the cache key');
        self::assertSame(2, $m['cache_misses']);
    }

    /** A real streamed render with a boundary lands in the snapshot. */
    public function test_collector_counts_suspense_boundaries_flushed(): void
    {
        $collector = new LintlDataCollector;
        $collector->reset();

        $tree = [
            '<ul>',
            new Suspense(fallback: Html::unsafeRaw('<li>loading</li>'), work: static fn (): Html => Html::unsafeRaw('<li>data</li>')),
            '</ul>',
        ];
        $out = stream($tree);

        self::assertStringContainsString('<li>loading</li>', $out);
        $collector->collect(new Request, new Response);
        self::assertSame(1, $collector->metrics()['suspense_flushed'], 'one boundary flushed');
    }

    /** reset() zeroes both counters and the snapshot. */
    public function test_reset_zeroes_the_panel(): void
    {
        $name = 'Pfc'.bin2hex((string) random_bytes(4));
        $cacheDir = $this->workspace($name);
        $this->renderInTwig($cacheDir, $name);
        self::assertGreaterThan(0, Loader::metrics()->files);

        $collector = new LintlDataCollector;
        $collector->reset();
        $collector->collect(new Request, new Response);
        $m = $collector->metrics();
        self::assertSame(0, $m['files']);
        self::assertSame(0, $m['cache_hits']);
        self::assertSame(0, $m['cache_misses']);
        self::assertSame(0, $m['suspense_flushed']);
        self::assertSame(0.0, $m['compile_time_ms']);
    }

    /** The profiler plumbing: panel id + template resource the app wires. */
    public function test_collector_name_and_template_resource(): void
    {
        $collector = new LintlDataCollector;

        self::assertSame('lintl', $collector->getName());
        self::assertFileExists(__DIR__.'/../Resources/views/panel.html.twig', 'the app-pointable template ships');
    }

    /** Render a suite-unique component through the bridge's Twig function. */
    private function renderInTwig(string $cacheDir, string $name): string
    {
        $twig = new Environment(new ArrayLoader(['p.twig' => '{{ lintl_render(name_var) }}']));
        $twig->addExtension(new Extension($cacheDir, [$cacheDir]));

        return $twig->render('p.twig', ['name_var' => $name]);
    }

    /** A workspace with one `<Name>.lintl.php` component, compiled fresh. */
    private function workspace(string $name): string
    {
        $dir = sys_get_temp_dir().'/lintl-sf-profiler-'.bin2hex((string) random_bytes(4));
        \mkdir($dir, 0777, true);
        \file_put_contents($dir.'/'.$name.'.lintl.php', <<<LINTL
        <?php
        function {$name}(string \$name = '') {
            return <b class="badge">Hello, {\$name}</b>;
        }
        LINTL);

        return $dir;
    }
}
