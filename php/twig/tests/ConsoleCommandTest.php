<?php

declare(strict_types=1);

namespace Lintel\Twig\Tests;

use Lintel\Twig\Console\BuildCommand;
use PHPUnit\Framework\TestCase;
use Symfony\Component\Console\Input\ArrayInput;
use Symfony\Component\Console\Output\BufferedOutput;

/**
 * `bin/console lintel:build` (§11 Symfony wrapper, §17.1-Q2's remaining
 * Symfony surface): the command executes through the real Symfony
 * `run()` lane `bin/console` itself drives, with explicit paths standing
 * in for the kernel extension's configuration — artifacts into the cache
 * dir + the T-31 manifest, diagnostics mapping the binary's exit to the
 * command's.
 */
final class ConsoleCommandTest extends TestCase
{
    /** `lintel:build` compiles the fixture: artifacts + manifest + exit 0. */
    public function testBuildCommandCompilesIntoCacheDir(): void
    {
        $root = sys_get_temp_dir() . '/lintel-sf-console-' . bin2hex((string) random_bytes(4));
        \mkdir($root . '/src', 0777, true);
        \copy(__DIR__ . '/fixture/Badge.lintel.php', $root . '/src/Badge.lintel.php');

        $back = \getcwd() ?: '.';
        \chdir($root); // `--manifest` (bare) writes lintel.json to the cwd — console runs from the project root
        try {
            $cmd = new BuildCommand($root . '/cache', [$root . '/src']);
            $rc = $cmd->run(new ArrayInput([]), $out = new BufferedOutput());
        } finally {
            \chdir($back);
        }

        self::assertSame(0, $rc, $out->fetch());
        self::assertFileExists($root . '/cache/Badge.php');
        self::assertFileExists($root . '/lintel.json');
    }

    /** The binary's error diagnostics map to FAILURE and render RAW — the
     * `<` markup of the diagnostic survives into the output. */
    public function testBuildCommandFailsOnBrokenSourceWithRawDiagnostics(): void
    {
        $root = sys_get_temp_dir() . '/lintel-sf-console-' . bin2hex((string) random_bytes(4));
        \mkdir($root . '/src', 0777, true);
        \file_put_contents($root . '/src/Broken.lintel.php', '<?php function Broken({a}: string) { return <div>{a}</span>; }');

        $cmd = new BuildCommand($root . '/cache', [$root . '/src']);
        $rc = $cmd->run(new ArrayInput([]), $out = new BufferedOutput());
        $text = $out->fetch();

        self::assertSame(1, $rc);
        self::assertStringContainsString('LNT0102', $text);
        self::assertStringContainsString('</span>', $text, 'diagnostics render RAW (no formatter mangling)');
    }

    /** The command carries the §11 signature for `bin/console list`. */
    public function testCommandNameAndDescription(): void
    {
        $cmd = new BuildCommand(sys_get_temp_dir(), []);

        self::assertSame('lintel:build', $cmd->getName());
        self::assertStringContainsString('Compile the Lintel sources', $cmd->getDescription());
    }
}
