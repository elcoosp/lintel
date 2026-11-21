<?php

declare(strict_types=1);

namespace Lintel\Twig\Tests\Component;

use Lintel\Html;
use Lintel\Twig\AsLintelComponent;

use function Lintel\e;

/** The §11 DI fixture: a service component (trusted markup, prop escaped). */
#[AsLintelComponent(name: 'Badge')]
final class BadgeComponent
{
    public function __invoke(string $name = ''): Html
    {
        return Html::unsafeRaw('<b class="badge">Hello, ' . e($name) . '</b>');
    }
}
