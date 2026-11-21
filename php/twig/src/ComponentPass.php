<?php

declare(strict_types=1);

namespace Lintel\Twig;

use Symfony\Component\DependencyInjection\ContainerBuilder;

/**
 * The §11 Symfony compiler pass (§17.1-Q2's remaining Symfony surface):
 * auto-registers `#[AsLintelComponent]` classes for the Twig bridge.
 * Every definition whose class carries the attribute is tagged
 * `lintel.component` and aliased `lintel.component.<name>` (public), so
 * the container autowires the service and {@see Extension} resolves the
 * component name against it. Definitions whose class is a `%parameter%`
 * placeholder or a not-yet-loadable string are left alone (a later pass
 * or runtime resolution sees them).
 */
final class ComponentPass implements \Symfony\Component\DependencyInjection\Compiler\CompilerPassInterface
{
    /** The tag the bridge resolves component names against. */
    public const TAG = 'lintel.component';

    /** The alias prefix: `lintel.component.<name>`. */
    public const ALIAS_PREFIX = 'lintel.component.';

    public function process(ContainerBuilder $container): void
    {
        foreach ($container->getDefinitions() as $id => $definition) {
            // `register($id)` without an explicit class implies the class
            // from the service id (ResolveClassPass's semantics) — honor
            // that here, before compile() has run in the tests.
            $class = $definition->getClass() ?? (\str_contains($id, '\\') ? $id : null);
            if ($class === null || !\class_exists($class)) {
                continue;
            }
            $attribute = null;
            foreach ((new \ReflectionClass($class))->getAttributes(AsLintelComponent::class) as $reflection) {
                $attribute = $reflection->newInstance();
                break;
            }
            if ($attribute === null) {
                continue;
            }
            $definition->addTag(self::TAG, ['name' => $attribute->name]);
            $definition->setPublic(true);
            $container->setAlias(self::ALIAS_PREFIX . $attribute->name, $id)->setPublic(true);
        }
    }
}
