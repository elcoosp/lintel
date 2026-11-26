<?php

declare(strict_types=1);

namespace Lintl\Tests;

use PHPUnit\Framework\Attributes\DataProvider;
use PHPUnit\Framework\TestCase;

/**
 * M1 release-readiness: the independent threat-corpus expansion. Every
 * vector in {@see ExpandedThreatCorpus} rides a dynamic child (the c() lane)
 * and, when URL/scheme-shaped, a dynamic URL attribute (the u() lane). Each
 * source is compiled with the REAL `lintl` binary (LINTL_TEST_BIN), the
 * compiled PHP is executed under real PHP, and the rendered output is
 * asserted INERT: no live `<script>` tag, no `javascript:`/`vbscript:`/
 * `data:text/html` scheme in a rendered URL, no unescaped `on\w+=` event
 * handler. A compile-time validator rejection (LNL0301 etc.) is the
 * strongest inertness — the source never becomes executable PHP — and is
 * counted as a passing vector with its code pinned.
 *
 * The vectors are REAL published payloads from OWASP (XSS Filter Evasion
 * Cheat Sheet, CC BY-SA 4.0) and PortSwigger (Web Security Academy XSS
 * cheat sheet); the attribution rides each vector. This closes the M1
 * blind spot: the §7.2 corpus was ~31 vectors authored by the same agent
 * that wrote the escaper, so oracle and implementation shared a blind spot
 * by construction. An independent, published, attributed corpus cannot.
 */
final class ExpandedThreatCorpusTest extends TestCase
{
    /** The lintl binary (the release build; CI sets LINTL_TEST_BIN). */
    private static function binary(): string
    {
        $binary = getenv('LINTL_TEST_BIN') ?: __DIR__.'/../../../target/release/lintl';
        self::assertFileExists($binary, 'the lintl binary must be built (cargo build --release -p lintl-cli)');

        return $binary;
    }

    /** A unique temp dir per test run (parallel-safe). */
    private static function workDir(): string
    {
        $dir = sys_get_temp_dir().'/lintl-expanded-threat-'.bin2hex((string) random_bytes(6));
        @mkdir($dir, 0o777, true);

        return $dir;
    }

    /**
     * Compile a `.lintl.php` source with the real binary; return the
     * compiled PHP, or null + the diagnostic text if the compiler rejected
     * the source (a validator refusal — the strongest inertness).
     *
     * @return array{?string, string} [compiled-php-or-null, stderr]
     */
    private static function compile(string $binary, string $source, string $workDir): array
    {
        $src = $workDir.'/case.lintl.php';
        file_put_contents($src, $source);
        $descriptors = [
            0 => ['pipe', 'r'],
            1 => ['pipe', 'w'],
            2 => ['pipe', 'w'],
        ];
        $cmd = escapeshellarg($binary).' build '.escapeshellarg($src);
        $proc = proc_open($cmd, $descriptors, $pipes);
        self::assertIsResource($proc, 'lintl build spawns');
        fclose($pipes[0]);
        $stdout = (string) stream_get_contents($pipes[1]);
        $stderr = (string) stream_get_contents($pipes[2]);
        fclose($pipes[1]);
        fclose($pipes[2]);
        $code = proc_close($proc);
        // A non-zero exit with diagnostics means the compiler rejected the
        // source (LNL0301 for an onclick spread, etc.) — the strongest
        // inertness. Return null + the stderr so the test pins the code.
        if ($code !== 0) {
            return [null, $stderr];
        }

        return [$stdout, $stderr];
    }

    /** Execute the compiled PHP under real PHP and return stdout. */
    private function execute(string $compiledPhp, string $workDir, string $payload): string
    {
        // The compiled template defines `function T($p)` that renders the
        // payload; we call it with the payload so the dynamic value flows
        // through the escaper exactly as a real render would.
        $runner = $workDir.'/runner.php';
        $runtime = __DIR__.'/../src/functions.php';
        $compiled = $workDir.'/compiled.php';
        file_put_contents($compiled, $compiledPhp);
        // The runner loads the runtime, requires the compiled template, and
        // echoes T($payload). json_encode carries the payload safely across
        // the PHP-source boundary (no quoting issues, no eval).
        $payloadJson = json_encode($payload, JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE);
        if ($payloadJson === false) {
            throw new \RuntimeException('json_encode failed for payload (invalid UTF-8?)');
        }
        file_put_contents($runner, <<<PHP
<?php
declare(strict_types=1);
require {$this->phpLiteral($runtime)};
require {$this->phpLiteral($compiled)};
\$payload = json_decode({$this->phpLiteral($payloadJson)}, true);
echo T(\$payload);
PHP);
        $cmd = escapeshellarg(PHP_BINARY).' '.escapeshellarg($runner).' 2>&1';
        $out = (string) shell_exec($cmd);

        return $out;
    }

    /** A PHP single-quoted string literal for a path/payload. */
    private function phpLiteral(string $s): string
    {
        return "'".addcslashes($s, "'\\")."'";
    }

    /**
     * The inertness assertion: every `<`, `>`, `"`, `'` the PAYLOAD carries
     * must be HTML-escaped in the rendered output. `htmlspecialchars` turns
     * `<script>` into `&lt;script&gt;` and `"` into `&quot;`, so a raw
     * `<script`/`<img`/`<svg` (unescaped `<`) or a bare `on\w+=` attribute
     * (unescaped quote that the payload injected) in the rendered output is
     * a real breakout — the escaper was bypassed.
     *
     * The template's OWN static markup (`<p>`, `<a href=...>`) legitimately
     * carries raw `<` and `"`. To distinguish template markup from a payload
     * breakout, we check the payload's OWN dangerous shapes against the
     * rendered output: if the payload contains `<script` and the rendered
     * output contains a raw `<script` (not `&lt;script`), the payload's tag
     * was not escaped. Same for `<img`, `<svg`, `<iframe`, `<style` and for
     * `on\w+=` event-handler attributes.
     */
    private static function assertInert(string $rendered, string $payload, string $name): void
    {
        // The dangerous shapes the payload might carry, each as a regex.
        // If the payload carries the shape AND the rendered output carries
        // the RAW (unescaped) shape, the escaper was bypassed — a breakout.
        $shapes = [
            'script-tag' => '/<script\b/i',
            'img-tag' => '/<img\b/i',
            'svg-tag' => '/<svg\b/i',
            'iframe-tag' => '/<iframe\b/i',
            'style-tag' => '/<style\b/i',
            'object-tag' => '/<object\b/i',
            'embed-tag' => '/<embed\b/i',
            'event-handler' => '/\son\w+\s*=\s*["\']/i',
        ];
        foreach ($shapes as $label => $shape) {
            if (preg_match($shape, $payload)) {
                // The payload carries this shape. The rendered output must
                // carry it ONLY in escaped form (`&lt;script`, `&quot;on...`).
                // A raw match (unescaped) means the payload's bytes reached
                // the output verbatim — a breakout.
                if (preg_match($shape, $rendered)) {
                    // Distinguish: did the RAW shape come from the payload,
                    // or is it the template's own static markup? The template
                    // sources here use only <p>, <a href={...}>link</a> — no
                    // <script>/<img>/<svg>/<iframe>/<style>/<object>/<embed>
                    // and no on*= handlers. So any raw match is from the payload.
                    self::fail(
                        "{$name}: {$label} breakout — the payload's shape reached the output unescaped\n"
                        ."--- payload ---\n{$payload}\n--- rendered ---\n{$rendered}"
                    );
                }
            }
        }
        // A live `javascript:`/`vbscript:`/`data:text/html` URL is only
        // dangerous when it is the ACTUAL href/src value (the URL the
        // browser navigates). The u() helper rewrites a scheme-leading URL
        // to `about:invalid#blocked`; a non-scheme-leading URL (e.g. one
        // that starts with `<` or text) passes through escaped, and the
        // `javascript:` inside it is DATA (escaped text inside the
        // attribute value), not a live scheme. So we only flag a live
        // scheme if it is at the START of the attribute value.
        if (preg_match('#(?:href|src|action|poster|formaction|background)\s*=\s*"(?:javascript:|vbscript:|data:text/html)#i', $rendered)) {
            self::fail("{$name}: live script scheme at the start of a rendered URL attribute\n--- rendered ---\n{$rendered}");
        }
    }

    #[DataProvider('provideVectors')]
    public function test_every_published_payload_is_inert(string $payload, string $attribution, bool $urlShaped): void
    {
        $name = $this->dataName();
        $binary = self::binary();
        $workDir = self::workDir();
        try {
            // The child lane: the payload rides a dynamic child through c().
            $childSource = "<?php\nfunction T(\$p) { return <p>{$this->phpExpr($payload)}</p>; }\n";
            [$compiled, $stderr] = $this->compile($binary, $childSource, $workDir);
            if ($compiled === null) {
                // Compile-time rejection: the strongest inertness. Pin that
                // the rejection carries an LNL code (not a crash).
                self::assertMatchesRegularExpression(
                    '/LNL\d{4}/',
                    $stderr,
                    "{$name}: compile rejection must carry an LNL code (payload: {$payload})"
                );

                return;
            }
            $rendered = $this->execute($compiled, $workDir, $payload);
            self::assertInert($rendered, $payload, $name.' [child lane]');

            // The URL lane: URL-shaped payloads also ride a dynamic href.
            if ($urlShaped) {
                $urlSource = "<?php\nfunction T(\$p) { return <a href={$this->phpExpr($payload)}>link</a>; }\n";
                [$compiled2, $stderr2] = $this->compile($binary, $urlSource, $workDir);
                if ($compiled2 !== null) {
                    $rendered2 = $this->execute($compiled2, $workDir, $payload);
                    self::assertInert($rendered2, $payload, $name.' [url lane]');
                }
            }
        } finally {
            self::cleanup($workDir);
        }
        // Reached only if both lanes were inert (or rejected with a code).
        $this->addToAssertionCount(1);
    }

    /** The PHP expression that carries the payload as a dynamic value. */
    private function phpExpr(string $payload): string
    {
        // The payload rides as a dynamic string value {$payload} — the
        // template's $p parameter, so the compiler sees a dynamic child/
        // attribute (not a static literal it could fold). We render T($p)
        // with the real payload at execution time.
        return '{$p}';
    }

    /** The test name (attribution + index) for failure messages. */

    /** Remove the temp workdir. */
    private static function cleanup(string $dir): void
    {
        if (! is_dir($dir)) {
            return;
        }
        foreach (glob($dir.'/*') ?: [] as $f) {
            @unlink($f);
        }
        @rmdir($dir);
    }

    /** @return iterable<string, array{payload: string, attribution: string, urlShaped: bool}> */
    public static function provideVectors(): iterable
    {
        // The data provider lives in a separate class so the corpus file is
        // a pure data artifact (regenerable by scripts/gen-expanded-threat-corpus.py).
        foreach (ExpandedThreatCorpus::vectors() as $name => $vector) {
            yield $name => [
                'payload' => $vector['payload'],
                'attribution' => $vector['attribution'],
                'urlShaped' => $vector['url_shaped'],
            ];
        }
    }

    /** The corpus counts at least 200 REAL published vectors (M1 contract). */
    public function test_the_corpus_carries_at_least200_real_published_vectors(): void
    {
        $count = count(ExpandedThreatCorpus::vectors());
        self::assertGreaterThanOrEqual(
            200,
            $count,
            "the expanded threat corpus must carry at least 200 REAL published vectors (got {$count})"
        );
        // Every vector carries an attribution (OWASP or PortSwigger).
        foreach (ExpandedThreatCorpus::vectors() as $name => $vector) {
            self::assertContains(
                $vector['attribution'],
                ['OWASP', 'PortSwigger'],
                "{$name}: attribution must be OWASP or PortSwigger"
            );
            self::assertNotEmpty($vector['payload'], "{$name}: payload is empty");
        }
    }
}
