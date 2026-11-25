<?php

declare(strict_types=1);

namespace Lintel\Blade;

use Illuminate\View\Compilers\BladeCompiler;

/**
 * The `@lintel` Blade directive (§11 Laravel Blade interop):
 * `@lintel('Badge', ['name' => $user])` compiles to an echo of
 * `Lintel\Blade\render('Badge', ['name' => $user])` — the same call the
 * {@see Bridge} core renders. `configure()` sets the bridge the compiled
 * directive resolves against (a Laravel provider would call it from
 * `boot()` with the app's configured cache dir and source dirs).
 */
final class Directive
{
    /** The bridge the compiled directives resolve against. */
    private static ?Bridge $bridge = null;

    /** Register `@lintel` on a Blade compiler and point it at a bridge. */
    public static function register(BladeCompiler $blade, Bridge $bridge): void
    {
        self::$bridge = $bridge;
        $blade->directive('lintel', static function (string $expression): string {
            return '<?php echo \Lintel\Blade\Directive::render('.$expression.'); ?>';
        });
    }

    /** The compiled directive target (`@internal` — the directive calls it).
     * @param array<string, mixed> $args spread as PHP named arguments */
    public static function render(string $component, array $args = []): string
    {
        if (self::$bridge === null) {
            throw new BridgeException('@lintel used before Directive::register() configured a bridge');
        }

        return self::$bridge->render($component, $args);
    }
}
