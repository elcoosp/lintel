<?php

declare(strict_types=1);

namespace Lintl\Twig\Tests\Component;

use Lintl\Twig\AsLintlComponent;

/** The §11 DI fixture: an attributed class with no `__invoke` — the
 * bridge must fail loudly, not silently. */
#[AsLintlComponent(name: 'Dead')]
final class NotInvokable
{
    public function render(string $name): string
    {
        return $name;
    }
}
