<?php

declare(strict_types=1);

namespace Lintl\PhpStan;

/**
 * The bridge's compile-and-record half (architecture doc §17.1 Q1, §7.3-Y2).
 *
 * For every source `.lintl.php` file a consumer wants under PHPStan,
 * {@see Bridge::prepare()} drives the SAME compiler binary the CLI and LSP
 * use — the single source of truth; nothing here re-implements scanning —
 * and writes three things into a cache directory:
 *
 * 1. the compiled artifact (`lintl build --out`): valid PHP, line count
 *    equal to the source by the line-preservation guarantee (§5.2), so a
 *    PHPStan error on an artifact line maps back to the source line by
 *    identity;
 * 2. the metadata sidecar (`<artifact>.lintl-meta.json`): the source's
 *    absolute path and the compiler's own JSON diagnostics
 *    (`lintl check --format=json`, `docs/diagnostics-json.md`) — the
 *    {@see LintlDiagnosticsRule} reads exactly this file, and reports
 *    each diagnostic as a PHPStan error on the SOURCE path and line with
 *    a stable `lintl.LNLxxxx` identifier;
 * 3. the signature stub (`<artifact>.lintl-stub.php`): one PHP function
 *    per declared component, with the scanner's proven facts only —
 *    parameter names, required-ness (no default), variadic, non-nullable
 *    `string` types (§7.3) — and the `\Lintl\Html` return type. A
 *    consumer's phpstan.neon points `stubFiles` at these files, so the
 *    named-argument calls the compiler emits (`Card(title: 'Hi', …)`)
 *    type-check against the real component signatures.
 *
 * Written for a source that fails to compile: no artifact, but the
 * metadata sidecar (with the compiler's diagnostics) and the stub are
 * still written, so the LNL errors surface in the same PHPStan run.
 */
final class Bridge
{
    /** The env var that names the lintl binary (the T-11 plugin's convention). */
    public const BINARY_ENV = 'LINTL_TEST_BIN';

    /** The metadata sidecar suffix, appended to the artifact path. */
    public const METADATA_SUFFIX = '.lintl-meta.json';

    /** The signature-stub suffix, appended to the artifact path. */
    public const STUB_SUFFIX = '.lintl-stub.php';

    /**
     * Compiles `$sourcePath` with the lintl binary and writes the
     * metadata sidecar and signature stub. Returns the artifact path (an
     * empty string when the source has compile errors — nothing was
     * written), alongside the paths a phpstan.neon needs.
     */
    public static function prepare(string $sourcePath, string $cacheDir, ?string $binary = null): Prepared
    {
        $sourcePath = realpath($sourcePath);
        if ($sourcePath === false) {
            throw new BridgeException("source does not exist: {$sourcePath}");
        }
        if (! str_ends_with($sourcePath, '.lintl.php')) {
            throw new BridgeException('bridge sources are `.lintl.php` files: '.$sourcePath);
        }
        if (! is_dir($cacheDir) && ! @mkdir($cacheDir, 0777, true) && ! is_dir($cacheDir)) {
            throw new BridgeException("cache directory cannot be created: {$cacheDir}");
        }

        $binary ??= self::binary();
        // `lintl build --out` names the artifact by the source's last
        // path segment with the `.lintl` infix dropped (`Page.lintl.php`
        // -> `Page.php`) — the mapping the rule's sidecar lookup keys on.
        $artifactName = preg_replace('/\.lintl\.php$/', '.php', basename($sourcePath));
        if ($artifactName === null) {
            throw new BridgeException('bridge sources are `.lintl.php` files: '.$sourcePath);
        }

        // 1. Compile (the exact `lintl build` lane; output only exists
        //    when the source compiled cleanly). A failed compile still
        //    leaves an empty-but-present artifact shell so the analysis
        //    run has a file for the metadata sidecar to sit beside; the
        //    metadata records `artifact: null` for the distinction.
        $artifact = $cacheDir.'/'.$artifactName;
        @unlink($artifact);
        $compile = self::run([$binary, 'build', $sourcePath, '--out', $cacheDir]);
        $hasArtifact = is_file($artifact);
        if (! $hasArtifact) {
            file_put_contents($artifact, "<?php\n");
        }

        // 2. Record the compiler's diagnostics for the source, whatever
        //    the compile verdict was. Each JSON line decodes into the
        //    validated diagnostic shape (`docs/diagnostics-json.md`'s
        //    stable fields) — malformed lines cannot become half-typed
        //    metadata the rule would have to re-guess.
        $check = self::run([$binary, 'check', $sourcePath, '--format=json']);
        $diagnostics = [];
        foreach (explode("\n", trim($check['stderr'])) as $line) {
            if ($line === '') {
                continue;
            }
            $decoded = json_decode($line, true);
            if (is_array($decoded)) {
                $diagnostics[] = self::decodeDiagnostic($decoded);
            }
        }
        $metaPath = $artifact.self::METADATA_SUFFIX;
        file_put_contents($metaPath, (string) json_encode([
            'source' => $sourcePath,
            'artifact' => $hasArtifact ? $artifact : null,
            'diagnostics' => $diagnostics,
        ], JSON_THROW_ON_ERROR));

        // 2b. The container-time autoloader: PHPStan compiles its DI
        //     container before any analysis, and resolves rule classes
        //     through the project autoloader it finds at/above the working
        //     directory — a Composer project's `vendor/autoload.php` (which
        //     already includes this package). A bare cache directory has
        //     none, so prepare writes the minimal shim; the rule and its
        //     metadata contract then load exactly as in a real project.
        self::writeAutoloaderShim($cacheDir);

        // 3. Generate the signature stub from the same compiler's shape
        //    report (`lintl stats --format=json`, §21.1's inventory).
        $stubPath = $artifact.self::STUB_SUFFIX;
        $stats = self::run([$binary, 'stats', $sourcePath, '--format=json']);
        $inventory = json_decode($stats['stdout'], true);
        $stub = is_array($inventory) ? self::stubFor($sourcePath, $inventory) : null;
        if ($stub !== null && $stub !== '') {
            file_put_contents($stubPath, $stub);
        } else {
            @unlink($stubPath);
            $stubPath = '';
        }

        return new Prepared(
            source: $sourcePath,
            artifact: $hasArtifact ? $artifact : '',
            metadata: $metaPath,
            stub: $stubPath,
        );
    }

    /**
     * Writes the minimal autoloader shim for a bare cache directory: an
     * `spl_autoload_register` over this package's PSR-4 prefix, so
     * `Lintl\PhpStan\LintlDiagnosticsRule` is container-constructible
     * when PHPStan runs with the cache as its working directory.
     */
    private static function writeAutoloaderShim(string $cacheDir): void
    {
        $vendorDir = $cacheDir.'/vendor';
        if (! is_dir($vendorDir) && ! @mkdir($vendorDir, 0777, true) && ! is_dir($vendorDir)) {
            throw new BridgeException("vendor directory cannot be created: {$vendorDir}");
        }
        $srcDir = var_export(__DIR__.'/', true);
        $lines = [
            '<?php',
            '',
            '// Generated by lintl/phpstan: the minimal autoloader for a bare analysis',
            '// cache. In a Composer project the real vendor/autoload.php (which already',
            "// includes lintl/phpstan) takes this file's place.",
            'spl_autoload_register(static function (string $class): void {',
            "    \$prefix = 'Lintl\\\\PhpStan\\\\';",
            '    if (! str_starts_with($class, $prefix)) {',
            '        return;',
            '    }',
            "    \$file = {$srcDir} . str_replace('\\\\', '/', substr(\$class, strlen(\$prefix))) . '.php';",
            '    if (is_file($file)) {',
            '        require $file;',
            '    }',
            '});',
        ];
        // The runtime's `Html` (and the `c()`/`a()`/… helpers the compiled
        // artifacts call) resolve at container-compile time only when it is
        // loaded eagerly — composer's `files` autoload is lazy. In a
        // Composer project the consumer's own autoloader covers it; a bare
        // cache points at the monorepo's runtime when it sits there.
        $runtime = __DIR__.'/../../runtime/src/functions.php';
        if (is_file($runtime)) {
            $lines[] = 'require_once '.var_export($runtime, true).';';
        }
        $lines[] = '';
        file_put_contents($vendorDir.'/autoload.php', implode("\n", $lines));
    }

    /**
     * The metadata for an analysed file, or `null` when the file is not a
     * compiled Lintl artifact (every ordinary PHP file in the project).
     * The rule calls this per file; a missing sidecar is the common case.
     *
     * @return array{source: string, artifact: string|null, diagnostics: list<array{code: string, severity: string, message: string, offset: int, line: int, col: int, help: string|null, docs: string}>}|null
     */
    public static function metadataFor(string $artifactPath): ?array
    {
        $metaPath = $artifactPath.self::METADATA_SUFFIX;
        if (! is_file($metaPath)) {
            return null;
        }
        $decoded = json_decode((string) file_get_contents($metaPath), true);
        if (
            ! is_array($decoded)
            || ! isset($decoded['source'], $decoded['diagnostics'])
            || ! is_string($decoded['source'])
            || ! is_array($decoded['diagnostics'])
        ) {
            return null;
        }
        $artifact = $decoded['artifact'] ?? null;
        $diagnostics = [];
        foreach ($decoded['diagnostics'] as $raw) {
            if (is_array($raw)) {
                $diagnostics[] = self::decodeDiagnostic($raw);
            }
        }

        return [
            'source' => $decoded['source'],
            'artifact' => is_string($artifact) ? $artifact : null,
            'diagnostics' => $diagnostics,
        ];
    }

    /**
     * The validated diagnostic shape (`docs/diagnostics-json.md`'s stable
     * fields — code, severity, message, offset, line, col, help, docs; new
     * fields may be appended, these never renamed). Every field carries a
     * fallback with the exact semantics the diagnostics rule used to
     * apply per-consumer, so decoding is the single validation point:
     * the compiler always emits this shape, and anything malformed
     * degrades to the same defaults the rule's own guards spelled — it
     * never produces a half-typed entry downstream code must re-guess.
     *
     * @param  array<mixed, mixed>  $raw
     * @return array{code: string, severity: string, message: string, offset: int, line: int, col: int, help: string|null, docs: string}
     */
    public static function decodeDiagnostic(array $raw): array
    {
        $code = is_string($raw['code'] ?? null) ? $raw['code'] : 'LNL????';
        $message = is_string($raw['message'] ?? null) ? $raw['message'] : 'unknown Lintl diagnostic';

        return [
            'code' => $code,
            'severity' => is_string($raw['severity'] ?? null) ? $raw['severity'] : 'error',
            'message' => $message,
            'offset' => is_int($raw['offset'] ?? null) ? $raw['offset'] : 0,
            'line' => is_int($raw['line'] ?? null) ? $raw['line'] : 1,
            'col' => is_int($raw['col'] ?? null) ? $raw['col'] : 1,
            'help' => is_string($raw['help'] ?? null) ? $raw['help'] : null,
            'docs' => is_string($raw['docs'] ?? null) ? $raw['docs'] : '',
        ];
    }

    /**
     * The signature stub for one scanned source: the declared components
     * as plain PHP functions in the source's own namespace, carrying the
     * scanner's proven facts only — names, required-ness (a required
     * parameter is declared without a default), variadic, non-nullable
     * `string` types (§7.3's typed-parameter set) — and the `\Lintl\Html`
     * return type. Unproven parameter types stay untyped (`mixed`), the
     * same conservatism the compiler's elision applies.
     *
     * @param  array<mixed, mixed>  $inventory  the `lintl stats --format=json` object
     */
    public static function stubFor(string $sourcePath, array $inventory): string
    {
        $namespace = '';
        if (preg_match('/^\s*namespace\s+([^;]+);/m', (string) file_get_contents($sourcePath), $m) === 1) {
            $namespace = trim($m[1]);
        }
        $out = "<?php\n\ndeclare(strict_types=1);\n\n";
        if ($namespace !== '') {
            $out .= "namespace {$namespace};\n\n";
        }
        $out .= "// Generated by lintl/phpstan from the compiler's signature table — do not edit.\n";
        $declared = $inventory['declared'] ?? [];
        if (! is_array($declared) || $declared === []) {
            return '';
        }
        foreach ($declared as $component) {
            if (! is_array($component) || ! isset($component['name']) || ! is_string($component['name'])) {
                continue;
            }
            $allParams = is_array($component['params'] ?? null) ? $component['params'] : [];
            $typedStrings = is_array($component['typed_strings'] ?? null) ? $component['typed_strings'] : [];
            $required = is_array($component['required'] ?? null) ? $component['required'] : [];
            $params = [];
            $docblock = [];
            foreach ($allParams as $index => $name) {
                if (! is_string($name)) {
                    continue;
                }
                $typed = in_array($name, $typedStrings, true);
                $type = $typed ? 'string ' : '';
                if (! $typed) {
                    // The scanner proves only `string` (§7.3); everything
                    // else is declared mixed — the same conservatism the
                    // compiler applies, spelled so the stub validator's
                    // typehint check passes on the unproven parameters.
                    $docblock[] = ' * @param mixed $'.$name;
                }
                $last = $index === array_key_last($allParams);
                if (($component['variadic'] ?? false) === true && $last) {
                    $params[] = $type.'...'.$name;

                    continue;
                }
                $default = in_array($name, $required, true) ? '' : ' = null';
                $params[] = $type.'$'.$name.$default;
            }
            if ($docblock !== []) {
                $out .= "/**\n".implode("\n", $docblock)."\n */\n";
            }
            $out .= 'function '.$component['name'].'('.implode(', ', $params).'): \\Lintl\\Html {}'."\n";
            unset($typedStrings, $required);
        }

        return $out;
    }

    /**
     * The lintl binary: the `LINTL_TEST_BIN` env var wins (the T-11
     * plugin's convention, also the CI's wiring), then `vendor/bin/lintl`
     * next to this package's vendor directory — the plugin's installed
     * cache (§12).
     */
    public static function binary(): string
    {
        $env = getenv(self::BINARY_ENV);
        if (is_string($env) && $env !== '') {
            return $env;
        }
        $candidates = [
            __DIR__.'/../../../bin/lintl',
            __DIR__.'/../../../../vendor/bin/lintl',
            __DIR__.'/../../../vendor/bin/lintl',
        ];
        foreach ($candidates as $candidate) {
            if (is_executable($candidate)) {
                return $candidate;
            }
        }
        throw new BridgeException(
            'the lintl binary is not discoverable: set '.self::BINARY_ENV
            .' to the built `lintl` binary (the compiler is the single source of truth)',
        );
    }

    /**
     * Runs a command, returning stdout and the exit code. The compiler
     * uses exit codes as signals (`check` exits 1 on errors by design,
     * `docs/diagnostics-json.md`), so non-zero exits are data, not
     * failures.
     *
     * @param  list<string>  $command
     * @return array{stdout: string, stderr: string, exit: int}
     */
    private static function run(array $command): array
    {
        $process = proc_open($command, [1 => ['pipe', 'w'], 2 => ['pipe', 'w']], $pipes);
        if (! is_resource($process)) {
            throw new BridgeException('cannot run: '.implode(' ', $command));
        }
        $stdout = (string) stream_get_contents($pipes[1]);
        fclose($pipes[1]);
        $stderr = (string) stream_get_contents($pipes[2]);
        fclose($pipes[2]);
        $exit = proc_close($process);

        return ['stdout' => $stdout, 'stderr' => $stderr, 'exit' => $exit];
    }
}
