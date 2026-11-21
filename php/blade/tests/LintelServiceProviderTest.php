<?php

declare(strict_types=1);

namespace Lintel\Blade\Tests;

use Illuminate\Config\Repository;
use Illuminate\Filesystem\Filesystem;
use Illuminate\Foundation\Application;
use Illuminate\View\Compilers\BladeCompiler;
use Lintel\Blade\Bridge;
use Lintel\Blade\Directive;
use Lintel\Blade\LintelServiceProvider;
use Lintel\Blade\Profiler\Panel;
use PHPUnit\Framework\TestCase;

/**
 * The §11 service provider (§17.1-Q2's remaining Laravel surface): boots
 * against a real Foundation `Application` — the framework's own
 * container, config repository, and path helpers — and proves the bridge
 * and panel resolve from the `lintel.*` config, the `@lintel` directive
 * renders against the bound bridge, and the compiled cache lands in the
 * configured dir. The component has a suite-unique name: the function
 * table is process-global, so the shared fixture would be served by
 * `function_exists` before the loader ran.
 */
final class LintelServiceProviderTest extends TestCase
{
    protected function setUp(): void
    {
        // The directive's bridge is process-static — reset between tests.
        $ref = new \ReflectionProperty(Directive::class, 'bridge');
        $ref->setValue(null, null);
    }

    /** The provider binds the bridge from config; `@lintel` renders. */
    public function testProviderBindsBridgeAndRegistersDirective(): void
    {
        $name = 'Prv' . bin2hex((string) random_bytes(4));
        $root = sys_get_temp_dir() . '/lintel-provider-' . bin2hex((string) random_bytes(4));
        \mkdir($root . '/src', 0777, true);
        \file_put_contents($root . '/src/' . $name . '.lintel.php', <<<LINTEL
        <?php
        function {$name}(string \$name = '') {
            return <b class="badge">Hello, {\$name}</b>;
        }
        LINTEL);

        $app = new Application($root);
        $app->instance('config', new Repository([
            'lintel' => [
                'cache_dir' => $root . '/cache',
                'source_dirs' => [$root . '/src'],
            ],
        ]));
        $app->instance('blade.compiler', new BladeCompiler(new Filesystem(), $root . '/cache/blade'));

        $provider = new LintelServiceProvider($app);
        $provider->register();
        $provider->boot();

        $bridge = $app->make(Bridge::class);
        self::assertInstanceOf(Bridge::class, $bridge);

        // The directive resolves against the bound bridge (config paths).
        $out = Directive::render($name, ['name' => 'DI']);
        self::assertSame('<b class="badge">Hello, DI</b>', $out);

        // The compiled cache lives in the configured dir, not somewhere else.
        self::assertNotEmpty(\glob($root . '/cache/*.php'), 'the dev loader compiled into the configured cache dir');

        // The panel singleton resolves too (the Debugbar collector's source).
        self::assertInstanceOf(Panel::class, $app->make(Panel::class));
    }
}
