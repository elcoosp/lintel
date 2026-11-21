<?php

declare(strict_types=1);

namespace Lintel\Twig\Tests\Component;

use Lintel\Twig\AsLintelComponent;

/** The §11 DI fixture: an attributed class with no `__invoke` — the
 * bridge must fail loudly, not silently. */
#[AsLintelComponent(name: 'Dead')]
final class NotInvokable
{
    public function render(string $name): string
    {
        return $name;
    }
}
