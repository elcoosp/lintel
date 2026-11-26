<?php

declare(strict_types=1);

namespace Markup\Tests;

use PHPUnit\Framework\TestCase;

use function Markup\render;

/**
 * The RFC-convergence end-to-end (architecture Year 3 "RFC convergence"):
 * ONE fixture compiled with `--target=rfc` renders through the new
 * `\Markup\` runtime, byte-identical to the SAME fixture compiled with the
 * string target and rendered through `\Lintl\c()`. The compiler side of
 * the rfc target shipped in lintl-core (docs/rfc-alignment.md) — this is
 * the runtime side that was missing. The compile runs the real
 * `lintl` binary (LINTL_TEST_BIN, the same lane the lintl package's
 * tests use); each target renders in its own process so the two compiled
 * files' same-named components don't collide.
 */
final class MarkupE2eTest extends TestCase
{
    /** The rfc target renders the same fixture to the string target's bytes. */
    public function test_rfc_runtime_matches_the_string_target_for_a_fixture_component(): void
    {
        $binary = getenv('LINTL_TEST_BIN') ?: __DIR__.'/../../../target/release/lintl';
        self::assertFileExists($binary, 'the lintl binary must be built for the e2e lane');
        $fixture = __DIR__.'/fixture/Page.lintl.php';

        $rfcPhp = shell_exec(escapeshellarg($binary).' build '.escapeshellarg($fixture).' --target=rfc');
        $stringPhp = shell_exec(escapeshellarg($binary).' build '.escapeshellarg($fixture));
        self::assertIsString($rfcPhp);
        self::assertIsString($stringPhp);

        $tmp = sys_get_temp_dir().'/lintl-markup-e2e-'.bin2hex((string) random_bytes(4));
        mkdir($tmp, 0777, true);
        file_put_contents($tmp.'/page-rfc.php', $rfcPhp);
        file_put_contents($tmp.'/page-string.php', $stringPhp);

        try {
            $rfcOut = $this->runLane(
                $tmp.'/lane-rfc.php',
                <<<'PHP'
<?php
require {AUTOLOAD};
require {COMPILED};
echo \Markup\render(Page('W"orld<script>', 'javascript:alert(1)'));
PHP,
                $tmp.'/page-rfc.php',
            );
            $stringOut = $this->runLane(
                $tmp.'/lane-string.php',
                <<<'PHP'
<?php
require {AUTOLOAD};
require {COMPILED};
echo \Lintl\c(Page('W"orld<script>', 'javascript:alert(1)'));
PHP,
                $tmp.'/page-string.php',
            );

            self::assertSame($stringOut, $rfcOut, 'the rfc runtime must render the string target\'s bytes');
            // The security contract rides through both lanes identically.
            self::assertStringContainsString('<h2>W&quot;orld&lt;script&gt;</h2>', $rfcOut);
            self::assertStringContainsString('href="about:invalid#blocked"', $rfcOut);
            self::assertStringContainsString('<em class="who">W&quot;orld&lt;script&gt;</em>', $rfcOut);
            self::assertStringContainsString('<hr>', $rfcOut);
        } finally {
            @unlink($tmp.'/page-rfc.php');
            @unlink($tmp.'/page-string.php');
            @unlink($tmp.'/lane-rfc.php');
            @unlink($tmp.'/lane-string.php');
            @rmdir($tmp);
        }
    }

    /** Write a lane script and run it in a fresh PHP process; the lane
     * requires the markup package autoload, the compiled file for its
     * target, and prints the rendered bytes. */
    private function runLane(string $lanePath, string $laneTemplate, string $compiled): string
    {
        $autoload = __DIR__.'/../vendor/autoload.php';
        $lane = str_replace(['{AUTOLOAD}', '{COMPILED}'], [
            var_export($autoload, true),
            var_export($compiled, true),
        ], $laneTemplate);
        file_put_contents($lanePath, $lane);
        $out = shell_exec(escapeshellarg(PHP_BINARY).' '.escapeshellarg($lanePath).' 2>&1');
        self::assertIsString($out, 'the lane process must produce output');

        return $out;
    }

    /** The rfc lowering contract on a root fragment: a plain PHP array renders through the runtime. */
    public function test_root_fragment_array_renders_through_the_runtime(): void
    {
        self::assertSame('aW&quot;orldb', render(['a', 'W"orld', 'b']));
    }
}
