<?php

declare(strict_types=1);

namespace Lintl\Twig;

/**
 * The §11 Symfony attribute (§17.1-Q2's remaining Symfony surface): marks
 * a PHP class as a Lintl component the DI container should expose to the
 * Twig bridge — `#[AsLintlComponent(name: 'Badge')]` on a class with an
 * `__invoke(...)` (props as named arguments) returning the markup child
 * (string / `\Lintl\Html`), rendered through the same `c()` escapers as
 * the file components. The {@see ComponentPass} registers the tagged
 * services; `Extension::render()` resolves `lintl.component.<name>`
 * before falling back to the `.lintl.php` sources.
 */
#[\Attribute(\Attribute::TARGET_CLASS)]
final class AsLintlComponent
{
    public function __construct(
        /** The component name the Twig template writes (`lintl_render('Badge', …)`). */
        public readonly string $name,
    ) {}
}
