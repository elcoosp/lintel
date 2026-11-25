<?php

declare(strict_types=1);

namespace Lintel\Bench;

use Illuminate\Events\Dispatcher;
use Illuminate\Filesystem\Filesystem;
use Illuminate\View\Compilers\BladeCompiler;
use Illuminate\View\Engines\CompilerEngine;
use Illuminate\View\Engines\EngineResolver;
use Illuminate\View\Factory;
use Illuminate\View\FileViewFinder;
use League\Plates\Engine;
use Twig\Environment;
use Twig\Loader\FilesystemLoader;

/**
 * The competitor template engines (§9.2: hand-written echo as the floor,
 * Blade and Twig pre-compiled, Plates) wired standalone, plus the template
 * sources each scenario renders. Every engine renders the SAME data as the
 * Lintel/echo/rfc lanes; escaping is each engine's default (Blade `{{ }}`
 * and Twig autoescape use htmlspecialchars, Plates templates call e()
 * explicitly — the same ENT_QUOTES|SUBSTITUTE|HTML5 matrix).
 */
final class Engines
{
    public static function blade(string $viewsDir, string $cacheDir): object
    {
        $fs = new Filesystem;
        $compiler = new BladeCompiler($fs, $cacheDir);
        $engine = new CompilerEngine($compiler);
        $resolver = new EngineResolver;
        $resolver->register('blade', static fn () => $engine);
        $finder = new FileViewFinder($fs, [$viewsDir]);
        $factory = new Factory($resolver, $finder, new Dispatcher);

        return $factory;
    }

    public static function twig(string $viewsDir, string $cacheDir): object
    {
        $loader = new FilesystemLoader($viewsDir);

        return new Environment($loader, ['cache' => $cacheDir, 'autoescape' => 'html']);
    }

    public static function plates(string $viewsDir): object
    {
        return new Engine($viewsDir);
    }

    /** Write the competitor templates for one scenario into $viewsDir. */
    public static function writeTemplates(string $scenarioId, string $viewsDir): void
    {
        \is_dir($viewsDir) || \mkdir($viewsDir, 0777, true);
        $writes = [
            'S1' => [
                's1.blade.php' => BladeTemplates::S1,
                's1.html.twig' => TwigTemplates::S1,
                's1.php' => PlatesTemplates::S1,
            ],
            'S2' => [
                's2.blade.php' => BladeTemplates::S2,
                's2.html.twig' => TwigTemplates::S2,
                's2.php' => PlatesTemplates::S2,
            ],
            'S3' => [
                'node.blade.php' => BladeTemplates::S3Node,
                's3.blade.php' => BladeTemplates::S3,
                's3.html.twig' => TwigTemplates::S3,
                's3.php' => PlatesTemplates::S3,
                'node.php' => PlatesTemplates::S3Node,
            ],
            'S4' => [
                's4.blade.php' => BladeTemplates::S4,
                's4.html.twig' => TwigTemplates::S4,
                's4.php' => PlatesTemplates::S4,
            ],
        ];
        foreach ($writes[$scenarioId] ?? [] as $name => $content) {
            \file_put_contents($viewsDir.'/'.$name, $content);
        }
    }
}

/** Blade sources: `{{ }}` is htmlspecialchars(ENT_QUOTES) — the engine default. */
final class BladeTemplates
{
    public const S1 = <<<'BLADE'
<div class="page"><header class="hero"><h1>{{ $d['title'] }}</h1><nav><a href="https://lintel.sh">docs</a><a href="https://lintel.sh/play">playground</a></nav></header><main><section class="intro"><p>Markup is an expression. JSX compiles ahead of time to plain PHP string concat, line numbers are preserved, and every dynamic value is escaped by default.</p></section><section class="features"><div class="feature"><h2>Line preserving</h2><p>Stack traces and PHPStan report the author's line numbers, not the compiled file's.</p></div><div class="feature"><h2>Secure by default</h2><p>The unsafe escape hatch is one greppable call: Html::unsafeRaw.</p></div><div class="feature"><h2>Zero runtime</h2><p>Production runs vendor-free compiled PHP plus ~300 lines of helpers.</p></div><div class="feature"><h2>Measured</h2><p>Every performance claim links to this reproducible benchmark.</p></div></section><footer><small>{{ $d['note'] }} · {{ $d['year'] }} {{ $d['brand'] }}</small></footer></main></div>
BLADE;

    public const S2 = <<<'BLADE'
<table class="rows"><thead><tr><th>id</th><th>name</th><th>score</th></tr></thead><tbody>
@foreach ($d['rows'] as $r)
<tr><td>{{ $r['id'] }}</td><td>{{ $r['name'] }}</td><td>{{ $r['score'] }}</td></tr>
@endforeach
</tbody></table>
BLADE;

    /** Recursive node include: depth counts down, fan-out 3. */
    public const S3Node = <<<'BLADE'
<div class="node">level-{{ $depth }}
@if ($depth > 0)
@include('node', ['depth' => $depth - 1])
@include('node', ['depth' => $depth - 1])
@include('node', ['depth' => $depth - 1])
@endif
</div>
BLADE;

    public const S3 = <<<'BLADE'
@include('node', ['depth' => $d['depth']])
BLADE;

    public const S4 = <<<'BLADE'
<div class="links">
@foreach ($d['links'] as $i => $l)
<a href="{{ $l['href'] }}" class="btn{{ $l['active'] ? ' btn--on' : '' }}" data-i="{{ $i }}" data-kind="{{ $l['kind'] }}">{{ $l['label'] }}</a>
@endforeach
</div>
BLADE;
}

/** Twig sources: autoescape 'html' — the engine default. */
final class TwigTemplates
{
    public const S1 = <<<'TWIG'
<div class="page"><header class="hero"><h1>{{ d.title }}</h1><nav><a href="https://lintel.sh">docs</a><a href="https://lintel.sh/play">playground</a></nav></header><main><section class="intro"><p>Markup is an expression. JSX compiles ahead of time to plain PHP string concat, line numbers are preserved, and every dynamic value is escaped by default.</p></section><section class="features"><div class="feature"><h2>Line preserving</h2><p>Stack traces and PHPStan report the author's line numbers, not the compiled file's.</p></div><div class="feature"><h2>Secure by default</h2><p>The unsafe escape hatch is one greppable call: Html::unsafeRaw.</p></div><div class="feature"><h2>Zero runtime</h2><p>Production runs vendor-free compiled PHP plus ~300 lines of helpers.</p></div><div class="feature"><h2>Measured</h2><p>Every performance claim links to this reproducible benchmark.</p></div></section><footer><small>{{ d.note }} · {{ d.year }} {{ d.brand }}</small></footer></main></div>
TWIG;

    public const S2 = <<<'TWIG'
<table class="rows"><thead><tr><th>id</th><th>name</th><th>score</th></tr></thead><tbody>
{% for r in d.rows %}
<tr><td>{{ r.id }}</td><td>{{ r.name }}</td><td>{{ r.score }}</td></tr>
{% endfor %}
</tbody></table>
TWIG;

    /** Self-contained recursive macro: depth counts down, fan-out 3. */
    public const S3 = <<<'TWIG'
{% macro node(depth) %}<div class="node">level-{{ depth }}{% if depth > 0 %}{{ _self.node(depth - 1) }}{{ _self.node(depth - 1) }}{{ _self.node(depth - 1) }}{% endif %}</div>{% endmacro %}
{{ _self.node(d.depth) }}
TWIG;

    public const S4 = <<<'TWIG'
<div class="links">
{% for l in d.links %}
<a href="{{ l.href }}" class="btn{{ l.active ? ' btn--on' : '' }}" data-i="{{ loop.index0 }}" data-kind="{{ l.kind }}">{{ l.label }}</a>
{% endfor %}
</div>
TWIG;
}

/** Plates sources: native PHP templates, e() escapes with the same matrix. */
final class PlatesTemplates
{
    public const S1 = <<<'PLATES'
<div class="page"><header class="hero"><h1><?=$this->e($d['title'])?></h1><nav><a href="https://lintel.sh">docs</a><a href="https://lintel.sh/play">playground</a></nav></header><main><section class="intro"><p>Markup is an expression. JSX compiles ahead of time to plain PHP string concat, line numbers are preserved, and every dynamic value is escaped by default.</p></section><section class="features"><div class="feature"><h2>Line preserving</h2><p>Stack traces and PHPStan report the author's line numbers, not the compiled file's.</p></div><div class="feature"><h2>Secure by default</h2><p>The unsafe escape hatch is one greppable call: Html::unsafeRaw.</p></div><div class="feature"><h2>Zero runtime</h2><p>Production runs vendor-free compiled PHP plus ~300 lines of helpers.</p></div><div class="feature"><h2>Measured</h2><p>Every performance claim links to this reproducible benchmark.</p></div></section><footer><small><?=$this->e($d['note'])?> · <?=$this->e((string) $d['year'])?> <?=$this->e($d['brand'])?></small></footer></main></div>
PLATES;

    public const S2 = <<<'PLATES'
<table class="rows"><thead><tr><th>id</th><th>name</th><th>score</th></tr></thead><tbody>
<?php foreach ($d['rows'] as $r): ?>
<tr><td><?=$this->e((string) $r['id'])?></td><td><?=$this->e($r['name'])?></td><td><?=$this->e((string) $r['score'])?></td></tr>
<?php endforeach; ?>
</tbody></table>
PLATES;

    public const S3Node = <<<'PLATES'
<div class="node">level-<?=$this->e((string) $depth)?><?php if ($depth > 0): ?>
<?=$this->fetch('node', ['depth' => $depth - 1])?>
<?=$this->fetch('node', ['depth' => $depth - 1])?>
<?=$this->fetch('node', ['depth' => $depth - 1])?>
<?php endif; ?></div>
PLATES;

    public const S3 = <<<'PLATES'
<?=$this->fetch('node', ['depth' => $d['depth']])?>
PLATES;

    public const S4 = <<<'PLATES'
<div class="links">
<?php foreach ($d['links'] as $i => $l): ?>
<a href="<?=$this->e($l['href'])?>" class="btn<?=$l['active'] ? ' btn--on' : ''?>" data-i="<?=$this->e((string) $i)?>" data-kind="<?=$this->e($l['kind'])?>"><?=$this->e($l['label'])?></a>
<?php endforeach; ?>
</div>
PLATES;
}
