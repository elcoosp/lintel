<?php

declare(strict_types=1);

namespace Lintl\Blade\Console;

use Illuminate\Console\Command as IlluminateCommand;
use Lintl\Dev\Loader;
use Symfony\Component\Console\Output\OutputInterface;

/**
 * Base for the §11 `artisan lintl:*` wrappers (§17.1-Q2's remaining
 * Laravel surface): they wrap the SAME `lintl` binary the dev loader
 * spawns (LINTL_BIN → the Composer cache → PATH, via
 * `Loader::binary()`), with the paths coming from Laravel — the explicit
 * constructor override, `config('lintl.*')`, then Laravel's
 * storage/resource defaults — so the commands respect the environment
 * the app booted in.
 */
abstract class Command extends IlluminateCommand
{
    /** The explicit cache-dir override (tests and providers may pin it). */
    private readonly ?string $cacheDir;

    /** @var list<string>|null the explicit source-dir override */
    private readonly ?array $sourceDirs;

    /** @param list<string>|null $sourceDirs dirs holding `<Name>.lintl.php` */
    public function __construct(?string $cacheDir = null, ?array $sourceDirs = null)
    {
        $this->cacheDir = $cacheDir;
        $this->sourceDirs = $sourceDirs;
        parent::__construct();
    }

    /** The cache dir: the override, `config('lintl.cache_dir')`, or the
     * framework-cache default. */
    protected function cacheDir(): string
    {
        if ($this->cacheDir !== null) {
            return $this->cacheDir;
        }
        $configured = config('lintl.cache_dir');

        return \is_string($configured) && $configured !== '' ? $configured : storage_path('framework/cache/lintl');
    }

    /** The source dirs: the override, `config('lintl.source_dirs')`, or
     * the resource default. Non-string config entries are skipped.
     *
     * @return list<string> */
    protected function sourceDirs(): array
    {
        if ($this->sourceDirs !== null) {
            return $this->sourceDirs;
        }
        $configured = config('lintl.source_dirs');

        if (\is_array($configured)) {
            return \array_values(\array_filter($configured, \is_string(...)));
        }

        return [resource_path('views/lintl')];
    }

    /**
     * Run the lintl binary, streaming stdout as plain lines and stderr
     * as error lines (RAW — diagnostics carry raw `<…>` markup the output
     * formatter must not eat), and translate the exit code.
     *
     * @param  list<string>  $args  the argument vector after the binary
     */
    protected function runLintl(array $args): int
    {
        $command = [Loader::binary(), ...$args];
        $process = @\proc_open($command, [
            1 => ['pipe', 'w'],
            2 => ['pipe', 'w'],
        ], $pipes);
        if (! \is_resource($process)) {
            $this->error('could not start '.Loader::binary());

            return self::FAILURE;
        }
        $stdout = '';
        $stderr = '';
        while (! \feof($pipes[1]) || ! \feof($pipes[2])) {
            $read = [$pipes[1], $pipes[2]];
            $write = null;
            $except = null;
            if (@\stream_select($read, $write, $except, 5) === false) {
                break;
            }
            foreach ($read as $pipe) {
                $chunk = (string) \fread($pipe, 65536);
                if ($pipe === $pipes[1]) {
                    $stdout .= $chunk;
                } else {
                    $stderr .= $chunk;
                }
            }
        }
        \fclose($pipes[1]);
        \fclose($pipes[2]);
        $exit = \proc_close($process);

        foreach ($this->lines($stdout) as $line) {
            $this->line($line);
        }
        foreach ($this->lines($stderr) as $line) {
            $this->output->writeln($line, OutputInterface::OUTPUT_RAW);
        }

        return $exit === 0 ? self::SUCCESS : self::FAILURE;
    }

    /** Split a pipe into trimmed, non-empty lines.
     *
     * @return list<string> */
    private function lines(string $pipe): array
    {
        $trimmed = \trim($pipe);
        if ($trimmed === '') {
            return [];
        }

        return \array_values(\array_filter(\preg_split('/\R/', $trimmed) ?: [], \is_string(...)));
    }
}
