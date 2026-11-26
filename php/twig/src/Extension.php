<?php

declare(strict_types=1);

namespace Lintl\Twig;

use Lintl\Dev\Loader;
use Twig\Extension\AbstractExtension;
use Twig\TwigFunction;

use function Lintl\c;

/**
 * The §11 Symfony interop bridge (§17.1-Q2 "Laravel & Symfony packages"):
 * exposes `lintl_render('Badge', {name: user})` as a Twig function.
 * Rendering goes through the dev loader (`Lintl\Dev\Loader::require`,
 * the §6.10 compile-if-stale lane) for `.lintl.php` sources — prod builds
 * autoload instead — and the returned value prints through the runtime's
 * `c()`, so a Twig-embedded component is not an escape bypass. Component
 * arguments spread as PHP named arguments.
 *
 * The §11 service lane composes with the file lane: a
 * {@see ComponentLocator} (fed the DI container, its `#[AsLintlComponent]`
 * services registered by {@see ComponentPass}) resolves a component name
 * to an invokable service FIRST — `__invoke(props…)`, rendered through
 * the same `c()` — and only untagged names fall back to the sources.
 */
final class Extension extends AbstractExtension
{
    /** @param list<string> $sourceDirs dirs holding `<Name>.lintl.php` sources */
    public function __construct(
        private readonly string $cacheDir,
        private readonly array $sourceDirs,
        private readonly ?ComponentLocator $components = null,
    ) {}

    /** @return list<TwigFunction> */
    public function getFunctions(): array
    {
        // The function's output is markup the runtime's escapers already
        // escaped — marked safe so Twig's own autoescape doesn't
        // double-escape it (the escape contract stays with c()).
        return [new TwigFunction('lintl_render', [$this, 'render'], ['is_safe' => ['all']])];
    }

    /** Render `<Name>(...$args)` through the Lintl runtime.
     * @param array<string, mixed> $args spread as PHP named arguments */
    public function render(string $component, array $args = []): string
    {
        $service = $this->components?->get($component);
        if ($service !== null) {
            if (! \is_callable($service)) {
                throw new BridgeException("service component \"$component\" does not define __invoke()");
            }

            return c($service(...$args));
        }
        if (! \function_exists($component)) {
            $this->load($component);
        }
        if (! \function_exists($component)) {
            throw new BridgeException("No .lintl.php source declares component \"$component\"");
        }

        return c($component(...$args));
    }

    /** Resolve `<Name>.lintl.php` in the source dirs; compile-if-stale via
     * the dev loader (requiring the cached PHP declares the function). */
    private function load(string $component): void
    {
        foreach ($this->sourceDirs as $dir) {
            $file = $dir.'/'.$component.'.lintl.php';
            if (\is_file($file)) {
                Loader::require($file, $this->cacheDir);

                return;
            }
        }
        throw new BridgeException("No .lintl.php source for component \"$component\" in [".\implode(', ', $this->sourceDirs).']');
    }
}
