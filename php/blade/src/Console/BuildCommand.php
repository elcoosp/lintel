<?php

declare(strict_types=1);

namespace Lintel\Blade\Console;

/**
 * `artisan lintel:build` (§11 Laravel wrapper): compiles the app's
 * `.lintel.php` sources with the lintel binary — artifacts into the
 * configured cache dir and the T-31 `lintel.json` manifest into the
 * working directory (artisan runs from the project root) — so a prod
 * deploy is warmed by the same command the app boots with.
 */
final class BuildCommand extends Command
{
    protected $signature = 'lintel:build';
    protected $description = 'Compile the Lintel sources (§11): artifacts into the cache dir + the lintel.json manifest';

    public function handle(): int
    {
        return $this->runLintel([
            'build',
            ...$this->sourceDirs(),
            '--out',
            $this->cacheDir(),
            '--manifest',
        ]);
    }
}
