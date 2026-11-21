<?php

declare(strict_types=1);

namespace Lintel\Twig\Tests;

use PHPUnit\Framework\TestCase;
use Lintel\Twig\Extension;
use Twig\Environment;
use Twig\Loader\ArrayLoader;
use Twig\TwigFunction;

/**
 * The §11 Symfony interop bridge (§17.1-Q2 "Laravel & Symfony packages"):
 * `lintel_render()` renders a `.lintel.php` component through the dev
 * loader + the runtime escapers, inside a real Twig Environment — with
 * Twig's autoescape ON, which the function is marked safe against (its
 * output is pre-escaped by c(); double-escaping would corrupt it).
 */
final class TwigBridgeTest extends TestCase
{
    /** The function renders the component inside a Twig template, escaped by the runtime. */
    public function testLintelRenderRendersComponentInTwig(): void
    {
        $twig = new Environment(new ArrayLoader([
            'page.twig' => '<div>{{ lintel_render(\'Badge\', {name: user}) }}</div>',
        ])); // autoescape ON (the Twig default)
        $twig->addExtension($this->extension());

        $out = $twig->render('page.twig', ['user' => '<script>alert(1)</script>']);
        self::assertSame(
            '<div><b class="badge">Hello, &lt;script&gt;alert(1)&lt;/script&gt;</b></div>',
            $out,
        );
        self::assertTrue(\function_exists('Badge'), 'the dev loader declared the component');
    }

    /** A hostile component OUTPUT would still print escaped (c() owns the contract). */
    public function testHostileComponentOutputStaysEscaped(): void
    {
        $twig = new Environment(new ArrayLoader(['p.twig' => '{{ lintel_render("Hostile") }}']));
        $twig->addExtension($this->extension());

        $out = $twig->render('p.twig');
        self::assertSame('<em>&lt;script&gt;pwned&lt;/script&gt;</em>', $out);
    }

    /** Unknown components throw (Twig surfaces it as a RuntimeError). */
    public function testUnknownComponentThrows(): void
    {
        $twig = new Environment(new ArrayLoader(['p.twig' => '{{ lintel_render("Missing") }}']));
        $twig->addExtension($this->extension());
        $this->expectException(\Twig\Error\RuntimeError::class);
        $this->expectExceptionMessage('No .lintel.php source for component "Missing"');
        $twig->render('p.twig');
    }

    /** The function is exposed as `lintel_render`, safe for markup. */
    public function testFunctionIsRegisteredSafeForMarkup(): void
    {
        $ext = $this->extension();
        $fns = $ext->getFunctions();
        self::assertCount(1, $fns);
        self::assertInstanceOf(TwigFunction::class, $fns[0]);
        self::assertSame('lintel_render', $fns[0]->getName());
        // getSafe() returns the is_safe option verbatim (args never consulted).
        self::assertSame(['all'], $fns[0]->getSafe(new \Twig\Node\Node([])));
    }

    private function extension(): Extension
    {
        $cacheDir = sys_get_temp_dir() . '/lintel-twig-bridge-' . bin2hex((string) random_bytes(4));

        return new Extension($cacheDir, [__DIR__ . '/fixture']);
    }
}
