<?php

declare(strict_types=1);

namespace Lintel\Tests;

use Composer\Composer;
use Composer\Config;
use Composer\EventDispatcher\EventSubscriberInterface;
use Composer\IO\BufferIO;
use Composer\IO\IOInterface;
use Composer\Package\RootPackage;
use Composer\Plugin\PluginInterface;
use Composer\Script\Event;
use Composer\Script\ScriptEvents;
use Lintel\Installer\Plugin;
use PHPUnit\Framework\TestCase;

use function chmod;
use function file_put_contents;
use function hash_file;
use function mkdir;
use function proc_open;
use function putenv;
use function random_int;
use function sys_get_temp_dir;
use function usleep;

/**
 * The composer-plugin surface (§12): the plugin subscribes to
 * `post-autoload-dump` and installs the lintel binary into
 * `<vendor>/bin/lintel` — from the `LINTEL_BIN` override or from the
 * `extra.lintel` source + checksum, never from an invented URL.
 */
final class PluginTest extends TestCase
{
    private static ?string $baseUrl = null;
    /** @var resource|null */
    private static $server = null;

    public static function setUpBeforeClass(): void
    {
        // Same fixture lane as InstallerTest; a second tiny server keeps the
        // suites independent.
        $port = random_int(45000, 59999);
        $proc = proc_open(
            [PHP_BINARY, '-S', '127.0.0.1:' . $port, '-t', __DIR__ . '/fixtures/http'],
            [1 => ['pipe', 'w'], 2 => ['pipe', 'w']],
            $pipes,
        );
        if (! is_resource($proc)) {
            self::markTestSkipped('could not start the fixture `php -S` server');
        }
        // Same pipe hygiene as InstallerTest: no parent-side writer survives.
        foreach ($pipes as $pipe) {
            fclose($pipe);
        }
        usleep(200_000);
        $status = proc_get_status($proc);
        if ($status['running']) {
            self::$server = $proc;
            self::$baseUrl = 'http://127.0.0.1:' . $port;
            register_shutdown_function(static function () use ($proc): void {
                if (self::$server === $proc) {
                    self::$server = null;
                    proc_terminate($proc);
                    proc_close($proc);
                }
            });
        } else {
            proc_close($proc);
            self::markTestSkipped('could not start the fixture `php -S` server');
        }
    }

    public static function tearDownAfterClass(): void
    {
        if (self::$server !== null) {
            $proc = self::$server;
            self::$server = null;
            proc_terminate($proc);
            proc_close($proc);
        }
    }

    protected function tearDown(): void
    {
        putenv('LINTEL_BIN');
        unset($GLOBALS['lintelProdArtifacts'], $GLOBALS['lintelFromSource']);
    }

    public function testSubscribesToPostAutoloadDump(): void
    {
        self::assertInstanceOf(PluginInterface::class, new Plugin());
        self::assertInstanceOf(EventSubscriberInterface::class, new Plugin());

        $events = Plugin::getSubscribedEvents();
        self::assertArrayHasKey(ScriptEvents::POST_AUTOLOAD_DUMP, $events);
        self::assertSame('installBinary', $events[ScriptEvents::POST_AUTOLOAD_DUMP]);
    }

    public function testInstallsFromTheOverrideOnPostAutoloadDump(): void
    {
        $project = $this->projectDir();
        $fake = $project . '/fake-lintel';
        file_put_contents($fake, "#!/bin/sh\necho fake\n");
        chmod($fake, 0755);
        putenv('LINTEL_BIN=' . $fake);

        $plugin = new Plugin();
        $composer = $this->composer($project, []);
        $io = new BufferIO();
        $plugin->activate($composer, $io);
        $plugin->installBinary($this->dumpEvent($composer, $io));

        self::assertFileExists($project . '/vendor/bin/lintel');
        self::assertTrue(is_executable($project . '/vendor/bin/lintel'));
    }

    public function testInstallsFromExtraUrlAndChecksumOnPostAutoloadDump(): void
    {
        $project = $this->projectDir();
        $sha = hash_file('sha256', __DIR__ . '/fixtures/http/lintel-fake-bin');
        self::assertNotFalse($sha);

        $plugin = new Plugin();
        $composer = $this->composer($project, [
            'lintel' => ['url' => self::$baseUrl . '/lintel-fake-bin', 'sha256' => $sha],
        ]);
        $io = new BufferIO();
        $plugin->activate($composer, $io);
        $plugin->installBinary($this->dumpEvent($composer, $io));

        self::assertFileExists($project . '/vendor/bin/lintel');
        self::assertStringContainsString('vendor/bin/lintel', $io->getOutput());
    }

    public function testWithoutASourceItNotifiesButDoesNotFail(): void
    {
        $project = $this->projectDir();

        $plugin = new Plugin();
        $composer = $this->composer($project, []);
        $io = new BufferIO();
        $plugin->activate($composer, $io);
        $plugin->installBinary($this->dumpEvent($composer, $io));

        self::assertFileDoesNotExist($project . '/vendor/bin/lintel');
        self::assertStringContainsStringIgnoringCase('LINTEL_BIN', $io->getOutput());
    }


    public function testSubscribesToPreAutoloadDump(): void
    {
        $events = Plugin::getSubscribedEvents();
        self::assertArrayHasKey(ScriptEvents::PRE_AUTOLOAD_DUMP, $events);
        self::assertSame('injectFunctionAutoload', $events[ScriptEvents::PRE_AUTOLOAD_DUMP]);
    }

    public function testInjectsFilesAutoloadFromTheProdManifest(): void
    {
        // T-31: the prod lane. `lintel build --manifest` wrote `prod.files`;
        // the plugin generates <vendor>/lintel/functions.php requiring every
        // artifact and appends it to the root package's `files` autoload.
        $project = $this->projectDir();
        mkdir($project . '/build', 0777, true);
        file_put_contents(
            $project . '/build/Card.php',
            "<?php\n\nnamespace Smoke;\n\nfunction prodCard(): string\n{\n    return 'from-prod-artifact';\n}\n"
        );
        file_put_contents(
            $project . '/lintel.json',
            '{"functions": ["src/*.lintel.php"], '
            . '"prod": {"artifacts": {"src/Card.lintel.php": "build/Card.php"}}}'
        );

        $plugin = new Plugin();
        $composer = $this->composer($project, []);
        $io = new BufferIO();
        $plugin->activate($composer, $io);
        $plugin->injectFunctionAutoload($this->preloadDumpEvent($composer, $io));

        self::assertFileExists($project . '/vendor/lintel/functions.php');
        $generated = (string) file_get_contents($project . '/vendor/lintel/functions.php');
        self::assertStringContainsString("dirname(__DIR__, 2)", $generated, 'the shim resolves artifacts against the project root');
        self::assertStringContainsString("'src/Card.lintel.php' => 'build/Card.php'", $generated, 'the shim carries the source-artifact pair');
        self::assertStringContainsString("lintelFromSource", $generated, 'the shim records the sources it loaded');
        self::assertStringNotContainsString($project, $generated, 'no absolute path leaks into the shim');

        $autoload = $composer->getPackage()->getAutoload();
        self::assertContains('vendor/lintel/functions.php', $autoload['files'] ?? []);

        // The shim itself works: requiring it (as Composer's files autoload
        // would) defines the function — no manual require, no dev loader.
        require $project . '/vendor/lintel/functions.php';
        self::assertTrue(function_exists('\Smoke\prodCard'));
        self::assertSame('from-prod-artifact', \Smoke\prodCard());
        self::assertStringContainsString('function-component artifact', $io->getOutput());
    }

    public function testInjectionIsIdempotentAcrossRepeatedDumps(): void
    {
        $project = $this->projectDir();
        file_put_contents(
            $project . '/lintel.json',
            '{"prod": {"artifacts": {"src/A.lintel.php": "build/A.php"}}}'
        );

        $plugin = new Plugin();
        $composer = $this->composer($project, []);
        $io = new BufferIO();
        $plugin->activate($composer, $io);
        $plugin->injectFunctionAutoload($this->preloadDumpEvent($composer, $io));
        $plugin->injectFunctionAutoload($this->preloadDumpEvent($composer, $io));

        $files = $composer->getPackage()->getAutoload()['files'] ?? [];
        self::assertCount(1, $files, 'a second dump must not duplicate the entry');
        self::assertSame(['vendor/lintel/functions.php'], $files);
    }

    public function testWithoutAProdSectionTheInjectionIsASilentNoOp(): void
    {
        // A dev-only manifest (functions globs, no prod) injects nothing —
        // and is not an error: the plugin ran fine before T-31 too.
        $project = $this->projectDir();
        file_put_contents($project . '/lintel.json', '{"functions": ["src/*.lintel.php"]}');

        $plugin = new Plugin();
        $composer = $this->composer($project, []);
        $io = new BufferIO();
        $plugin->activate($composer, $io);
        $plugin->injectFunctionAutoload($this->preloadDumpEvent($composer, $io));

        self::assertFileDoesNotExist($project . '/vendor/lintel/functions.php');
        $autoload = $composer->getPackage()->getAutoload();
        self::assertArrayNotHasKey('files', $autoload, 'no files entry without a prod section');
    }

    public function testAMalformedProdSectionIsAnIoNoticeNotAFailure(): void
    {
        // composer install must never fail over the manifest: a malformed
        // `prod.files` is a notice on the error stream and a no-op.
        $project = $this->projectDir();
        file_put_contents($project . '/lintel.json', '{"prod": {"artifacts": 42}}');

        $plugin = new Plugin();
        $composer = $this->composer($project, []);
        $io = new BufferIO();
        $plugin->activate($composer, $io);
        $plugin->injectFunctionAutoload($this->preloadDumpEvent($composer, $io));

        self::assertFileDoesNotExist($project . '/vendor/lintel/functions.php');
        self::assertStringContainsString('skipping the prod function autoload', $io->getOutput());
        $autoload = $composer->getPackage()->getAutoload();
        self::assertArrayNotHasKey('files', $autoload);
    }

    public function testAMalformedArtifactValueIsAnIoNoticeNotAFailure(): void
    {
        $project = $this->projectDir();
        file_put_contents(
            $project . '/lintel.json',
            '{"prod": {"artifacts": {"src/Card.lintel.php": 42}}}'
        );

        $plugin = new Plugin();
        $composer = $this->composer($project, []);
        $io = new BufferIO();
        $plugin->activate($composer, $io);
        $plugin->injectFunctionAutoload($this->preloadDumpEvent($composer, $io));

        self::assertFileDoesNotExist($project . '/vendor/lintel/functions.php');
        self::assertStringContainsString('skipping the prod function autoload', $io->getOutput());
    }

    public function testAMissingManifestIsASilentNoOp(): void
    {
        $this->expectNotToPerformAssertions();
        $plugin = new Plugin();
        $composer = $this->composer($this->projectDir(), []);
        $io = new BufferIO();
        $plugin->activate($composer, $io);
        $plugin->injectFunctionAutoload($this->preloadDumpEvent($composer, $io));
    }

    /** A minimal composer object rooted at $project with the given extra.
     *
     * @param array<string, mixed> $extra
     */
    private function composer(string $project, array $extra): Composer
    {
        $composer = new Composer();
        $config = new Config(false, $project);
        $composer->setConfig($config);
        $package = new RootPackage('lintel/smoke-root', '1.0.0', '1.0.0');
        $package->setExtra($extra);
        $composer->setPackage($package);

        return $composer;
    }

    private function dumpEvent(Composer $composer, IOInterface $io): Event
    {
        return new Event(ScriptEvents::POST_AUTOLOAD_DUMP, $composer, $io);
    }

    private function preloadDumpEvent(Composer $composer, IOInterface $io): Event
    {
        return new Event(ScriptEvents::PRE_AUTOLOAD_DUMP, $composer, $io);
    }

    private function projectDir(): string
    {
        $dir = sys_get_temp_dir() . '/lintel-plugin-' . bin2hex((string) random_bytes(4));
        mkdir($dir . '/vendor/bin', 0777, true);

        return $dir;
    }
}
