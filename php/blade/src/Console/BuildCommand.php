<?php

declare(strict_types=1);

namespace Lintl\Blade\Console;

/**
 * `artisan lintl:build` (§11 Laravel wrapper): compiles the app's
 * `.lintl.php` sources with the lintl binary — artifacts into the
 * configured cache dir and the T-31 `lintl.json` manifest into the
 * workspace root (the common ancestor of the source dirs and the cache
 * dir — the app root, wherever artisan runs from) — so a prod deploy is
 * warmed by the same command the app boots with.
 */
final class BuildCommand extends Command
{
    protected $signature = 'lintl:build';

    protected $description = 'Compile the Lintl sources (§11): artifacts into the cache dir + the lintl.json manifest';

    public function handle(): int
    {
        return $this->runLintl([
            'build',
            ...$this->sourceDirs(),
            '--out',
            $this->cacheDir(),
            '--manifest',
        ]);
    }
}
