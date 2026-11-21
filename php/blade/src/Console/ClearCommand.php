<?php

declare(strict_types=1);

namespace Lintel\Blade\Console;

/**
 * `artisan lintel:clear` (§11 Laravel wrapper): wipes the dev loader's
 * compile cache (the `<hash>-<mtime>-<size>.php` keys and any `.tmp`
 * leftovers) so the next request recompiles from the sources — the
 * Laravel-native counterpart of deleting the cache dir by hand.
 */
final class ClearCommand extends Command
{
    protected $signature = 'lintel:clear';
    protected $description = 'Wipe the dev loader compile cache';

    public function handle(): int
    {
        $dir = $this->cacheDir();
        if (!\is_dir($dir)) {
            $this->info("lintel cache: nothing to clear (no cache at {$dir})");

            return self::SUCCESS;
        }
        $cleared = 0;
        foreach (\glob($dir . '/*.php') ?: [] as $file) {
            if (@\unlink($file)) {
                ++$cleared;
            }
        }
        foreach (\glob($dir . '/*.tmp') ?: [] as $file) {
            @\unlink($file);
        }
        $this->info("cleared {$cleared} cached compilation(s) from {$dir}");

        return self::SUCCESS;
    }
}
