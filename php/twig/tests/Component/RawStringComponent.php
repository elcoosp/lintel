<?php

declare(strict_types=1);

namespace Lintl\Twig\Tests\Component;

use Lintl\Twig\AsLintlComponent;

/** The §11 DI fixture: a service component returning a plain string —
 * c() owns its escaping. */
#[AsLintlComponent(name: 'Raw')]
final class RawStringComponent
{
    public function __invoke(string $name = '<script>pwned</script>'): string
    {
        return '<em>'.$name.'</em>';
    }
}
