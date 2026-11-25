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
    /** `lintel:build` compiles the fixture: artifacts + manifest + exit 0.
     *
     * The M0 cwd-hygiene regression: the bare `--manifest` write resolves
     * against the workspace being compiled (the src/… + cache/ ancestor —
     * the project root), never the process cwd, so this runs from wherever
     * PHPUnit sits and the manifest must still land at `$root` with no
     * stray `lintel.json` at the runner's cwd. */
    public function test_build_command_compiles_into_cache_dir(): void
    {
        $root = sys_get_temp_dir().'/lintel-sf-console-'.bin2hex((string) random_bytes(4));
        \mkdir($root.'/src', 0777, true);
        \copy(__DIR__.'/fixture/Badge.lintel.php', $root.'/src/Badge.lintel.php');

        $runnerCwd = \getcwd() ?: '.';
        $cmd = new BuildCommand($root.'/cache', [$root.'/src']);
        $rc = $cmd->run(new ArrayInput([]), $out = new BufferedOutput);

        self::assertSame(0, $rc, $out->fetch());
        self::assertFileExists($root.'/cache/Badge.php');
        self::assertFileExists($root.'/lintel.json', 'the manifest lands at the workspace root');
        self::assertFileDoesNotExist(
            $runnerCwd.'/lintel.json',
            'no stray manifest at the process cwd (M0 cwd hygiene)',
        );
    }

    /** The binary's error diagnostics map to FAILURE and render RAW — the
     * `<` markup of the diagnostic survives into the output. */
    public function test_build_command_fails_on_broken_source_with_raw_diagnostics(): void
    {
        $root = sys_get_temp_dir().'/lintel-sf-console-'.bin2hex((string) random_bytes(4));
        \mkdir($root.'/src', 0777, true);
        \file_put_contents($root.'/src/Broken.lintel.php', '<?php function Broken({a}: string) { return <div>{a}</span>; }');

        $cmd = new BuildCommand($root.'/cache', [$root.'/src']);
        $rc = $cmd->run(new ArrayInput([]), $out = new BufferedOutput);
        $text = $out->fetch();

        self::assertSame(1, $rc);
        self::assertStringContainsString('LNT0102', $text);
        self::assertStringContainsString('</span>', $text, 'diagnostics render RAW (no formatter mangling)');
    }

    /** The command carries the §11 signature for `bin/console list`. */
    public function test_command_name_and_description(): void
    {
        $cmd = new BuildCommand(sys_get_temp_dir(), []);

        self::assertSame('lintel:build', $cmd->getName());
        self::assertStringContainsString('Compile the Lintel sources', $cmd->getDescription());
    }
}
