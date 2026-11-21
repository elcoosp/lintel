<?php

declare(strict_types=1);

namespace Lintel\Blade;

use Lintel\Dev\Loader;

use function Lintel\c;

/**
 * The §11 Laravel interop core (§17.1-Q2 "Laravel & Symfony packages"):
 * renders a Lintel component invocation, reusing the dev loader
 * (`Lintel\Dev\Loader::require`, the §6.10 compile-if-stale lane) for
 * `.lintel.php` sources — prod builds autoload instead, the bridge never
 * compiles in production. Component arguments spread as PHP named
 * arguments (`Badge(...['name' => $x])` = `Badge(name: $x)`), and the
 * returned value renders through the runtime's `c()` — the SAME escapers
 * the compiler emitted, so a Blade-embedded component is not an escape
 * bypass.
 */
final class Bridge
{
    /** @param list<string> $sourceDirs dirs holding `<Name>.lintel.php` sources */
    public function __construct(
        private readonly string $cacheDir,
        private readonly array $sourceDirs,
    ) {}

    /** Render `<Name>(...$args)` through the Lintel runtime.
     * @param array<string, mixed> $args spread as PHP named arguments */
    public function render(string $component, array $args = []): string
    {
        if (!\function_exists($component)) {
            $this->load($component);
        }
        if (!\function_exists($component)) {
            throw new BridgeException("No .lintel.php source declares component \"$component\"");
        }

        return c($component(...$args));
    }

    /** Resolve `<Name>.lintel.php` in the source dirs and compile-if-stale
     * through the dev loader — requiring the cached PHP declares the
     * component function. */
    private function load(string $component): void
    {
        foreach ($this->sourceDirs as $dir) {
            $file = $dir . '/' . $component . '.lintel.php';
            if (\is_file($file)) {
                Loader::require($file, $this->cacheDir);

                return;
            }
        }
        throw new BridgeException("No .lintel.php source for component \"$component\" in [" . \implode(', ', $this->sourceDirs) . ']');
    }
}
