<?php

declare(strict_types=1);

namespace Lintl\Twig\Tests;

use Lintl\Twig\Extension;
use PHPUnit\Framework\TestCase;
use Twig\Environment;
use Twig\Error\RuntimeError;
use Twig\Loader\ArrayLoader;
use Twig\Node\Node;
use Twig\TwigFunction;

/**
 * The §11 Symfony interop bridge (§17.1-Q2 "Laravel & Symfony packages"):
 * `lintl_render()` renders a `.lintl.php` component through the dev
 * loader + the runtime escapers, inside a real Twig Environment — with
 * Twig's autoescape ON, which the function is marked safe against (its
 * output is pre-escaped by c(); double-escaping would corrupt it).
 */
final class TwigBridgeTest extends TestCase
{
    /** The function renders the component inside a Twig template, escaped by the runtime. */
    public function test_lintl_render_renders_component_in_twig(): void
    {
        $twig = new Environment(new ArrayLoader([
            'page.twig' => '<div>{{ lintl_render(\'Badge\', {name: user}) }}</div>',
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
    public function test_hostile_component_output_stays_escaped(): void
    {
        $twig = new Environment(new ArrayLoader(['p.twig' => '{{ lintl_render("Hostile") }}']));
        $twig->addExtension($this->extension());

        $out = $twig->render('p.twig');
        self::assertSame('<em>&lt;script&gt;pwned&lt;/script&gt;</em>', $out);
    }

    /** Unknown components throw (Twig surfaces it as a RuntimeError). */
    public function test_unknown_component_throws(): void
    {
        $twig = new Environment(new ArrayLoader(['p.twig' => '{{ lintl_render("Missing") }}']));
        $twig->addExtension($this->extension());
        $this->expectException(RuntimeError::class);
        $this->expectExceptionMessage('No .lintl.php source for component "Missing"');
        $twig->render('p.twig');
    }

    /** The function is exposed as `lintl_render`, safe for markup. */
    public function test_function_is_registered_safe_for_markup(): void
    {
        $ext = $this->extension();
        $fns = $ext->getFunctions();
        self::assertCount(1, $fns);
        self::assertInstanceOf(TwigFunction::class, $fns[0]);
        self::assertSame('lintl_render', $fns[0]->getName());
        // getSafe() returns the is_safe option verbatim (args never consulted).
        self::assertSame(['all'], $fns[0]->getSafe(new Node([])));
    }

    private function extension(): Extension
    {
        $cacheDir = sys_get_temp_dir().'/lintl-twig-bridge-'.bin2hex((string) random_bytes(4));

        return new Extension($cacheDir, [__DIR__.'/fixture']);
    }
}
