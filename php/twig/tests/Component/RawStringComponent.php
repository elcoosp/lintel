<?php

declare(strict_types=1);

namespace Lintel\Twig\Tests\Component;

use Lintel\Twig\AsLintelComponent;

/** The §11 DI fixture: a service component returning a plain string —
 * c() owns its escaping. */
#[AsLintelComponent(name: 'Raw')]
final class RawStringComponent
{
    public function __invoke(string $name = '<script>pwned</script>'): string
    {
        return '<em>' . $name . '</em>';
    }
}
