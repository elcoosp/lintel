<?php

declare(strict_types=1);

namespace Lintel\Test;

use Lintel\Dev\Loader;
use PHPUnit\Framework\Assert;

use function array_key_exists;
use function file_put_contents;
use function function_exists;
use function glob;
use function hash;
use function is_dir;
use function mkdir;
use function preg_match;
use function preg_match_all;
use function proc_close;
use function proc_open;
use function sprintf;
use function stream_get_contents;
use function str_starts_with;
use function strtoupper;
use function sys_get_temp_dir;
use function uniqid;
use function unlink;

/**
 * The §17.1-Q3 testing helper (architecture doc §17.1 Q3 "testing
 * helpers", `docs/architecture.md`): compile-and-render a component
 * source in one call, without booting a framework or hand-rolling a
 * compilation step. A test writes the JSX it wants to exercise as the
 * source, the tester compiles it on the fly through the real `lintel`
 * binary — the same compile path the §6.10 dev loader drives — requires
 * the artifact once, invokes the component with named arguments, and
 * renders the result through the §6.8 runtime's `\Lintel\c()` (the same
 * sink prod markup goes through). Escaping is therefore under test too:
 * a hostile payload that reaches the markup through a test prop proves
 * the compiled component escapes it, exactly as a framework boot would.
 *
 * ```php
 * $t = new ComponentTester();
 * $t->assertRenders(
 *     '<b class="badge">Ada</b>',
 *     'function Badge(string $name): Html { return <b class="badge">{$name}</b>; }',
 *     ['name' => 'Ada'],
 * );
 * ```
 *
 * Two in-process constraints, both enforced loudly instead of fatalling:
 * - each unique source compiles once per tester (a second `render()` of
 *   the same source reuses the compiled component — PHP functions cannot
 *   be redeclared);
 * - two *different* sources may not declare the same component name in
 *   one PHP process — the second render throws before the fatal;
 * - a component named after a PHP builtin (function names are
 *   case-insensitive, so `Stat` collides with `stat()`) can never be
 *   declared in any process — the guard refuses before the fatal.
 *
 * `render()` needs no PHPUnit; the `assert*` helpers ride
 * `PHPUnit\Framework\Assert`, so a consumer that only renders needs
 * nothing beyond the runtime the compiled components already call.
 */
final class ComponentTester
{
    /** The compile binary (the §6.10 resolution order by default). */
    private string $binary;

    /** The scratch directory holding this tester's sources + artifacts. */
    private string $scratch;

    /** @var array<string, string> source hash → invoked FQ component name */
    private array $loaded = [];

    /** The next source's sequence number (unique scratch file names). */
    private int $sources = 0;

    public function __construct(?string $binary = null)
    {
        $this->binary = $binary ?? Loader::binary();
        $this->scratch = sys_get_temp_dir() . '/lintel-component-tester-' . uniqid();
        if (!mkdir($this->scratch, 0777, true) && !is_dir($this->scratch)) {
            throw new \RuntimeException("cannot create the tester scratch dir {$this->scratch}");
        }
    }

    public function __destruct()
    {
        foreach (glob($this->scratch . '/*') ?: [] as $file) {
            @unlink($file);
        }
        @rmdir($this->scratch);
    }

    /**
     * Compile `$jsx` on the fly, invoke its component with `$props` as
     * named arguments, and render the result through `\Lintel\c()` — the
     * exact string the compiled component contributes to a page.
     *
     * @param array<string, mixed> $props named arguments for the component
     *
     * @throws \RuntimeException when the source declares no component, a
     * component name is already declared in this process, the compile
     * fails, or the artifact does not declare the scanned component
     */
    public function render(string $jsx, array $props = []): string
    {
        $component = $this->component($jsx);
        if (!\is_callable($component)) {
            throw new \RuntimeException("the component {$component} is not callable");
        }
        $value = $component(...$props);

        return \Lintel\c($value);
    }

    /**
     * The rendered markup equals `$expectedHtml` byte for byte — the
     * assertion a component's golden-HTML test reads.
     *
     * @param array<string, mixed> $props named arguments for the component
     */
    public function assertRenders(string $expectedHtml, string $jsx, array $props = []): void
    {
        Assert::assertSame(
            $expectedHtml,
            $this->render($jsx, $props),
            'the component rendered unexpected markup',
        );
    }

    /**
     * The rendered markup contains `$needle` — the assertion for tests
     * that pin one decisive fragment (an escaped payload, one attribute)
     * without pinning the whole element.
     *
     * @param array<string, mixed> $props named arguments for the component
     */
    public function assertRendersContaining(string $needle, string $jsx, array $props = []): void
    {
        Assert::assertStringContainsString(
            $needle,
            $this->render($jsx, $props),
            'the rendered markup misses the expected fragment',
        );
    }

    /**
     * The FQ name of the source's first component (§4.7: a capitalized
     * function name), compiling + requiring the artifact exactly once per
     * unique source.
     *
     * @throws \RuntimeException (see {@see render()})
     */
    private function component(string $jsx): string
    {
        $hash = hash('xxh3', $jsx);
        if (array_key_exists($hash, $this->loaded)) {
            return $this->loaded[$hash];
        }
        $name = self::componentName($jsx);
        $fqn = self::namespaceOf($jsx) . $name;
        if (function_exists($fqn)) {
            throw new \RuntimeException(sprintf(
                'component %s is already declared in this PHP process: a test source'
                . ' may declare each component name once — rename the component'
                . ' (function %s) in one of the sources',
                $fqn,
                $name,
            ));
        }
        $artifact = $this->scratch . '/' . $hash . '.php';
        file_put_contents($artifact, $this->compile($jsx));
        require $artifact;
        if (!function_exists($fqn)) {
            throw new \RuntimeException(sprintf(
                'the compiled artifact does not declare %s — the source must declare'
                . ' the component as a plain function (the compiler only rewrites JSX,'
                . ' it never renames declarations)',
                $fqn,
            ));
        }
        $this->loaded[$hash] = $fqn;

        return $fqn;
    }

    /**
     * Run the real binary over the source (`lintel build <file>` — the
     * §6.10 compile path) and return the compiled PHP. A compile failure
     * throws with the compiler's rendered diagnostics, so the test fails
     * with the LNT message instead of a bare exit code.
     *
     * @throws \RuntimeException when the binary cannot start or fails
     */
    private function compile(string $jsx): string
    {
        ++$this->sources;
        $source = $this->scratch . '/s' . $this->sources . '.lintel.php';
        // The compiler treats an open-tag-less file as a pure HTML
        // template and passes it through untouched (§21.3): a JSX snippet
        // is compiler INPUT, so the `<?php` prologue rides along when the
        // snippet omits it.
        if (!str_starts_with($jsx, '<?php')) {
            $jsx = "<?php\n" . $jsx;
        }
        file_put_contents($source, $jsx);
        $process = @proc_open(
            [$this->binary, 'build', $source],
            [1 => ['pipe', 'w'], 2 => ['pipe', 'w']],
            $pipes,
        );
        if (!is_resource($process)) {
            throw new \RuntimeException("cannot start the lintel binary at {$this->binary}");
        }
        $stdout = (string) stream_get_contents($pipes[1]);
        $stderr = (string) stream_get_contents($pipes[2]);
        fclose($pipes[1]);
        fclose($pipes[2]);
        $exit = proc_close($process);
        if ($exit !== 0 || $stdout === '') {
            throw new \RuntimeException(sprintf(
                'the component source failed to compile (exit %d): %s',
                $exit,
                $stderr !== '' ? $stderr : $stdout,
            ));
        }

        return $stdout;
    }

    /**
     * The source's first component name (§4.7: capitalized first letter —
     * lowercase `function helper()` declarations are helpers, not
     * components).
     *
     * @throws \RuntimeException when the source declares no component
     */
    private static function componentName(string $jsx): string
    {
        $matched = preg_match_all('/\bfunction\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(/', $jsx, $groups);
        if ($matched !== false && $matched > 0) {
            foreach ($groups[1] as $name) {
                $first = $name[0];
                if (strtoupper($first) === $first && strtolower($first) !== $first) {
                    return $name;
                }
            }
        }

        throw new \RuntimeException(
            'the given source declares no component — testing helpers expect'
            . ' `function Name(...): Html` with a capitalized name (§4.7)',
        );
    }

    /**
     * The source's `namespace …;` declaration as a call prefix (`''` for
     * a global-namespace component).
     */
    private static function namespaceOf(string $jsx): string
    {
        if (preg_match('/\bnamespace\s+([A-Za-z_][A-Za-z0-9_\\\\]*)\s*;/', $jsx, $m) !== 1) {
            return '';
        }

        return $m[1] . '\\';
    }
}
