<?php

declare(strict_types=1);

namespace Lintl\Twig\Tests\Component;

use Lintl\Html;
use Lintl\Twig\AsLintlComponent;

use function Lintl\e;

/** The §11 DI fixture: a service component (trusted markup, prop escaped). */
#[AsLintlComponent(name: 'Badge')]
final class BadgeComponent
{
    public function __invoke(string $name = ''): Html
    {
        return Html::unsafeRaw('<b class="badge">Hello, '.e($name).'</b>');
    }
}
