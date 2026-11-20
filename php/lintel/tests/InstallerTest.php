<?php

declare(strict_types=1);

namespace Lintel\Tests;

use Composer\IO\BufferIO;
use Lintel\Installer\BinaryInstaller;
use Lintel\Installer\InstallerException;
use PHPUnit\Framework\TestCase;

use function chmod;
use function file_get_contents;
use function file_put_contents;
use function hash_file;
use function is_executable;
use function mkdir;
use function proc_close;
use function proc_get_status;
use function proc_open;
use function random_int;
use function str_repeat;
use function sys_get_temp_dir;
use function usleep;

use const PHP_EOL;

/**
 * §12's installer path, exercised against a real fixture URL: the plugin
 * downloads the binary, verifies the SHA-256, and caches it executable at
 * the destination — or refuses without leaving debris. A `php -S` server
 * serves the fixture directory, so no live release URL is invented (the
 * real ones arrive with T-18).
 */
final class InstallerTest extends TestCase
{
    private static ?string $baseUrl = null;
    /** @var resource|null */
    private static $server = null;

    public static function setUpBeforeClass(): void
    {
        for ($attempt = 0; $attempt < 8 && self::$baseUrl === null; $attempt++) {
            $port = random_int(21000, 44000);
            $proc = @proc_open(
                [PHP_BINARY, '-S', '127.0.0.1:' . $port, '-t', __DIR__ . '/fixtures/http'],
                [1 => ['pipe', 'w'], 2 => ['pipe', 'w']],
                $pipes,
            );
            if (! is_resource($proc)) {
                continue;
            }
            // Close the parent-side log pipes immediately: the server's
            // startup banner cannot fill them, and no writer survives the
            // run to hold an output pipe open.
            foreach ($pipes as $pipe) {
                fclose($pipe);
            }
            usleep(200_000);
            $status = proc_get_status($proc);
            if ($status['running']) {
                self::$server = $proc;
                self::$baseUrl = 'http://127.0.0.1:' . $port;
                // Kill on process exit too — even a fatal error mid-suite
                // must not orphan the server (it would hold output pipes).
                register_shutdown_function(static function () use ($proc): void {
                    if (self::$server === $proc) {
                        self::$server = null;
                        proc_terminate($proc);
                        proc_close($proc);
                    }
                });
                break;
            } else {
                proc_close($proc);
            }
        }
        if (self::$baseUrl === null) {
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

    private function tmp(): string
    {
        $dir = sys_get_temp_dir() . '/lintel-installer-' . bin2hex((string) random_bytes(4));
        mkdir($dir . '/vendor/bin', 0777, true);

        return $dir;
    }

    private function fixtureSha(): string
    {
        $sha = hash_file('sha256', __DIR__ . '/fixtures/http/lintel-fake-bin');
        self::assertNotFalse($sha);

        return $sha;
    }

    public function testDownloadVerifiesAndCachesTheBinary(): void
    {
        $dir = $this->tmp();
        $dest = $dir . '/vendor/bin/lintel';

        $installed = BinaryInstaller::downloadVerified(
            self::$baseUrl . '/lintel-fake-bin',
            $this->fixtureSha(),
            $dest,
        );

        self::assertSame($dest, $installed);
        self::assertFileExists($dest);
        self::assertTrue(is_executable($dest), 'the cached binary must be executable');
        self::assertFileEquals(__DIR__ . '/fixtures/http/lintel-fake-bin', $dest);
    }

    public function testChecksumMismatchRefusesAndCachesNothing(): void
    {
        $dir = $this->tmp();
        $dest = $dir . '/vendor/bin/lintel';
        $wrong = str_repeat('0', 64);

        try {
            BinaryInstaller::downloadVerified(self::$baseUrl . '/lintel-fake-bin', $wrong, $dest);
            self::fail('a checksum mismatch must refuse the binary');
        } catch (InstallerException $e) {
            self::assertStringContainsStringIgnoringCase('sha-256', $e->getMessage());
        }

        self::assertFileDoesNotExist($dest);
        self::assertFileDoesNotExist($dest . '.tmp');
    }

    public function testUnavailableSourceIsReportedCleanly(): void
    {
        $dir = $this->tmp();
        $dest = $dir . '/vendor/bin/lintel';

        try {
            BinaryInstaller::downloadVerified(self::$baseUrl . '/no-such-binary', $this->fixtureSha(), $dest);
            self::fail('a failed download must throw');
        } catch (InstallerException $e) {
            self::assertStringContainsString('no-such-binary', $e->getMessage());
        }

        self::assertFileDoesNotExist($dest);
        self::assertFileDoesNotExist($dest . '.tmp');
    }

    public function testOverrideInstallsLocallyWithoutNetwork(): void
    {
        $dir = $this->tmp();
        $dest = $dir . '/vendor/bin/lintel';

        $installed = BinaryInstaller::installLocal(__DIR__ . '/fixtures/http/lintel-fake-bin', $dest);

        self::assertSame($dest, $installed);
        self::assertTrue(is_executable($dest));
        self::assertFileEquals(__DIR__ . '/fixtures/http/lintel-fake-bin', $dest);
    }

    public function testResolvePrefersTheLinTelBinOverride(): void
    {
        $dir = $this->tmp();
        putenv('LINTEL_BIN=' . __DIR__ . '/fixtures/http/lintel-fake-bin');

        try {
            $resolved = BinaryInstaller::resolve(
                ['lintel' => ['url' => self::$baseUrl . '/lintel-fake-bin', 'sha256' => $this->fixtureSha()]],
                $dir . '/vendor/bin',
                new BufferIO(),
            );
        } finally {
            putenv('LINTEL_BIN');
        }

        self::assertSame($dir . '/vendor/bin/lintel', $resolved);
        self::assertFileExists($dir . '/vendor/bin/lintel');
    }

    public function testResolveDownloadsWhenExtraPinsSourceAndChecksum(): void
    {
        $dir = $this->tmp();

        $resolved = BinaryInstaller::resolve(
            ['lintel' => ['url' => self::$baseUrl . '/lintel-fake-bin', 'sha256' => $this->fixtureSha()]],
            $dir . '/vendor/bin',
            new BufferIO(),
        );

        self::assertSame($dir . '/vendor/bin/lintel', $resolved);
        self::assertTrue(is_executable($dir . '/vendor/bin/lintel'));
    }

    public function testResolveWithoutASourceNotifiesAndInstallsNothing(): void
    {
        $dir = $this->tmp();
        putenv('LINTEL_BIN');

        $io = new BufferIO();
        $resolved = BinaryInstaller::resolve([], $dir . '/vendor/bin', $io);

        self::assertNull($resolved, 'no invented release URL: T-18 owns the live ones');
        self::assertFileDoesNotExist($dir . '/vendor/bin/lintel');
        self::assertStringContainsStringIgnoringCase('LINTEL_BIN', $io->getOutput());
        self::assertStringContainsStringIgnoringCase('extra.lintel', $io->getOutput());
    }

    public function testResolveRejectsAUrlWithoutAChecksum(): void
    {
        $dir = $this->tmp();

        $this->expectException(InstallerException::class);
        $this->expectExceptionMessage('sha256');
        BinaryInstaller::resolve(
            ['lintel' => ['url' => self::$baseUrl . '/lintel-fake-bin']],
            $dir . '/vendor/bin',
            new BufferIO(),
        );
    }
}
