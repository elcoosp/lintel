<?php

declare(strict_types=1);

namespace Lintl\Blade\Tests;

use Illuminate\Foundation\Application;
use Lintl\Blade\Console\BuildCommand;
use Lintl\Blade\Console\CheckCommand;
use Lintl\Blade\Console\ClearCommand;
use Lintl\Blade\Console\Command;
use PHPUnit\Framework\TestCase;
use Symfony\Component\Console\Input\ArrayInput;
use Symfony\Component\Console\Output\BufferedOutput;

/**
 * The §11 `artisan lintl:*` wrappers (§17.1-Q2's remaining Laravel
 * surface): each command executes through the real Symfony `run()` lane
 * artisan itself drives, with a real Foundation `Application` as the
 * container (its `runningUnitTests()` and `OutputStyle`/`Factory`
 * resolution are exactly what `Command::run()` consumes) and explicit
 * paths standing in for the `config('lintl.*')` values the environment
 * would give it. `lintl:build` produces the cache-dir artifacts + the
 * manifest, `lintl:check` maps the binary's diagnostics exit to the
 * command's, `lintl:clear` wipes the dev loader's cache keys.
 */
final class ConsoleCommandsTest extends TestCase
{
    /** `lintl:build` compiles the fixture: artifacts into the cache dir
     * and the `lintl.json` manifest into the working directory.
     *
     * The M0 cwd-hygiene regression: the bare `--manifest` write resolves
     * against the workspace being compiled (the src/… + cache/ ancestor —
     * the app root), never the process cwd, so this runs from wherever
     * PHPUnit sits (no chdir — the workaround that once scattered a stray
     * `lintl.json` at the outer cwd under some invocation orderings) and
     * the manifest must still land at `$root`. */
    public function test_build_command_compiles_into_cache_dir(): void
    {
        $root = $this->workspace();
        \mkdir($root.'/src', 0777, true);
        \copy(__DIR__.'/fixture/Badge.lintl.php', $root.'/src/Badge.lintl.php');

        $runnerCwd = \getcwd() ?: '.';
        [$rc, $text] = $this->runCommand(new BuildCommand($root.'/cache', [$root.'/src']), $root);

        self::assertSame(0, $rc, $text);
        self::assertFileExists($root.'/cache/Badge.php');
        self::assertFileExists($root.'/lintl.json', 'the manifest lands at the app root');
        $manifest = \json_decode((string) \file_get_contents($root.'/lintl.json'), true);
        self::assertIsArray($manifest);
        self::assertArrayHasKey('prod', $manifest, 'the T-31 manifest shape');
        self::assertFileDoesNotExist(
            $runnerCwd.'/lintl.json',
            'no stray manifest at the process cwd (M0 cwd hygiene)',
        );
    }

    /** `lintl:check` succeeds on clean sources. */
    public function test_check_command_passes_on_clean_source(): void
    {
        $root = $this->workspace();
        \mkdir($root.'/src', 0777, true);
        \copy(__DIR__.'/fixture/Badge.lintl.php', $root.'/src/Badge.lintl.php');

        [$rc, $text] = $this->runCommand(new CheckCommand($root.'/cache', [$root.'/src']), $root);

        self::assertSame(0, $rc, $text);
    }

    /** `lintl:check` maps the binary's error-diagnostics exit (1) to the
     * artisan FAILURE — CI and deploy gates feel a broken component. */
    public function test_check_command_fails_on_broken_source(): void
    {
        $root = $this->workspace();
        \mkdir($root.'/src', 0777, true);
        \file_put_contents($root.'/src/Broken.lintl.php', '<?php function Broken({a}: string) { return <div>{a}</span>; }');

        [$rc, $text] = $this->runCommand(new CheckCommand($root.'/cache', [$root.'/src']), $root);

        self::assertSame(1, $rc);
        self::assertStringContainsString('LNL0102', $text, 'the diagnostics render into the command output');
    }

    /** `lintl:clear` wipes the dev loader's cached compilations. */
    public function test_clear_command_wipes_cache(): void
    {
        $root = $this->workspace();
        \mkdir($root.'/cache', 0777, true);
        \file_put_contents($root.'/cache/abc123-1-2.php', '<?php /* cached */');
        \file_put_contents($root.'/cache/def456-1-2.php.tmp', '<?php /* half-written */');

        [$rc, $text] = $this->runCommand(new ClearCommand($root.'/cache'), $root);

        self::assertSame(0, $rc, $text);
        self::assertFileDoesNotExist($root.'/cache/abc123-1-2.php');
        self::assertFileDoesNotExist($root.'/cache/def456-1-2.php.tmp');
        self::assertStringContainsString('cleared 1 cached compilation', $text);
    }

    /** `lintl:clear` on a missing cache dir succeeds with a note. */
    public function test_clear_command_without_cache_succeeds(): void
    {
        $root = $this->workspace();

        [$rc, $text] = $this->runCommand(new ClearCommand($root.'/no-cache'), $root);

        self::assertSame(0, $rc, $text);
        self::assertStringContainsString('nothing to clear', $text);
    }

    /** A per-test workspace dir (cleaned in tearDown). */
    private function workspace(): string
    {
        $dir = sys_get_temp_dir().'/lintl-artisan-'.bin2hex((string) random_bytes(4));
        \mkdir($dir, 0777, true);

        return $dir;
    }

    /** Execute a command through the real Symfony run() lane with a real
     * Foundation Application as the container — artisan's own shape.
     *
     * @return array{int, string} the exit code and the captured output */
    private function runCommand(Command $cmd, string $basePath): array
    {
        $cmd->setLaravel(new Application($basePath));
        $out = new BufferedOutput;
        $rc = $cmd->run(new ArrayInput([]), $out);

        return [$rc, $out->fetch()];
    }

    protected function tearDown(): void
    {
        foreach (\glob(sys_get_temp_dir().'/lintl-artisan-*') ?: [] as $dir) {
            self::rmrf($dir);
        }
    }

    private static function rmrf(string $dir): void
    {
        foreach (\glob($dir.'/*') ?: [] as $f) {
            if (\is_dir($f)) {
                self::rmrf($f);
            } else {
                @\unlink($f);
            }
        }
        @\rmdir($dir);
    }
}
