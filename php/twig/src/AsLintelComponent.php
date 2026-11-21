<?php

declare(strict_types=1);

namespace Lintel\Twig;

/**
 * The §11 Symfony attribute (§17.1-Q2's remaining Symfony surface): marks
 * a PHP class as a Lintel component the DI container should expose to the
 * Twig bridge — `#[AsLintelComponent(name: 'Badge')]` on a class with an
 * `__invoke(...)` (props as named arguments) returning the markup child
 * (string / `\Lintel\Html`), rendered through the same `c()` escapers as
 * the file components. The {@see ComponentPass} registers the tagged
 * services; `Extension::render()` resolves `lintel.component.<name>`
 * before falling back to the `.lintel.php` sources.
 */
#[\Attribute(\Attribute::TARGET_CLASS)]
final class AsLintelComponent
{
    public function __construct(
        /** The component name the Twig template writes (`lintel_render('Badge', …)`). */
        public readonly string $name,
    ) {}
}
