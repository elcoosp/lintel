<?php

declare(strict_types=1);

namespace Lintl\Blade\Tests;

use Illuminate\Filesystem\Filesystem;
use Illuminate\View\Compilers\BladeCompiler;
use Lintl\Blade\Bridge;
use Lintl\Blade\BridgeException;
use Lintl\Blade\Directive;
use PHPUnit\Framework\TestCase;

use function Lintl\c;

/**
 * The §11 Blade interop bridge (§17.1-Q2 "Laravel & Symfony packages"):
 * the `@lintl` directive registers on a real `BladeCompiler`
 * (illuminate/view, the same major the bench harness pins), compiles to
 * an echo of the bridge's render call, and executing the compiled string
 * renders a `.lintl.php` component loaded through the dev loader — with
 * the hostile payload escaping through the runtime's `c()`.
 */
final class BladeBridgeTest extends TestCase
{
    protected function setUp(): void
    {
        // The directive's bridge is process-static — reset between tests.
        $ref = new \ReflectionProperty(Directive::class, 'bridge');
        $ref->setValue(null, null);
    }

    /** The directive compiles and the compiled string renders the component. */
    public function test_directive_compiles_and_renders(): void
    {
        $blade = new BladeCompiler(new Filesystem, sys_get_temp_dir().'/lintl-blade-cache');
        Directive::register($blade, $this->bridge());

        $compiled = $blade->compileString('@lintl(\'Badge\', [\'name\' => $user])');
        self::assertStringContainsString("echo \\Lintl\\Blade\\Directive::render('Badge', ['name' => \$user]);", $compiled);

        $user = '<script>alert(1)</script>';
        \ob_start();
        try {
            eval('?>'.$compiled);
        } finally {
            $out = \ob_get_clean();
        }
        self::assertNotFalse($out);
        self::assertSame('<b class="badge">Hello, &lt;script&gt;alert(1)&lt;/script&gt;</b>', $out);
    }

    /** The bridge loads the component through the dev loader and renders
     * through c(); a second render reuses the compiled cache. */
    public function test_bridge_loads_compiles_and_escapes(): void
    {
        $bridge = $this->bridge();

        $first = $bridge->render('Badge', ['name' => 'W"orld']);
        self::assertSame('<b class="badge">Hello, W&quot;orld</b>', $first);
        self::assertTrue(\function_exists('Badge'), 'the dev loader declared the component');

        // The loader's mtime+size cache serves the second render.
        $second = $bridge->render('Badge', ['name' => 'again']);
        self::assertSame('<b class="badge">Hello, again</b>', $second);
    }

    /** A component with no source dir entry throws with the searched dirs. */
    public function test_unknown_component_throws(): void
    {
        $bridge = $this->bridge();
        $this->expectException(BridgeException::class);
        $this->expectExceptionMessage('No .lintl.php source for component "Missing"');
        $bridge->render('Missing');
    }

    /** `render()` before any register throws (the directive is unconfigured). */
    public function test_render_before_register_throws(): void
    {
        $this->expectException(BridgeException::class);
        $this->expectExceptionMessage('before Directive::register()');
        Directive::render('Badge');
    }

    private function bridge(): Bridge
    {
        $cacheDir = sys_get_temp_dir().'/lintl-blade-bridge-'.bin2hex((string) random_bytes(4));

        return new Bridge($cacheDir, [__DIR__.'/fixture']);
    }
}
