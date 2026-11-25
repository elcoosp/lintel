<?php

declare(strict_types=1);

namespace Lintel\Blade;

use Illuminate\Contracts\Config\Repository as ConfigRepository;
use Illuminate\Contracts\Foundation\Application;
use Illuminate\Support\ServiceProvider as IlluminateServiceProvider;
use Illuminate\View\Compilers\BladeCompiler;
use Lintel\Blade\Console\BuildCommand;
use Lintel\Blade\Console\CheckCommand;
use Lintel\Blade\Console\ClearCommand;
use Lintel\Blade\Profiler\Panel;

/**
 * The §11 Laravel service provider (§17.1-Q2's remaining Laravel
 * surface): binds the bridge (and the profiler panel) from the
 * `lintel.*` config — the environment's paths — registers `@lintel` on
 * the app's Blade compiler, and offers the `artisan lintel:*` wrappers.
 *
 * Laravel 5.5+ auto-discovers this provider from the package manifest
 * (`composer.json` `extra.laravel.providers`).
 */
final class LintelServiceProvider extends IlluminateServiceProvider
{
    /** Bind the bridge and the profiler panel from the `lintel.*` config. */
    public function register(): void
    {
        $this->app->singleton(Bridge::class, static function (Application $app): Bridge {
            $config = $app->make('config');
            if (! $config instanceof ConfigRepository) {
                throw new BridgeException('the "config" binding must be an Illuminate config repository');
            }
            $cacheDir = $config->get('lintel.cache_dir');
            $sourceDirs = $config->get('lintel.source_dirs');

            return new Bridge(
                \is_string($cacheDir) && $cacheDir !== '' ? $cacheDir : storage_path('framework/cache/lintel'),
                \is_array($sourceDirs) ? \array_values(\array_filter($sourceDirs, \is_string(...))) : [resource_path('views/lintel')],
            );
        });
        $this->app->singleton(Panel::class);
    }

    /** Register `@lintel` on the app's Blade compiler + the artisan wrappers. */
    public function boot(): void
    {
        $compiler = $this->app->make('blade.compiler');
        if (! $compiler instanceof BladeCompiler) {
            throw new BridgeException('the "blade.compiler" binding must be an Illuminate\\View\\Compilers\\BladeCompiler');
        }
        Directive::register($compiler, $this->app->make(Bridge::class));
        $this->commands([BuildCommand::class, CheckCommand::class, ClearCommand::class]);
    }
}
