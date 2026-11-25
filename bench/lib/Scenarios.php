<?php

declare(strict_types=1);

namespace Lintel\Bench;

/**
 * The §9.2 scenarios S1–S5. Each scenario carries:
 *  - `source`: the .lintel.php source every lintel lane is compiled from
 *    (string / rfc / stream targets of the SAME source),
 *  - `lintel_call`: the compiled component invocation (data ⇒ output),
 *  - `competitors`: echo (hand-written floor), blade, twig, plates, and the
 *    compiled rfc lane, each rendering the SAME data shape.
 *
 * S5 differs: its lanes produce CHUNK STREAMS, and the harness records
 * time-to-first-chunk plus total render time (§9.2 "streaming TTFB").
 */
final class Scenarios
{
    /** @return array<int, string> the hostile payloads from the §7.2 corpus that stay in the data */
    public static function hostileNames(): array
    {
        return ['java'."\t".'script:alert(1)', '<script>alert(1)</script>', '" onmouseover="x()', 'javascript:alert(1)', 'JaVaScRiPt:alert(1)', '&#x6A;avascript:alert(1)', 'data:text/html;base64,PHNjcmlwdD4=', 'vbscript:msgbox'];
    }

    public static function s1Data(): array
    {
        return ['title' => 'Lintel Bench', 'brand' => 'lintel.sh', 'year' => 2026, 'note' => 'dynamic note'];
    }

    /** @return array<int, array{id: int, name: string, score: float}> */
    public static function s2Data(): array
    {
        $rows = [];
        $hostile = self::hostileNames();
        for ($i = 0; $i < 1000; $i++) {
            $rows[] = ['id' => $i, 'name' => $hostile[$i % \count($hostile)]." row $i", 'score' => \round($i / 7, 2)];
        }

        return $rows;
    }

    public static function s3Data(): array
    {
        return ['depth' => 10];
    }

    /** @return array<int, array{href: string, label: string, active: bool, kind: string}> */
    public static function s4Data(): array
    {
        $links = [];
        for ($i = 0; $i < 200; $i++) {
            $links[] = ['href' => "https://lintel.sh/item/$i", 'label' => "item $i", 'active' => $i % 3 === 0, 'kind' => 'nav-link'];
        }

        return $links;
    }

    public static function s5Data(): array
    {
        return ['ms' => 5, 'title' => 'Streaming TTFB'];
    }

    /** The shared preamble every .lintel.php scenario source uses. */
    private const SRC_HEAD = <<<'PHP'
<?php

declare(strict_types=1);

namespace Lintel\Bench\Gen;

use Lintel\Html;

PHP;

    /** @return array<string, array{title: string, source: string, lintel_call: string, competitors: array<string, callable}> */
    public static function all(string $compiledDir): array
    {
        return [
            'S1' => self::s1($compiledDir),
            'S2' => self::s2($compiledDir),
            'S3' => self::s3($compiledDir),
            'S4' => self::s4($compiledDir),
            'S5' => self::s5($compiledDir),
        ];
    }

    private static function s1(string $compiledDir): array
    {
        $source = self::SRC_HEAD.<<<'PHP'
function S1Page(array $d): Html
{
    return <div class="page">
        <header class="hero">
            <h1>{$d['title']}</h1>
            <nav><a href="https://lintel.sh">docs</a><a href="https://lintel.sh/play">playground</a></nav>
        </header>
        <main>
            <section class="intro"><p>Markup is an expression. JSX compiles ahead of time to plain PHP string concat, line numbers are preserved, and every dynamic value is escaped by default.</p></section>
            <section class="features">
                <div class="feature"><h2>Line preserving</h2><p>Stack traces and PHPStan report the author's line numbers, not the compiled file's.</p></div>
                <div class="feature"><h2>Secure by default</h2><p>The unsafe escape hatch is one greppable call: Html::unsafeRaw.</p></div>
                <div class="feature"><h2>Zero runtime</h2><p>Production runs vendor-free compiled PHP plus ~300 lines of helpers.</p></div>
                <div class="feature"><h2>Measured</h2><p>Every performance claim links to this reproducible benchmark.</p></div>
            </section>
            <footer><small>{$d['note']} · {$d['year']} {$d['brand']}</small></footer>
        </main>
    </div>;
}
PHP;

        $d = self::s1Data();

        return [
            'title' => 'S1 static-heavy page (95% static) — measures folding',
            'source' => $source,
            'lintel_call' => 'Lintel\\Bench\\Gen\\S1Page($bench_data)',
            'competitors' => [
                'echo' => static fn (): string => '<div class="page"><header class="hero"><h1>'.\htmlspecialchars($d['title'], \ENT_QUOTES | \ENT_SUBSTITUTE | \ENT_HTML5, 'UTF-8').'</h1><nav><a href="https://lintel.sh">docs</a><a href="https://lintel.sh/play">playground</a></nav></header><main><section class="intro"><p>Markup is an expression. JSX compiles ahead of time to plain PHP string concat, line numbers are preserved, and every dynamic value is escaped by default.</p></section><section class="features"><div class="feature"><h2>Line preserving</h2><p>Stack traces and PHPStan report the author\'s line numbers, not the compiled file\'s.</p></div><div class="feature"><h2>Secure by default</h2><p>The unsafe escape hatch is one greppable call: Html::unsafeRaw.</p></div><div class="feature"><h2>Zero runtime</h2><p>Production runs vendor-free compiled PHP plus ~300 lines of helpers.</p></div><div class="feature"><h2>Measured</h2><p>Every performance claim links to this reproducible benchmark.</p></div></section><footer><small>'.\htmlspecialchars($d['note'], \ENT_QUOTES | \ENT_SUBSTITUTE | \ENT_HTML5, 'UTF-8').' · '.$d['year'].' '.$d['brand'].'</small></footer></main></div>',
                'blade' => static fn (object $engine): string => $engine->make('s1', ['d' => $d])->render(),
                'twig' => static fn (object $engine): string => $engine->render('s1.html.twig', ['d' => $d]),
                'plates' => static fn (object $engine): string => $engine->render('s1', ['d' => $d]),
            ],
        ];
    }

    private static function s2(string $compiledDir): array
    {
        $source = self::SRC_HEAD.<<<'PHP'
function S2Table(array $rows): Html
{
    $o = '';
    foreach ($rows as $r) {
        $o .= <tr><td>{$r['id']}</td><td>{$r['name']}</td><td>{$r['score']}</td></tr>;
    }
    return <table class="rows"><thead><tr><th>id</th><th>name</th><th>score</th></tr></thead><tbody>{$o}</tbody></table>;
}
PHP;

        $rows = self::s2Data();
        $esc = static fn (mixed $v): string => \htmlspecialchars((string) $v, \ENT_QUOTES | \ENT_SUBSTITUTE | \ENT_HTML5, 'UTF-8');

        return [
            'title' => 'S2 1,000-row table with escaping — measures c()/a() overhead',
            'source' => $source,
            'lintel_call' => 'Lintel\\Bench\\Gen\\S2Table($bench_data)',
            'competitors' => [
                'echo' => static function () use ($rows, $esc): string {
                    $o = '';
                    foreach ($rows as $r) {
                        $o .= '<tr><td>'.$esc($r['id']).'</td><td>'.$esc($r['name']).'</td><td>'.$esc($r['score']).'</td></tr>';
                    }

                    return '<table class="rows"><thead><tr><th>id</th><th>name</th><th>score</th></tr></thead><tbody>'.$o.'</tbody></table>';
                },
                'blade' => static fn (object $engine): string => $engine->make('s2', ['d' => ['rows' => $rows]])->render(),
                'twig' => static fn (object $engine): string => $engine->render('s2.html.twig', ['d' => ['rows' => $rows]]),
                'plates' => static fn (object $engine): string => $engine->render('s2', ['d' => ['rows' => $rows]]),
            ],
        ];
    }

    private static function s3(string $compiledDir): array
    {
        $source = self::SRC_HEAD.<<<'PHP'
function S3Node(int $depth): Html
{
    if ($depth <= 0) {
        return <span class="leaf">leaf</span>;
    }
    return <div class="node">{[<S3Node depth={$depth - 1} />, <S3Node depth={$depth - 1} />, <S3Node depth={$depth - 1} />]}</div>;
}

function S3Tree(array $d): Html
{
    return <section class="tree"><S3Node depth={$d['depth']} /></section>;
}
PHP;

        $depth = self::s3Data()['depth'];

        return [
            'title' => 'S3 component depth 10 × fan-out 3 — measures Html allocation',
            'source' => $source,
            'lintel_call' => 'Lintel\\Bench\\Gen\\S3Tree($bench_data)',
            'competitors' => [
                'echo' => static function () use ($depth): string {
                    $node = static function (int $d) use (&$node): string {
                        if ($d <= 0) {
                            return '<span class="leaf">leaf</span>';
                        }

                        return '<div class="node">'.$node($d - 1).$node($d - 1).$node($d - 1).'</div>';
                    };

                    return '<section class="tree">'.$node($depth).'</section>';
                },
                'blade' => static fn (object $engine): string => $engine->make('s3', ['d' => ['depth' => $depth]])->render(),
                'twig' => static fn (object $engine): string => $engine->render('s3.html.twig', ['d' => ['depth' => $depth]]),
                'plates' => static fn (object $engine): string => $engine->render('s3', ['d' => ['depth' => $depth]]),
            ],
        ];
    }

    private static function s4(string $compiledDir): array
    {
        $source = self::SRC_HEAD.<<<'PHP'
function S4Links(array $links): Html
{
    $o = '';
    foreach ($links as $i => $l) {
        $o .= <a {...['href' => $l['href'], 'class' => ['btn', 'btn--on' => $l['active']], 'data-i' => $i, 'data-kind' => $l['kind']]}>{$l['label']}</a>;
    }
    return <div class="links">{$o}</div>;
}
PHP;

        $links = self::s4Data();
        $esc = static fn (mixed $v): string => \htmlspecialchars((string) $v, \ENT_QUOTES | \ENT_SUBSTITUTE | \ENT_HTML5, 'UTF-8');

        return [
            'title' => 'S4 attribute-heavy (class, href, spread) — measures helper cost',
            'source' => $source,
            'lintel_call' => 'Lintel\\Bench\\Gen\\S4Links($bench_data)',
            'competitors' => [
                'echo' => static function () use ($links, $esc): string {
                    $o = '';
                    foreach ($links as $i => $l) {
                        $cls = 'btn'.($l['active'] ? ' btn--on' : '');
                        $o .= '<a href="'.$esc($l['href']).'" class="'.$esc($cls).'" data-i="'.$esc((string) $i).'" data-kind="'.$esc($l['kind']).'">'.$esc($l['label']).'</a>';
                    }

                    return '<div class="links">'.$o.'</div>';
                },
                'blade' => static fn (object $engine): string => $engine->make('s4', ['d' => ['links' => $links]])->render(),
                'twig' => static fn (object $engine): string => $engine->render('s4.html.twig', ['d' => ['links' => $links]]),
                'plates' => static fn (object $engine): string => $engine->render('s4', ['d' => ['links' => $links]]),
            ],
        ];
    }

    /** S5: the stream target compiles the SAME source; slow boundaries are simulated by the component body's usleep. */
    private static function s5(string $compiledDir): array
    {
        $source = self::SRC_HEAD.<<<'PHP'
function S5Boundary(string $label, int $ms): Html
{
    \usleep($ms * 1000);
    return <div class="boundary">{$label} resolved</div>;
}

function S5Page(array $d): Html
{
    return <main class="s5"><h1>{$d['title']}</h1><S5Boundary label="one" ms={$d['ms']} /><S5Boundary label="two" ms={$d['ms']} /><S5Boundary label="three" ms={$d['ms']} /></main>;
}
PHP;

        // The stream emit lowers JSX to `static function () use (...):
        // \Generator` closures that capture the enclosing locals their body
        // references (the §21.2 emit fix), so this lane reads its data
        // through ordinary function parameters — a DYNAMIC streaming
        // component: every yield flows through the capture at runtime.
        $streamSource = self::SRC_HEAD.<<<'PHP'
function S5BoundaryStream(string $label, int $ms): \Generator
{
    \usleep($ms * 1000);
    return <div class="boundary">{$label} resolved</div>;
}

function S5PageStream(array $d): \Generator
{
    return <main class="s5"><h1>{$d['title']}</h1><S5BoundaryStream label="one" ms={$d['ms']} /><S5BoundaryStream label="two" ms={$d['ms']} /><S5BoundaryStream label="three" ms={$d['ms']} /></main>;
}
PHP;

        $d = self::s5Data();

        return [
            'title' => 'S5 streaming TTFB with 3 slow boundaries (5 ms simulated I/O each)',
            'source' => $source,
            'stream_source' => $streamSource,
            'lintel_call' => 'Lintel\\Bench\\Gen\\S5Page($bench_data)',
            'competitors' => [],
        ];
    }
}
