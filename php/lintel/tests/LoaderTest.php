<?php

declare(strict_types=1);

namespace Lintel\Tests;

use Composer\Autoload\ClassLoader;
use Lintel\Dev\Loader;
use PHPUnit\Framework\TestCase;

use function file_put_contents;
use function getenv;
use function glob;
use function mkdir;
use function putenv;
use function sys_get_temp_dir;

use function chmod;
use function clearstatcache;
use function count;
use function filemtime;
use function filesize;
use function function_exists;
use function touch;

use const PHP_EOL;

/**
 * The §6.10 dev-loader corpus: PSR-4 fallback autoloading of `*.lintel.php`
 * sources, the stale-compile cache key, the `lintel.json` functions-glob
 * eager-require path (§6.10's T-31 note) and the proc_open compile fallback
 * (the T-12 daemon is gated, so the binary path is the active one).
 */
final class LoaderTest extends TestCase
{
    private string $tmp;
    private ?ClassLoader $registered = null;

    protected function setUp(): void
    {
        $this->tmp = sys_get_temp_dir() . '/lintel-loader-' . (getmypid() ?: '0')
            . '-' . bin2hex((string) random_bytes(4));
        mkdir($this->tmp . '/cache', 0777, true);
    }

    protected function tearDown(): void
    {
        if ($this->registered instanceof ClassLoader) {
            $this->registered->unregister();
            $this->registered = null;
        }
        putenv('LINTEL_BIN');
        self::rrmdir($this->tmp);
    }

    /** Register a real PSR-4 ClassLoader so `getRegisteredLoaders()` sees it. */
    private function registerPsr4(string $prefix, string $dir): void
    {
        // A real app's loader is constructed with its vendor dir — only
        // then does `ClassLoader::register()` record it in
        // `getRegisteredLoaders()`, which the §6.10 fallback scans.
        $loader = new ClassLoader($this->tmp . '/vendor');
        $loader->setPsr4($prefix, [$dir]);
        $loader->register(true);
        $this->registered = $loader;
    }

    private function put(string $rel, string $content): string
    {
        $path = $this->tmp . '/' . $rel;
        $dir = dirname($path);
        if (!is_dir($dir)) {
            mkdir($dir, 0777, true);
        }
        file_put_contents($path, $content);

        return $path;
    }

    private static function rrmdir(string $dir): void
    {
        if (!is_dir($dir)) {
            return;
        }
        foreach (glob($dir . '/*') ?: [] as $entry) {
            is_dir($entry) ? self::rrmdir($entry) : unlink($entry);
        }
        rmdir($dir);
    }

    public function testPsr4LintelSourcesAutoloadCompileAndExecute(): void
    {
        // Loader-mechanics scope: a plain-PHP class source with the
        // pass-through stand-in binary (`cat "$3"`). The *compiler* half of
        // this path is proven end-to-end by the real-binary manifest test
        // below and by scripts/dx-smoke.sh.
        $comp = $this->put('comp/View/Panel.lintel.php', <<<'PHP'
            <?php

            namespace Fixtures\View;

            final class Panel
            {
                public function render(): string
                {
                    return '<div class="panel"><p>panel body</p></div>';
                }
            }
            PHP);
        $this->registerPsr4('Fixtures\\', dirname($comp, 2));
        putenv('LINTEL_BIN=' . $this->fakeLintel());

        Loader::boot($this->tmp . '/cache');

        // The fixture class exists only after the loader compiles and
        // requires the cached file at runtime — invisible to static analysis.
        /** @phpstan-ignore class.notFound (runtime-defined via the compiled cache) */
        $panel = new \Fixtures\View\Panel();
        /** @phpstan-ignore class.notFound (runtime-defined via the compiled cache) */
        self::assertSame('<div class="panel"><p>panel body</p></div>', $panel->render());
    }

    /**
     * §6.10's cache key hashes only the path + mtime; a same-second edit is
     * then served stale forever. The key must carry the size too — this test
     * pins the fix (recorded as a §6.10 drift correction, architecture §21.2).
     */
    public function testCacheKeySurvivesSameMtimeDifferentSize(): void
    {
        $file = $this->put('src/Page.lintel.php', "<?php return 'v1';\n");
        putenv('LINTEL_BIN=' . $this->fakeLintel());

        self::assertSame('v1', Loader::require($file, $this->tmp . '/cache'));
        $first = count(glob($this->tmp . '/cache/*.php') ?: []);
        self::assertSame(1, $first);

        // Same mtime, different size/content: a doc-verbatim key would serve
        // the cached `v1` forever.
        $mtime = filemtime($file);
        self::assertNotFalse($mtime);
        file_put_contents($file, "<?php return 'v2-recompiled';\n\n");
        clearstatcache(true, $file);
        touch($file, $mtime);
        clearstatcache(true, $file);

        putenv('LINTEL_BIN=' . $this->fakeLintel());
        self::assertSame('v2-recompiled', Loader::require($file, $this->tmp . '/cache'));
        self::assertSame(2, count(glob($this->tmp . '/cache/*.php') ?: []));
    }

    public function testRepeatedRequireHitsTheCache(): void
    {
        $file = $this->put('src/One.lintel.php', "<?php return 'once';\n");
        putenv('LINTEL_BIN=' . $this->fakeLintel());

        self::assertSame('once', Loader::require($file, $this->tmp . '/cache'));
        $first = glob($this->tmp . '/cache/*.php') ?: [];
        self::assertSame(1, count($first));
        $mtime = filemtime($first[0]);
        self::assertNotFalse($mtime);
        clearstatcache(true, $first[0]);

        self::assertSame('once', Loader::require($file, $this->tmp . '/cache'));
        clearstatcache(true, $first[0]);
        self::assertSame($mtime, filemtime($first[0]), 'second require must not recompile');
    }

    public function testFunctionsManifestGlobsAreEagerRequired(): void
    {
        // The §6.10 T-31 note: function-only components are not PSR-4
        // autoloadable — the loader must eager-require the lintel.json
        // functions globs, or the demos never load. This variant pins the
        // plumbing with the pass-through stand-in binary.
        putenv('LINTEL_BIN=' . $this->fakeLintel());
        $this->put('proj/lintel.json', '{"functions": ["components/*.lintel.php"]}');
        $this->put('proj/components/hello.lintel.php', <<<'PHP'
            <?php

            namespace App\Ui;

            function hello(): int
            {
                return 7;
            }
            PHP);

        Loader::boot($this->tmp . '/cache', null, $this->tmp . '/proj');

        self::assertTrue(function_exists('\App\Ui\hello'), 'function-only components must load eagerly (§6.10 T-31 note)');
        self::assertSame(7, \App\Ui\hello());
    }

    public function testFunctionsManifestCompileRealMarkupWhenTheCompilerIsPresent(): void
    {
        $bin = getenv('LINTEL_TEST_BIN');
        if ($bin === false || $bin === '') {
            self::markTestSkipped('real-compiler lane: set LINTEL_TEST_BIN (CI and dx-smoke set it to the built lintel)');
        }
        putenv('LINTEL_BIN=' . $bin);
        $this->put('proj4/lintel.json', '{"functions": ["components/*.lintel.php"]}');
        $this->put('proj4/components/salute.lintel.php', <<<'PHP'
            <?php

            namespace App\Real;

            use Lintel\Html;

            function salute(string $name): Html
            {
                return <p class="greet">Hello, {$name}</p>;
            }
            PHP);

        Loader::boot($this->tmp . '/cache', null, $this->tmp . '/proj4');

        self::assertTrue(function_exists('\App\Real\salute'), 'a JSX function component must compile and load via the manifest');
        $plain = \App\Real\salute('Lintel');
        self::assertInstanceOf(\Lintel\Html::class, $plain, 'the compiled component returns the runtime Html');
        self::assertSame('<p class="greet">Hello, Lintel</p>', $plain->__toString());
        $hostile = \App\Real\salute('<script>');
        self::assertInstanceOf(\Lintel\Html::class, $hostile);
        self::assertSame(
            '<p class="greet">Hello, &lt;script&gt;</p>',
            $hostile->__toString(),
            'runtime escaping must hold for hostile component arguments',
        );
    }

    public function testFunctionsManifestAcceptsASingleGlobString(): void
    {
        putenv('LINTEL_BIN=' . $this->fakeLintel());
        $this->put('proj2/lintel.json', '{"functions": "components/one.lintel.php"}');
        $this->put('proj2/components/one.lintel.php', "<?php\n\nfunction one(): int { return 41 + 1; }\n");

        Loader::boot($this->tmp . '/cache', null, $this->tmp . '/proj2');

        self::assertTrue(function_exists('\one'));
        self::assertSame(42, \one());
    }

    public function testSocketParameterIsToleratedWithoutADaemon(): void
    {
        // The T-12 daemon is gated; a configured socket must not break the
        // proc_open fallback (§6.10: "daemon over unix socket, else ...").
        $file = $this->put('src/Sock.lintel.php', "<?php return 'via-fallback';\n");
        putenv('LINTEL_BIN=' . $this->fakeLintel());

        Loader::boot($this->tmp . '/cache', '/nonexistent/lintel.sock');

        self::assertSame('via-fallback', Loader::require($file, $this->tmp . '/cache'));
    }

    public function testCompileRunsTheBinaryThroughProcOpen(): void
    {
        $file = $this->put('src/Proc.lintel.php', "<?php return 'from-the-binary';\n");
        putenv('LINTEL_BIN=' . $this->fakeLintel());

        self::assertSame('from-the-binary', Loader::require($file, $this->tmp . '/cache'));
    }

    public function testCompileFailureSurfacesTheBinaryStderr(): void
    {
        $file = $this->put('src/Broken.lintel.php', "<?php return 'ignored';\n");
        $bin = $this->tmp . '/failing-lintel';
        file_put_contents($bin, "#!/bin/sh\necho 'boom: bad markup' >&2\nexit 1\n");
        chmod($bin, 0755);
        putenv('LINTEL_BIN=' . $bin);

        try {
            Loader::require($file, $this->tmp . '/cache');
            self::fail('a failed compile must throw');
        } catch (\RuntimeException $e) {
            self::assertStringContainsString('boom: bad markup', $e->getMessage());
            self::assertStringContainsString('lintel build', $e->getMessage());
        }
    }

    public function testMissingFunctionsManifestIsFine(): void
    {
        // No lintel.json in the root: boot stays silent and lazy.
        $this->expectNotToPerformAssertions();
        Loader::boot($this->tmp . '/cache', null, $this->tmp . '/empty-root');
    }

    public function testMalformedFunctionsManifestThrows(): void
    {
        $this->put('proj3/lintel.json', '{"functions": 42}');

        $this->expectException(\RuntimeException::class);
        $this->expectExceptionMessage('lintel.json');
        Loader::boot($this->tmp . '/cache', null, $this->tmp . '/proj3');
    }

    /**
     * A pass-through stand-in `lintel` binary: emits the source file
     * verbatim, so the loader's proc_open path (argument order, stdout
     * capture, exit-code handling) is exercised without the compiler.
     * Real compilation is asserted by the LINTEL_TEST_BIN lane.
     */
    private function fakeLintel(): string
    {
        $bin = $this->tmp . '/fake-lintel';
        // argv: <bin> build <file> — emit the source verbatim
        file_put_contents($bin, '#!/bin/sh' . PHP_EOL . 'cat -- "$2"' . PHP_EOL);
        chmod($bin, 0755);

        return $bin;
    }
}
