<?php

declare(strict_types=1);

namespace Lintel\Tests;

use Lintel\PhpStan\Bridge;
use PHPUnit\Framework\TestCase;

use function array_map;
use function dirname;
use function getenv;
use function glob;
use function is_executable;
use function sprintf;
use function sys_get_temp_dir;
use function uniqid;

/**
 * The bridge's compile-and-record contracts, against the real compiler
 * binary (the single source of truth): the metadata sidecar carries the
 * compiler's own JSON diagnostics, and the signature stub carries the
 * scanner's proven facts — parameter names, required-ness, `string`
 * types, `\Lintel\Html` return.
 */
final class BridgeTest extends TestCase
{
    private string $cacheDir;

    public static function setUpBeforeClass(): void
    {
        $binary = getenv(Bridge::BINARY_ENV);
        if (is_string($binary) && $binary !== '' && is_executable($binary)) {
            return;
        }
        self::markTestSkipped(sprintf(
            'the lintel binary is not available: set %s (CI builds it; locally: cargo build -p lintel-cli)',
            Bridge::BINARY_ENV,
        ));
    }

    protected function setUp(): void
    {
        $this->cacheDir = sys_get_temp_dir().'/lintel-phpstan-'.uniqid();
    }

    protected function tearDown(): void
    {
        foreach (glob($this->cacheDir.'/*') ?: [] as $file) {
            @unlink((string) $file);
        }
        @rmdir($this->cacheDir);
    }

    public function test_prepare_records_the_compiler_diagnostics_verbatim(): void
    {
        $prepared = Bridge::prepare(dirname(__DIR__).'/tests/fixture/Diagnostics.lintel.php', $this->cacheDir);
        $metadata = Bridge::metadataFor($this->cacheDir.'/Diagnostics.php');
        self::assertNotNull($metadata, 'the sidecar exists beside the artifact shell');
        self::assertSame(dirname(__DIR__).'/tests/fixture/Diagnostics.lintel.php', $metadata['source']);
        self::assertNull($metadata['artifact'], 'a failing source produces no compiled artifact');
        // The diagnostics are the bridge's validated shape — `code` is
        // proven by `Bridge::decodeDiagnostic`, no re-narrowing here.
        $codes = array_map(static fn (array $diagnostic): string => $diagnostic['code'], $metadata['diagnostics']);
        self::assertContains('LNT0205', $codes, 'the missing-required rejection is recorded');
        self::assertContains('LNT0203', $codes, 'the unknown-prop rejection is recorded');
        self::assertSame('', $prepared->artifact);
        self::assertFileExists($prepared->metadata);
    }

    public function test_prepare_compiles_a_clean_source_and_records_no_diagnostics(): void
    {
        $prepared = Bridge::prepare(dirname(__DIR__).'/tests/fixture/Types.lintel.php', $this->cacheDir);
        self::assertSame($this->cacheDir.'/Types.php', $prepared->artifact, '`build --out` drops the `.lintel` infix');
        self::assertFileExists($prepared->artifact, 'a clean source compiles through the same `lintel build` lane');
        $metadata = Bridge::metadataFor($prepared->artifact);
        self::assertNotNull($metadata);
        self::assertSame([], $metadata['diagnostics'], 'the clean source carries no LNT diagnostics');
    }

    public function test_stub_carries_the_scanner_proven_facts(): void
    {
        $prepared = Bridge::prepare(dirname(__DIR__).'/tests/fixture/Types.lintel.php', $this->cacheDir);
        self::assertNotSame('', $prepared->stub);
        $stub = (string) file_get_contents($prepared->stub);
        // The source's namespace, the proven `string` type, required-ness
        // (no default), the optional `children`, and the `Html` return.
        // `$count` stays untyped: the scanner proves only `string` (§7.3),
        // and the stub stays as conservative as the compiler.
        self::assertStringContainsString('namespace Lintel\\Fixtures;', $stub, $stub);
        self::assertStringContainsString('function Card(string $title, $count, $children = null): \\Lintel\\Html {}', $stub, $stub);
    }

    public function test_ordinary_php_files_have_no_metadata(): void
    {
        self::assertNull(Bridge::metadataFor(__FILE__), 'the rule costs ordinary PHP files nothing');
    }
}
