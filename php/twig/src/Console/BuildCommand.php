<?php

declare(strict_types=1);

namespace Lintel\Twig\Console;

use Lintel\Dev\Loader;
use Symfony\Component\Console\Attribute\AsCommand;
use Symfony\Component\Console\Command\Command;
use Symfony\Component\Console\Input\InputInterface;
use Symfony\Component\Console\Output\OutputInterface;

/**
 * `bin/console lintel:build` (§11 Symfony wrapper, §17.1-Q2's remaining
 * Symfony surface): compiles the app's `.lintel.php` sources with the
 * lintel binary — artifacts into the configured cache dir and the T-31
 * `lintel.json` manifest into the workspace root (the common ancestor of
 * the source dirs and the cache dir — the project root, wherever the
 * console runs from) — the same prod warm-up the artisan lane drives.
 * Paths arrive as constructor arguments (the kernel extension wires them
 * from the app's configuration), so the command respects the environment
 * it booted in. Diagnostics stream RAW — they carry raw `<…>` markup the
 * output formatter must not eat.
 */
#[AsCommand(name: 'lintel:build', description: 'Compile the Lintel sources (§11): artifacts into the cache dir + the lintel.json manifest')]
final class BuildCommand extends Command
{
    /** @param list<string> $sourceDirs dirs holding `<Name>.lintel.php` */
    public function __construct(
        private readonly string $cacheDir,
        private readonly array $sourceDirs,
    ) {
        parent::__construct();
    }

    protected function execute(InputInterface $input, OutputInterface $output): int
    {
        $command = [Loader::binary(), 'build', ...$this->sourceDirs, '--out', $this->cacheDir, '--manifest'];
        $process = @\proc_open($command, [
            1 => ['pipe', 'w'],
            2 => ['pipe', 'w'],
        ], $pipes);
        if (!\is_resource($process)) {
            $output->writeln('could not start ' . Loader::binary(), OutputInterface::OUTPUT_RAW);

            return self::FAILURE;
        }
        $stdout = '';
        $stderr = '';
        while (!\feof($pipes[1]) || !\feof($pipes[2])) {
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
            $output->writeln($line);
        }
        foreach ($this->lines($stderr) as $line) {
            $output->writeln($line, OutputInterface::OUTPUT_RAW);
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
