<?php

declare(strict_types=1);

namespace Lintl\Twig;

use Symfony\Component\DependencyInjection\ContainerInterface;
use Symfony\Component\DependencyInjection\Exception\ServiceNotFoundException;

/**
 * The §11 Symfony component locator (§17.1-Q2's remaining Symfony
 * surface): resolves a component name to its `#[AsLintlComponent]`
 * service via the `lintl.component.<name>` aliases the
 * {@see ComponentPass} registers. Unknown names yield `null` — the
 * {@see Extension} falls back to the `.lintl.php` sources, so the file
 * lane and the service lane compose instead of competing.
 */
final class ComponentLocator
{
    public function __construct(
        private readonly ContainerInterface $container,
    ) {}

    /** The service component for `name`, or `null` when none is tagged.
     *
     * @throws ServiceNotFoundException when the alias exists but the
     *                                  service cannot be built (a miswired definition — surfaced, not
     * swallowed, so a broken component fails loudly like the file lane) */
    public function get(string $name): ?object
    {
        $id = ComponentPass::ALIAS_PREFIX.$name;
        if (! $this->container->has($id)) {
            return null;
        }

        return $this->container->get($id);
    }
}
