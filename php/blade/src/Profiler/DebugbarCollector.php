<?php

declare(strict_types=1);

namespace Lintel\Blade\Profiler;

use DebugBar\DataCollector\DataCollectorInterface;
use DebugBar\DataCollector\Renderable;

/**
 * The §11 Laravel Debugbar collector (§17.1-Q2's remaining Laravel
 * surface): registers as a `lintel` tab whose widget lists the profiler
 * metrics — files compiled, cache hits/misses, compilation time, suspense
 * boundaries flushed — captured from a real render window. Apps shipping
 * laravel-debugbar add it with one line (`Debugbar::addCollector(new
 * DebugbarCollector(app(Panel::class)))`); the interface here is
 * `maximebf/debugbar`'s, the library underneath it.
 */
final class DebugbarCollector implements DataCollectorInterface, Renderable
{
    public function __construct(private readonly Panel $panel) {}

    /** Called by the Debugbar when the toolbar snapshot renders.
     * @return array{files: int, cache_hits: int, cache_misses: int, compile_time_ms: float, suspense_flushed: int} */
    public function collect(): array
    {
        return $this->panel->metrics();
    }

    /** The unique collector name — the tab id and the widget map key. */
    public function getName(): string
    {
        return 'lintel';
    }

    /** The toolbar widget: a variable list of the metric snapshot.
     * @return array<string, array{icon: string, title: string, widget: string, map: string, default: string}> */
    public function getWidgets(): array
    {
        return [
            'lintel' => [
                'icon' => 'bolt',
                'title' => 'Lintel',
                'widget' => 'PhpDebugBar.Widgets.VariableListWidget',
                'map' => 'lintel',
                'default' => '{}',
            ],
        ];
    }
}
