<?php

declare(strict_types=1);

namespace Lintl\Blade\Console;

/**
 * `artisan lintl:check` (§11 Laravel wrapper): compiles the app's
 * `.lintl.php` sources diagnostics-only — the binary exits non-zero
 * when any error-severity diagnostic fires, which this command maps to
 * the artisan FAILURE so CI and `deploys` feel it.
 */
final class CheckCommand extends Command
{
    protected $signature = 'lintl:check';

    protected $description = 'Compile the Lintl sources diagnostics-only; exits non-zero on errors';

    public function handle(): int
    {
        return $this->runLintl(['check', ...$this->sourceDirs()]);
    }
}
