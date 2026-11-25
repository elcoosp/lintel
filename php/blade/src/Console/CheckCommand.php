<?php

declare(strict_types=1);

namespace Lintel\Blade\Console;

/**
 * `artisan lintel:check` (§11 Laravel wrapper): compiles the app's
 * `.lintel.php` sources diagnostics-only — the binary exits non-zero
 * when any error-severity diagnostic fires, which this command maps to
 * the artisan FAILURE so CI and `deploys` feel it.
 */
final class CheckCommand extends Command
{
    protected $signature = 'lintel:check';

    protected $description = 'Compile the Lintel sources diagnostics-only; exits non-zero on errors';

    public function handle(): int
    {
        return $this->runLintel(['check', ...$this->sourceDirs()]);
    }
}
