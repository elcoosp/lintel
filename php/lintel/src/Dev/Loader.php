<?php

declare(strict_types=1);

namespace Lintel\Dev;

use Composer\Autoload\ClassLoader;

/**
 * Dev-only loader (architecture doc §6.10). In prod, `lintel build` output
 * is autoloaded by Composer as plain PHP — this class ships only inside the
 * dev `lintel/lintel` package and never runs in production.
 *
 * Two deviations from the §6.10 sketch, each forced by a failing test and
 * recorded as a drift correction in architecture §21.2:
 * 1. The cache key carries the file **size** alongside mtime — the doc's
 *    mtime-only key serves a same-second edit stale forever.
 * 2. The doc's `ClassLoader::class::getRegisteredLoaders()` line is invalid
 *    PHP (`php -l` rejects it); the correct static call is used.
 *
 * The T-12 daemon is gated, so the active compile path is the proc_open
 * fallback: `<binary> build --stdout <file>`. The `$socket` parameter is
 * transcribed and reserved for the daemon handoff.
 */
final class Loader
{
    /** Reserved for the gated T-12 daemon; unused until it lands. */
    private static ?string $sock = null;

    /** Process-global §11 metric counters — `LoaderMetrics` snapshots them. */
    private static int $metricFiles = 0;
    private static int $metricHits = 0;
    private static int $metricMisses = 0;
    private static float $metricCompileMs = 0.0;

    /** The counters so far (the §11 profiler panels' data source). */
    public static function metrics(): LoaderMetrics
    {
        return new LoaderMetrics(self::$metricFiles, self::$metricHits, self::$metricMisses, self::$metricCompileMs);
    }

    /** Zero the counters, bracketing a render window the panels measure. */
    public static function resetMetrics(): void
    {
        self::$metricFiles = 0;
        self::$metricHits = 0;
        self::$metricMisses = 0;
        self::$metricCompileMs = 0.0;
    }

    /** The socket the loader was booted against, for daemon-aware consumers. */
    public static function socket(): ?string
    {
        return self::$sock;
    }

    /**
     * Register the fallback autoloader and eager-require the `lintel.json`
     * function components. `Composer classmap/PSR-4 can't find
     * *.lintel.php; add a fallback autoloader` (§6.10).
     *
     * The T-31 order is deliberate: the function components are required
     * **before** the PSR-4 fallback registers. PHP has no function
     * autoloading — once `boot()` returns, a call to an undeclared function
     * fatals — so the eager lane must have completed by then. The compiled
     * component bodies resolve their classes (the runtime, other
     * components) lazily at call time, after the fallback has registered.
     *
     * When the T-31 prod shim ran in this process (it registers the
     * sources it loaded in `$GLOBALS['lintelFromSource']`), those sources
     * are skipped here: their functions already exist, and requiring the
     * source again would fatal on redeclare. The prod shim is authoritative
     * for the lanes it covers; the loader covers everything else (sources
     * added since the last `lintel build --manifest`, and projects without
     * a prod build).
     *
     * @throws \RuntimeException when a present lintel.json is malformed
     */
    public static function boot(string $cacheDir, ?string $socket = null, ?string $projectRoot = null): void
    {
        self::$sock = $socket;
        self::loadFunctionComponents($cacheDir, $projectRoot ?? (\getcwd() ?: '.'));
        \spl_autoload_register(static function (string $class) use ($cacheDir): void {
            foreach (ClassLoader::getRegisteredLoaders() as $loader) {
                foreach ($loader->getPrefixesPsr4() as $prefix => $dirs) {
                    if (!\str_starts_with($class, $prefix)) {
                        continue;
                    }
                    $rel = \str_replace('\\', '/', \substr($class, \strlen($prefix))) . '.lintel.php';
                    foreach ($dirs as $d) {
                        if (\is_file($f = $d . '/' . $rel)) {
                            self::require($f, $cacheDir);

                            return;
                        }
                    }
                }
            }
        }, true, true);
    }

    /**
     * Compile if stale (mtime+size key), then require the cached PHP.
     * OPcache caches it like any file. The doc's original wording holds:
     * the cache key hashes the source path and carries mtime — plus the
     * size (§21.2 drift correction).
     *
     * @throws \RuntimeException when the file cannot be stat'ed or compiled
     */
    public static function require(string $file, string $cacheDir): mixed
    {
        $mtime = \filemtime($file);
        $size = \filesize($file);
        if ($mtime === false || $size === false) {
            throw new \RuntimeException("cannot stat {$file}");
        }
        $key = $cacheDir . '/' . \hash('xxh3', $file) . '-' . $mtime . '-' . $size . '.php';
        ++self::$metricFiles;
        if (!\is_file($key)) {
            ++self::$metricMisses;
            if (!\is_dir($cacheDir)) {
                @\mkdir($cacheDir, 0777, true);
            }
            $started = \hrtime(true);
            $php = self::compile($file);
            self::$metricCompileMs += (\hrtime(true) - $started) / 1e6;
            \file_put_contents($key . '.tmp', $php);
            \rename($key . '.tmp', $key);
        } else {
            ++self::$metricHits;
        }

        return require $key;
    }

    /**
     * Resolve and run the lintel binary over proc_open
     * (`lintel build <file>`, which writes the compiled PHP to stdout —
     * §6.10's sketch wrote `--stdout`, a flag the real §6.7 CLI rejects;
     * proven by a failing test and recorded in §21.2).
     * The binary comes from the LINTEL_BIN override, the Composer cache at
     * `<vendor>/bin/lintel` (§12), or the PATH, in that order.
     *
     * @throws \RuntimeException when the binary fails or emits nothing
     */
    private static function compile(string $file): string
    {
        if (self::$sock !== null) {
            $daemon = self::compileOverSocket($file);
            if ($daemon !== null) {
                return $daemon;
            }
            // The socket is dead — fall through to the spawn fallback.
        }
        $command = [self::binary(), 'build', $file];
        $process = @\proc_open($command, [
            1 => ['pipe', 'w'],
            2 => ['pipe', 'w'],
        ], $pipes);
        if (!\is_resource($process)) {
            throw new \RuntimeException("lintel build failed for {$file}: could not start " . self::binary());
        }
        $stdout = (string) \stream_get_contents($pipes[1]);
        $stderr = (string) \stream_get_contents($pipes[2]);
        \fclose($pipes[1]);
        \fclose($pipes[2]);
        $exit = \proc_close($process);
        if ($exit !== 0 || $stdout === '') {
            throw new \RuntimeException("lintel build failed for {$file} (exit {$exit}): " . $stderr);
        }

        return $stdout;
    }

    /**
     * The T-12 daemon protocol (one request per connection): the client
     * sends `<abs path>\n`, the daemon answers `OK <len>\n<php>` or
     * `ERR <len>\n<rendered diagnostics>`. Returns `null` on any
     * transport failure (the caller falls back to proc_open); an `ERR`
     * answer is a *compile* failure, surfaced as the daemon rendered it.
     */
    private static function compileOverSocket(string $file): ?string
    {
        $stream = @\stream_socket_client('unix://' . self::$sock, $errno, $errstr, 5.0);
        if ($stream === false) {
            return null;
        }
        $written = @\fwrite($stream, $file . "\n");
        if ($written === false || \strpos($file, "\n") !== false) {
            \fclose($stream);

            return null;
        }
        $header = self::socketLine($stream);
        if ($header === null || !\preg_match('/^(OK|ERR) (\d+)$/', $header, $m)) {
            \fclose($stream);

            return null;
        }
        $length = (int) $m[2];
        $body = '';
        while (\strlen($body) < $length && !\feof($stream)) {
            $chunk = @\fread($stream, (int) \max(1, $length - \strlen($body)));
            if ($chunk === false || $chunk === '') {
                break;
            }
            $body .= $chunk;
        }
        \fclose($stream);
        if (\strlen($body) !== $length) {
            return null;
        }

        if ($m[1] === 'ERR') {
            // A compile failure the daemon already rendered — surface it
            // like the proc_open fallback surfaces a non-zero exit.
            throw new \RuntimeException("lintel build failed for {$file}: " . $body);
        }

        return $body;
    }

    /**
     * One `\n`-terminated line from the socket, or `null` on EOF/error.
     *
     * @param resource $stream the socket the daemon handshake returned
     */
    private static function socketLine($stream): ?string
    {
        $line = \fgets($stream);
        if ($line === false) {
            return null;
        }

        return \rtrim($line, "\r\n");
    }

    /**
     * The compile binary, in the same order every consumer of the loader
     * uses (the §11 artisan/console commands wrap the same binary): the
     * LINTEL_BIN override, the Composer cache at `<vendor>/bin/lintel`
     * (§12), or the PATH.
     */
    public static function binary(): string
    {
        $override = \getenv('LINTEL_BIN');
        if (\is_string($override) && $override !== '') {
            return $override;
        }
        try {
            $package = \Composer\InstalledVersions::getInstallPath('lintel/lintel');
        } catch (\Throwable) {
            $package = null;
        }
        if (\is_string($package) && $package !== '') {
            $vendorBin = \dirname($package, 2) . '/bin/lintel';
            if (\is_file($vendorBin)) {
                return $vendorBin;
            }
        }

        return 'lintel';
    }

    /**
     * §6.10's T-31 note: function-only components are not PSR-4
     * autoloadable — the loader requires them eagerly in dev. The
     * `lintel.json` manifest sits at the project root and lists `functions`
     * globs (`{"functions": ["components/*.lintel.php"]}`, a bare string is
     * accepted too); matches are compiled+cached through the same mtime+size
     * path, sorted for deterministic load order. `lintel build` emits the
     * Composer `files` manifest for prod.
     *
     * @throws \RuntimeException when the manifest is malformed
     */
    private static function loadFunctionComponents(string $cacheDir, string $root): void
    {
        $manifest = $root . '/lintel.json';
        if (!\is_file($manifest)) {
            return;
        }
        $decoded = \json_decode((string) \file_get_contents($manifest), true);
        if (!\is_array($decoded)) {
            throw new \RuntimeException("lintel.json at {$root} is not a valid JSON object");
        }
        $globs = $decoded['functions'] ?? null;
        if ($globs === null) {
            return;
        }
        if (\is_string($globs)) {
            $globs = [$globs];
        }
        if (!\is_array($globs)) {
            throw new \RuntimeException("lintel.json at {$root}: \"functions\" must be a string or a list of globs");
        }
        // The T-31 prod shim records the sources it required; a source it
        // loaded already defined this process's functions — skip it.
        $shimLoaded = $GLOBALS['lintelFromSource'] ?? null;
        foreach ($globs as $glob) {
            if (!\is_string($glob) || $glob === '') {
                throw new \RuntimeException("lintel.json at {$root}: \"functions\" entries must be non-empty strings");
            }
            $matches = \glob($root . '/' . $glob) ?: [];
            \sort($matches);
            foreach ($matches as $match) {
                if (!\is_file($match)) {
                    continue;
                }
                if (\is_array($shimLoaded)
                    && isset($shimLoaded[self::manifestRelative($match, $root)])) {
                    continue;
                }
                self::require($match, $cacheDir);
            }
        }
    }

    /**
     * A file's path relative to the manifest root, with normalized
     * separators — the key space the prod shim's registry uses.
     */
    private static function manifestRelative(string $file, string $root): string
    {
        $normalize = static fn (string $path): string => \str_replace('\\', '/', $path);
        $file = $normalize($file);
        $root = \rtrim($normalize($root), '/');
        if ($root !== '' && \str_starts_with($file, $root . '/')) {
            return \substr($file, \strlen($root) + 1);
        }

        return $file;
    }
}
