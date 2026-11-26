<?php

declare(strict_types=1);

namespace Lintl\Twig;

use Symfony\Component\DependencyInjection\Compiler\CompilerPassInterface;
use Symfony\Component\DependencyInjection\ContainerBuilder;

/**
 * The §11 Symfony compiler pass (§17.1-Q2's remaining Symfony surface):
 * auto-registers `#[AsLintlComponent]` classes for the Twig bridge.
 * Every definition whose class carries the attribute is tagged
 * `lintl.component` and aliased `lintl.component.<name>` (public), so
 * the container autowires the service and {@see Extension} resolves the
 * component name against it. Definitions whose class is a `%parameter%`
 * placeholder or a not-yet-loadable string are left alone (a later pass
 * or runtime resolution sees them).
 */
final class ComponentPass implements CompilerPassInterface
{
    /** The tag the bridge resolves component names against. */
    public const TAG = 'lintl.component';

    /** The alias prefix: `lintl.component.<name>`. */
    public const ALIAS_PREFIX = 'lintl.component.';

    public function process(ContainerBuilder $container): void
    {
        foreach ($container->getDefinitions() as $id => $definition) {
            // `register($id)` without an explicit class implies the class
            // from the service id (ResolveClassPass's semantics) — honor
            // that here, before compile() has run in the tests.
            $class = $definition->getClass() ?? (\str_contains($id, '\\') ? $id : null);
            if ($class === null || ! \class_exists($class)) {
                continue;
            }
            $attribute = null;
            foreach ((new \ReflectionClass($class))->getAttributes(AsLintlComponent::class) as $reflection) {
                $attribute = $reflection->newInstance();
                break;
            }
            if ($attribute === null) {
                continue;
            }
            $definition->addTag(self::TAG, ['name' => $attribute->name]);
            $definition->setPublic(true);
            $container->setAlias(self::ALIAS_PREFIX.$attribute->name, $id)->setPublic(true);
        }
    }
}
