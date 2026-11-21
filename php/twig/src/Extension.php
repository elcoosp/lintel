<?php

declare(strict_types=1);

namespace Lintel\Twig;

use Lintel\Dev\Loader;
use Twig\Extension\AbstractExtension;
use Twig\TwigFunction;

use function Lintel\c;

/**
 * The §11 Symfony interop bridge (§17.1-Q2 "Laravel & Symfony packages"):
 * exposes `lintel_render('Badge', {name: user})` as a Twig function.
 * Rendering goes through the dev loader (`Lintel\Dev\Loader::require`,
 * the §6.10 compile-if-stale lane) for `.lintel.php` sources — prod builds
 * autoload instead — and the returned value prints through the runtime's
 * `c()`, so a Twig-embedded component is not an escape bypass. Component
 * arguments spread as PHP named arguments.
 */
final class Extension extends AbstractExtension
{
    /** @param list<string> $sourceDirs dirs holding `<Name>.lintel.php` sources */
    public function __construct(
        private readonly string $cacheDir,
        private readonly array $sourceDirs,
    ) {}

    /** @return list<TwigFunction> */
    public function getFunctions(): array
    {
        // The function's output is markup the runtime's escapers already
        // escaped — marked safe so Twig's own autoescape doesn't
        // double-escape it (the escape contract stays with c()).
        return [new TwigFunction('lintel_render', [$this, 'render'], ['is_safe' => ['all']])];
    }

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

    /** Resolve `<Name>.lintel.php` in the source dirs; compile-if-stale via
     * the dev loader (requiring the cached PHP declares the function). */
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
