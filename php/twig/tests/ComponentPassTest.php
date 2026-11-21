<?php

declare(strict_types=1);

namespace Lintel\Twig\Tests;

use Lintel\Twig\AsLintelComponent;
use Lintel\Twig\ComponentLocator;
use Lintel\Twig\ComponentPass;
use Lintel\Twig\Extension;
use PHPUnit\Framework\TestCase;
use Symfony\Component\DependencyInjection\ContainerBuilder;
use Twig\Environment;
use Twig\Loader\ArrayLoader;


/**
 * The §11 Symfony DI lane (§17.1-Q2's remaining Symfony surface): the
 * `#[AsLintelComponent]` attribute + the {@see ComponentPass} register
 * service components on a real `ContainerBuilder`, and the bridge's
 * `lintel_render()` resolves them through {@see ComponentLocator}.
 * The service lane keeps the escape contract: trusted markup ships as
 * `\Lintel\Html` (the shape compiled components return — the author
 * escapes dynamic props through `e()`), and a plain string output goes
 * through `c()`'s escapers like any child value. Untagged names fall
 * back to the `.lintel.php` sources.
 */
final class ComponentPassTest extends TestCase
{
    /** The pass tags + aliases attributed definitions; unattributed and
     * placeholder definitions stay alone. */
    public function testPassRegistersServiceComponents(): void
    {
        $container = new ContainerBuilder();
        $container->register(Component\BadgeComponent::class)
            ->addTag('app.plain') // a definition that existed before the pass ran
            ->setAutowired(true);
        $container->register(\stdClass::class); // unattributed — must stay untagged
        $container->register('%param.class%'); // a placeholder — must be left alone
        $container->setParameter('param.class', 'NoSuchClass');

        (new ComponentPass())->process($container);

        $definition = $container->getDefinition(Component\BadgeComponent::class);
        self::assertSame([['name' => 'Badge']], $definition->getTag(ComponentPass::TAG));
        self::assertTrue($definition->isPublic(), 'service components resolve at runtime');

        $alias = $container->getAlias(ComponentPass::ALIAS_PREFIX . 'Badge');
        self::assertSame(Component\BadgeComponent::class, (string) $alias);
        self::assertTrue($alias->isPublic());

        self::assertSame([], $container->getDefinition(\stdClass::class)->getTags(), 'no attribute, no tag');
    }

    /** The container builds the service; the locator resolves the alias;
     * the bridge renders it — the hostile prop escaped through `e()`. */
    public function testBridgeRendersServiceComponent(): void
    {
        $container = new ContainerBuilder();
        $container->register(Component\BadgeComponent::class)->setPublic(true);
        (new ComponentPass())->process($container);
        $container->compile();

        $locator = new ComponentLocator($container);
        self::assertInstanceOf(Component\BadgeComponent::class, $locator->get('Badge'));
        self::assertNull($locator->get('Missing'), 'untagged names fall back to the file lane');

        $extension = new Extension(
            sys_get_temp_dir() . '/lintel-twig-services-' . bin2hex((string) random_bytes(4)),
            [__DIR__ . '/fixture'],
            $locator,
        );
        self::assertSame('<b class="badge">Hello, &lt;u&gt;pwn&lt;/u&gt;</b>', $extension->render('Badge', ['name' => '<u>pwn</u>']));
    }

    /** A service returning a plain string prints through c() — the same
     * escapers the file lane renders through, so a service component is
     * not an escape bypass either. */
    public function testServiceComponentStringOutputStaysEscaped(): void
    {
        $container = new ContainerBuilder();
        $container->register(Component\RawStringComponent::class);
        (new ComponentPass())->process($container);
        $container->compile();

        $extension = new Extension(
            sys_get_temp_dir() . '/lintel-twig-services-' . bin2hex((string) random_bytes(4)),
            [],
            new ComponentLocator($container),
        );

        self::assertSame('&lt;em&gt;&lt;script&gt;pwned&lt;/script&gt;&lt;/em&gt;', $extension->render('Raw'));
    }

    /** The file lane still serves untagged names — the lanes compose. */
    public function testFileLaneServesUntaggedComponents(): void
    {
        $locator = new ComponentLocator(new ContainerBuilder());
        $extension = new Extension(
            sys_get_temp_dir() . '/lintel-twig-services-' . bin2hex((string) random_bytes(4)),
            [__DIR__ . '/fixture'],
            $locator,
        );
        self::assertSame('<b class="badge">Hello, file lane</b>', $extension->render('Badge', ['name' => 'file lane']));
    }

    /** A service component without `__invoke` fails loudly, not silently. */
    public function testNonInvokableServiceComponentThrows(): void
    {
        $container = new ContainerBuilder();
        $container->register(Component\NotInvokable::class);
        (new ComponentPass())->process($container);
        $container->compile();

        $extension = new Extension(
            sys_get_temp_dir() . '/lintel-twig-services-' . bin2hex((string) random_bytes(4)),
            [],
            new ComponentLocator($container),
        );

        $this->expectException(\Lintel\Twig\BridgeException::class);
        $this->expectExceptionMessage('does not define __invoke()');
        $extension->render('Dead', ['name' => 'x']);
    }

    /** The attribute drives a Twig template end-to-end. */
    public function testServiceComponentInsideTwigTemplate(): void
    {
        $container = new ContainerBuilder();
        $container->register(Component\BadgeComponent::class);
        (new ComponentPass())->process($container);
        $container->compile();

        $twig = new Environment(new ArrayLoader(['p.twig' => '{{ lintel_render("Badge", {name: user}) }}']));
        $twig->addExtension(new Extension(
            sys_get_temp_dir() . '/lintel-twig-services-' . bin2hex((string) random_bytes(4)),
            [],
            new ComponentLocator($container),
        ));

        self::assertSame('<b class="badge">Hello, &lt;i&gt;twig&lt;/i&gt;</b>', $twig->render('p.twig', ['user' => '<i>twig</i>']));
    }
}

