<?php

declare(strict_types=1);

namespace Lintl\Tests;

use const PHP_EOL;

use Composer\Autoload\ClassLoader;
use Fixtures\View\Panel;
use Lintl\Dev\Loader;
use Lintl\Html;
use PHPUnit\Framework\TestCase;

use function chmod;
use function clearstatcache;
use function count;
use function file_put_contents;
use function filemtime;
use function function_exists;
use function getenv;
use function glob;
use function mkdir;
use function putenv;
use function sys_get_temp_dir;
use function touch;

/**
 * The §6.10 dev-loader corpus: PSR-4 fallback autoloading of `*.lintl.php`
 * sources, the stale-compile cache key, the `lintl.json` functions-glob
 * eager-require path (§6.10's T-31 note) and the proc_open compile fallback
 * (the T-12 daemon is gated, so the binary path is the active one).
 */
final class LoaderTest extends TestCase
{
    private string $tmp;

    private ?ClassLoader $registered = null;

    /** @var resource|null */
    private $daemonResponder = null;

    protected function setUp(): void
    {
        $this->tmp = sys_get_temp_dir().'/lintl-loader-'.(getmypid() ?: '0')
            .'-'.bin2hex((string) random_bytes(4));
        mkdir($this->tmp.'/cache', 0777, true);
    }

    protected function tearDown(): void
    {
        if ($this->registered instanceof ClassLoader) {
            $this->registered->unregister();
            $this->registered = null;
        }
        putenv('LINTL_BIN');
        unset($GLOBALS['lintlFromSource']);
        if ($this->daemonResponder !== null) {
            $proc = $this->daemonResponder;
            $this->daemonResponder = null;
            proc_terminate($proc);
            proc_close($proc);
        }
        self::rrmdir($this->tmp);
    }

    /** Register a real PSR-4 ClassLoader so `getRegisteredLoaders()` sees it. */
    private function registerPsr4(string $prefix, string $dir): void
    {
        // A real app's loader is constructed with its vendor dir — only
        // then does `ClassLoader::register()` record it in
        // `getRegisteredLoaders()`, which the §6.10 fallback scans.
        $loader = new ClassLoader($this->tmp.'/vendor');
        $loader->setPsr4($prefix, [$dir]);
        $loader->register(true);
        $this->registered = $loader;
    }

    private function put(string $rel, string $content): string
    {
        $path = $this->tmp.'/'.$rel;
        $dir = dirname($path);
        if (! is_dir($dir)) {
            mkdir($dir, 0777, true);
        }
        file_put_contents($path, $content);

        return $path;
    }

    private static function rrmdir(string $dir): void
    {
        if (! is_dir($dir)) {
            return;
        }
        foreach (glob($dir.'/*') ?: [] as $entry) {
            is_dir($entry) ? self::rrmdir($entry) : unlink($entry);
        }
        rmdir($dir);
    }

    public function test_psr4_lintl_sources_autoload_compile_and_execute(): void
    {
        // Loader-mechanics scope: a plain-PHP class source with the
        // pass-through stand-in binary (`cat "$3"`). The *compiler* half of
        // this path is proven end-to-end by the real-binary manifest test
        // below and by scripts/dx-smoke.sh.
        $comp = $this->put('comp/View/Panel.lintl.php', <<<'PHP'
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
        putenv('LINTL_BIN='.$this->fakeLintl());

        Loader::boot($this->tmp.'/cache');

        // The fixture class exists only after the loader compiles and
        // requires the cached file at runtime — invisible to static analysis.
        /** @pun-ignore analyse/classes/invalid-new -- runtime-defined via the compiled cache */
        $panel = new Panel;
        /** @pun-ignore analyse/classes/invalid-new -- runtime-defined via the compiled cache */
        self::assertSame('<div class="panel"><p>panel body</p></div>', $panel->render());
    }

    /**
     * §6.10's cache key hashes only the path + mtime; a same-second edit is
     * then served stale forever. The key must carry the size too — this test
     * pins the fix (recorded as a §6.10 drift correction, architecture §21.2).
     */
    public function test_cache_key_survives_same_mtime_different_size(): void
    {
        $file = $this->put('src/Page.lintl.php', "<?php return 'v1';\n");
        putenv('LINTL_BIN='.$this->fakeLintl());

        self::assertSame('v1', Loader::require($file, $this->tmp.'/cache'));
        $first = count(glob($this->tmp.'/cache/*.php') ?: []);
        self::assertSame(1, $first);

        // Same mtime, different size/content: a doc-verbatim key would serve
        // the cached `v1` forever.
        $mtime = filemtime($file);
        self::assertNotFalse($mtime);
        file_put_contents($file, "<?php return 'v2-recompiled';\n\n");
        clearstatcache(true, $file);
        touch($file, $mtime);
        clearstatcache(true, $file);

        putenv('LINTL_BIN='.$this->fakeLintl());
        self::assertSame('v2-recompiled', Loader::require($file, $this->tmp.'/cache'));
        self::assertSame(2, count(glob($this->tmp.'/cache/*.php') ?: []));
    }

    public function test_repeated_require_hits_the_cache(): void
    {
        $file = $this->put('src/One.lintl.php', "<?php return 'once';\n");
        putenv('LINTL_BIN='.$this->fakeLintl());

        self::assertSame('once', Loader::require($file, $this->tmp.'/cache'));
        $first = glob($this->tmp.'/cache/*.php') ?: [];
        self::assertSame(1, count($first));
        $mtime = filemtime($first[0]);
        self::assertNotFalse($mtime);
        clearstatcache(true, $first[0]);

        self::assertSame('once', Loader::require($file, $this->tmp.'/cache'));
        clearstatcache(true, $first[0]);
        self::assertSame($mtime, filemtime($first[0]), 'second require must not recompile');
    }

    public function test_functions_manifest_globs_are_eager_required(): void
    {
        // The §6.10 T-31 note: function-only components are not PSR-4
        // autoloadable — the loader must eager-require the lintl.json
        // functions globs, or the demos never load. This variant pins the
        // plumbing with the pass-through stand-in binary.
        putenv('LINTL_BIN='.$this->fakeLintl());
        $this->put('proj/lintl.json', '{"functions": ["components/*.lintl.php"]}');
        $this->put('proj/components/hello.lintl.php', <<<'PHP'
            <?php

            namespace App\Ui;

            function hello(): int
            {
                return 7;
            }
            PHP);

        Loader::boot($this->tmp.'/cache', null, $this->tmp.'/proj');

        self::assertTrue(function_exists('\App\Ui\hello'), 'function-only components must load eagerly (§6.10 T-31 note)');
        self::assertSame(7, \App\Ui\hello());
    }

    public function test_boot_defines_function_components_before_the_fallback_registers(): void
    {
        // The T-31 order: the eager lane completes during boot() — PHP has
        // no function autoloading, so a function first called after boot can
        // only exist if the eager require already ran. The PSR-4 fallback
        // keeps working for class-shaped sources (the reorder must not
        // break the §6.10 class lane).
        putenv('LINTL_BIN='.$this->fakeLintl());
        $this->put('proj9/lintl.json', '{"functions": ["components/*.lintl.php"]}');
        $this->put('proj9/components/badge.lintl.php', "<?php\n\nnamespace App\\Ui;\n\nfunction badge(): int\n{\n    return 9;\n}\n");
        $comp = $this->put('proj9/components/Panel.lintl.php', <<<'PHP'
            <?php

            namespace Fixtures\Nine;

            final class Panel
            {
                public function render(): string
                {
                    return '<p>panel</p>';
                }
            }
            PHP);
        $this->registerPsr4('Fixtures\\', dirname($comp, 2));

        Loader::boot($this->tmp.'/cache', null, $this->tmp.'/proj9');

        self::assertTrue(function_exists('\App\Ui\badge'), 'the eager lane completed during boot');
        self::assertSame(9, \App\Ui\badge());
        /** @pun-ignore analyse/classes/invalid-new -- runtime-defined via the compiled cache */
        $panel = new \Fixtures\Nine\Panel;
        /** @pun-ignore analyse/classes/invalid-new -- runtime-defined via the compiled cache */
        self::assertSame('<p>panel</p>', $panel->render(), 'the PSR-4 fallback still loads class components');
    }

    public function test_functions_manifest_compile_real_markup_when_the_compiler_is_present(): void
    {
        $bin = getenv('LINTL_TEST_BIN');
        if ($bin === false || $bin === '') {
            self::markTestSkipped('real-compiler lane: set LINTL_TEST_BIN (CI and dx-smoke set it to the built lintl)');
        }
        putenv('LINTL_BIN='.$bin);
        $this->put('proj4/lintl.json', '{"functions": ["components/*.lintl.php"]}');
        $this->put('proj4/components/salute.lintl.php', <<<'PHP'
            <?php

            namespace App\Real;

            use Lintl\Html;

            function salute(string $name): Html
            {
                return <p class="greet">Hello, {$name}</p>;
            }
            PHP);

        Loader::boot($this->tmp.'/cache', null, $this->tmp.'/proj4');

        self::assertTrue(function_exists('\App\Real\salute'), 'a JSX function component must compile and load via the manifest');
        $plain = \App\Real\salute('Lintl');
        self::assertInstanceOf(Html::class, $plain, 'the compiled component returns the runtime Html');
        self::assertSame('<p class="greet">Hello, Lintl</p>', $plain->__toString());
        $hostile = \App\Real\salute('<script>');
        self::assertInstanceOf(Html::class, $hostile);
        self::assertSame(
            '<p class="greet">Hello, &lt;script&gt;</p>',
            $hostile->__toString(),
            'runtime escaping must hold for hostile component arguments',
        );
    }

    public function test_functions_manifest_accepts_a_single_glob_string(): void
    {
        putenv('LINTL_BIN='.$this->fakeLintl());
        $this->put('proj2/lintl.json', '{"functions": "components/one.lintl.php"}');
        $this->put('proj2/components/one.lintl.php', "<?php\n\nfunction one(): int { return 41 + 1; }\n");

        Loader::boot($this->tmp.'/cache', null, $this->tmp.'/proj2');

        self::assertTrue(function_exists('\one'));
        self::assertSame(42, \one());
    }

    public function test_boot_skips_sources_the_prod_shim_already_loaded(): void
    {
        // T-31 lane collision: when the prod files shim ran in this process
        // (it registers the sources it required in $GLOBALS['lintlFromSource']),
        // the eager lane must skip them — requiring the source again would
        // fatal on redeclare. Sources the shim does not know still load.
        putenv('LINTL_BIN='.$this->fakeLintl());
        $this->put('proj10/lintl.json', '{"functions": ["components/*.lintl.php"]}');
        $this->put('proj10/components/shimmed.lintl.php', "<?php\n\nnamespace App\\Shim;\n\nfunction shimmed(): int\n{\n    return 1;\n}\n");
        $this->put('proj10/components/fresh.lintl.php', "<?php\n\nnamespace App\\Fresh;\n\nfunction fresh(): int\n{\n    return 2;\n}\n");
        $GLOBALS['lintlFromSource'] = ['components/shimmed.lintl.php' => true];

        try {
            Loader::boot($this->tmp.'/cache', null, $this->tmp.'/proj10');
        } finally {
            unset($GLOBALS['lintlFromSource']);
        }

        self::assertFalse(
            function_exists('\App\Shim\shimmed'),
            'a source the prod shim loaded must be skipped, not redeclared'
        );
        self::assertTrue(function_exists('\App\Fresh\fresh'), 'unshimmed sources still load eagerly');
    }

    public function test_socket_parameter_is_tolerated_without_a_daemon(): void
    {
        // The T-12 daemon is gated; a configured socket must not break the
        // proc_open fallback (§6.10: "daemon over unix socket, else ...").
        $file = $this->put('src/Sock.lintl.php', "<?php return 'via-fallback';\n");
        putenv('LINTL_BIN='.$this->fakeLintl());

        Loader::boot($this->tmp.'/cache', '/nonexistent/lintl.sock');

        self::assertSame('via-fallback', Loader::require($file, $this->tmp.'/cache'));
    }

    /**
     * The §6.10 daemon handoff (T-12): with `$socket` configured and a
     * live daemon behind it, the compile goes over the socket — a failing
     * `lintl` binary proves the spawn fallback never ran.
     */
    public function test_compile_uses_the_daemon_socket_when_it_answers(): void
    {
        $file = $this->put('src/Daemon.lintl.php', "<?php return 'ignored-source';\n");
        putenv('LINTL_BIN='.$this->failingLintl());
        $socket = $this->tmp.'/d.sock';
        $this->startDaemonResponder($socket, 'ok');

        Loader::boot($this->tmp.'/cache', $socket, $this->tmp);

        self::assertSame('from-daemon', Loader::require($file, $this->tmp.'/cache'));
    }

    public function test_compile_surfaces_the_daemon_err_as_a_diagnostic(): void
    {
        $file = $this->put('src/Broken2.lintl.php', "<?php return 'ignored';\n");
        putenv('LINTL_BIN='.$this->failingLintl());
        $socket = $this->tmp.'/d.sock';
        $this->startDaemonResponder($socket, 'err');

        Loader::boot($this->tmp.'/cache', $socket, $this->tmp);

        try {
            Loader::require($file, $this->tmp.'/cache');
            self::fail('a daemon ERR must throw');
        } catch (\RuntimeException $e) {
            self::assertStringContainsString('rendered daemon diagnostics', $e->getMessage());
            self::assertStringContainsString('Broken2.lintl.php', $e->getMessage());
        }
    }

    /**
     * A stand-in daemon that answers `ok`/`err` over the socket path.
     * The child lingers up to 5 idle seconds; tearDown terminates it.
     */
    private function startDaemonResponder(string $socket, string $mode): void
    {
        $proc = proc_open(
            [PHP_BINARY, __DIR__.'/fixtures/daemon-responder.php', $socket, $mode],
            [1 => ['pipe', 'w'], 2 => ['pipe', 'w']],
            $pipes,
        );
        if (! is_resource($proc)) {
            self::markTestSkipped('could not start the daemon responder');
        }
        foreach ($pipes as $pipe) {
            fclose($pipe);
        }
        $this->daemonResponder = $proc;
        // Wait for the socket to appear (the child binds it on start).
        for ($i = 0; $i < 50 && ! file_exists($socket); $i++) {
            usleep(20_000);
        }
        self::assertFileExists($socket, 'the responder never bound the socket');
    }

    /** A `lintl` stand-in that fails — the socket lane must never use it. */
    private function failingLintl(): string
    {
        $bin = $this->tmp.'/failing-lintl';
        file_put_contents($bin, "#!/bin/sh\necho 'the fallback must not run' >&2\nexit 1\n");
        chmod($bin, 0755);

        return $bin;
    }

    public function test_compile_runs_the_binary_through_proc_open(): void
    {
        $file = $this->put('src/Proc.lintl.php', "<?php return 'from-the-binary';\n");
        putenv('LINTL_BIN='.$this->fakeLintl());

        self::assertSame('from-the-binary', Loader::require($file, $this->tmp.'/cache'));
    }

    public function test_compile_failure_surfaces_the_binary_stderr(): void
    {
        $file = $this->put('src/Broken.lintl.php', "<?php return 'ignored';\n");
        $bin = $this->tmp.'/failing-lintl';
        file_put_contents($bin, "#!/bin/sh\necho 'boom: bad markup' >&2\nexit 1\n");
        chmod($bin, 0755);
        putenv('LINTL_BIN='.$bin);

        try {
            Loader::require($file, $this->tmp.'/cache');
            self::fail('a failed compile must throw');
        } catch (\RuntimeException $e) {
            self::assertStringContainsString('boom: bad markup', $e->getMessage());
            self::assertStringContainsString('lintl build', $e->getMessage());
        }
    }

    public function test_missing_functions_manifest_is_fine(): void
    {
        // No lintl.json in the root: boot stays silent and lazy.
        $this->expectNotToPerformAssertions();
        Loader::boot($this->tmp.'/cache', null, $this->tmp.'/empty-root');
    }

    public function test_malformed_functions_manifest_throws(): void
    {
        $this->put('proj3/lintl.json', '{"functions": 42}');

        $this->expectException(\RuntimeException::class);
        $this->expectExceptionMessage('lintl.json');
        Loader::boot($this->tmp.'/cache', null, $this->tmp.'/proj3');
    }

    /**
     * A pass-through stand-in `lintl` binary: emits the source file
     * verbatim, so the loader's proc_open path (argument order, stdout
     * capture, exit-code handling) is exercised without the compiler.
     * Real compilation is asserted by the LINTL_TEST_BIN lane.
     */
    private function fakeLintl(): string
    {
        $bin = $this->tmp.'/fake-lintl';
        // argv: <bin> build <file> — emit the source verbatim
        file_put_contents($bin, '#!/bin/sh'.PHP_EOL.'cat -- "$2"'.PHP_EOL);
        chmod($bin, 0755);

        return $bin;
    }
}
