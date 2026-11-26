<?php

/**
 * The §9.2 benchmark harness entry (task-card T-14).
 *
 * Orchestrator (`php bench/run.php`): walks the OPcache/JIT settings matrix
 * by re-executing itself as a worker subprocess with `-d` flags, merges the
 * JSON, measures compile throughput with the lintl binary, probes the
 * opcode-count instrumentation, and writes JSON + Markdown + SVG into
 * bench/results/.
 *
 * Worker (`php bench/run.php worker --setting NAME`): compiles every
 * scenario source with the lintl binary (string / rfc / stream targets),
 * warms the template engines, and times 30 renders per
 * scenario × competitor (median + p95) plus peak memory. Prints one JSON
 * line to stdout.
 *
 * Environment: LINTL_BIN (path to the built lintl; default
 * ../target/release/lintl relative to this file), BENCH_RUNS (default 30).
 */

declare(strict_types=1);

require __DIR__.'/vendor/autoload.php';

use Lintl\Bench\Engines;
use Lintl\Bench\Metrics;
use Lintl\Bench\ProgressiveTiming;
use Lintl\Bench\Scenarios;
use Markup\Element;

const RUNS = 30;
const WARMUP = 3;

/** Resolve the lintl binary: LINTL_BIN or the repo's release build. */
function lintl_bin(): ?string
{
    $env = \getenv('LINTL_BIN');
    if (\is_string($env) && $env !== '') {
        return \is_file($env) ? $env : null;
    }
    $candidate = \dirname(__DIR__).'/target/release/lintl';

    return \is_file($candidate) ? $candidate : null;
}

/** Compile one scenario source with the given target; returns the output path. */
function compile_scenario(string $bin, string $outDir, string $id, string $source, string $target): string
{
    \is_dir($outDir) || \mkdir($outDir, 0777, true);
    $src = "$outDir/$id.lintl.php";
    \file_put_contents($src, $source);
    $cmd = [$bin, 'build', "--target=$target", '--out', "$outDir/$target", $src];
    $process = \proc_open($cmd, [1 => ['pipe', 'w'], 2 => ['pipe', 'w']], $pipes);
    if (! \is_resource($process)) {
        throw new RuntimeException("could not run the lintl binary: $bin");
    }
    $out = (string) \stream_get_contents($pipes[1]);
    $err = (string) \stream_get_contents($pipes[2]);
    \fclose($pipes[1]);
    \fclose($pipes[2]);
    $exit = \proc_close($process);
    if ($exit !== 0) {
        throw new RuntimeException("lintl build --target=$target failed for $id (exit $exit): $err");
    }
    $path = "$outDir/$target/$id.php";
    if (! \is_file($path)) {
        throw new RuntimeException("lintl build did not write $path");
    }

    return $path;
}

/**
 * All lowering targets define the SAME scenario functions; a worker process
 * requires string + rfc (+ stream for S5) side by side, so each variant is
 * compiled from a source whose function names carry the target suffix and
 * whose return types match the target's authoring contract (docs/
 * rfc-alignment.md: components return the runtime value; docs/
 * stream-alignment.md: "components that stream declare : \Generator").
 *
 * @param  array<int, string>  $names
 */
function variant_source(string $source, array $names, string $suffix, string $returnType): string
{
    foreach ($names as $name) {
        $source = str_replace($name, $name.$suffix, $source);
    }

    return str_replace(': Html', ': '.$returnType, $source);
}

/** @return array<string, array<int, string>> the function names each scenario source declares */
function scenario_fns(): array
{
    return [
        'S1' => ['S1Page'],
        'S2' => ['S2Table'],
        'S3' => ['S3Tree', 'S3Node'],
        'S4' => ['S4Links'],
        'S5' => ['S5Page', 'S5Boundary'],
    ];
}

/** Time one callable N times (hrtime); returns the ns list. */
/** @return array<int, float> */
function time_runs(callable $render, int $runs, int $warmup): array
{
    for ($i = 0; $i < $warmup; $i++) {
        $render();
    }
    $times = [];
    for ($i = 0; $i < $runs; $i++) {
        $t0 = \hrtime(true);
        $render();
        $times[] = (float) (\hrtime(true) - $t0);
    }

    return $times;
}

/** Run one setting: every scenario × competitor under the current PHP flags. */
function run_worker(string $setting, string $compiledDir): array
{
    $runs = (int) (\getenv('BENCH_RUNS') ?: RUNS);
    $bin = lintl_bin();
    $scenarios = Scenarios::all($compiledDir);
    $data = [
        'S1' => Scenarios::s1Data(),
        'S2' => Scenarios::s2Data(),
        'S3' => Scenarios::s3Data(),
        'S4' => Scenarios::s4Data(),
        'S5' => Scenarios::s5Data(),
    ];
    $views = "$compiledDir/views-".\preg_replace('/[^a-z0-9-]/i', '-', $setting);
    \is_dir($views) || \mkdir($views, 0777, true);
    $engines = [
        'blade' => Engines::blade($views, "$views/blade-cache"),
        'twig' => Engines::twig($views, "$views/twig-cache"),
        'plates' => Engines::plates($views),
    ];
    foreach (['S1', 'S2', 'S3', 'S4'] as $id) {
        Engines::writeTemplates($id, $views);
    }

    $results = [];
    $notes = [];
    foreach ($scenarios as $id => $scenario) {
        $results[$id] = [];

        // Lintl string target, plus rfc (the same source, the RFC lowering)
        // and — for S5 — the stream target, compiled from suffixed variants
        // so every lowering can be required side by side in one process.
        if ($bin !== null) {
            $stringPath = compile_scenario($bin, $compiledDir, $id, $scenario['source'], 'string');
            $fnName = 'Lintl\\Bench\\Gen\\'.scenario_fns()[$id][0];
            require $stringPath;
            $results[$id]['lintl'] = bench_one(static fn (): string => (string) $fnName($data[$id]), $runs);

            if ($id !== 'S5') {
                // S5's lanes are the streaming ones (§9.2): a string render
                // of the rfc tree carries no TTFB semantics there.
                if (! \class_exists(Element::class, false)) {
                    require __DIR__.'/lib/rfc_reference.php';
                }
                $rfcPath = compile_scenario($bin, $compiledDir, $id, variant_source($scenario['source'], scenario_fns()[$id], 'Rfc', '\\Markup\\Element'), 'rfc');
                $rfcFnName = 'Lintl\\Bench\\Gen\\'.scenario_fns()[$id][0].'Rfc';
                require $rfcPath;
                $results[$id]['rfc'] = bench_one(static fn (): string => (string) $rfcFnName($data[$id]), $runs);
            }
        } else {
            $notes[] = 'no lintl binary: lintl/rfc lanes skipped';
        }

        // S5: streaming lanes (stream target + the hand-written floor).
        if ($id === 'S5') {
            // Sync lane: the whole page blocks before the first byte, so
            // its TTFB is the full render time.
            $sync = $results[$id]['lintl'];
            $results[$id]['lintl-sync'] = [
                'ttfb_median_ns' => $sync['median_ns'],
                'ttfb_p95_ns' => $sync['p95_ns'],
                'total_median_ns' => $sync['median_ns'],
                'total_p95_ns' => $sync['p95_ns'],
                'peak_bytes' => $sync['peak_bytes'],
            ];
            unset($results[$id]['lintl']);
            if ($bin !== null) {
                // stream_source is authored streaming-safe: the emitted
                // generator closures capture their locals (the §21.2 emit
                // fix — see lib/Scenarios.php), so the lane reads its data
                // through an ordinary parameter.
                $streamPath = compile_scenario($bin, $compiledDir, $id, $scenario['stream_source'], 'stream');
                require $streamPath;
                $streamFn = 'Lintl\\Bench\\Gen\\S5PageStream';
                $results[$id]['lintl-stream'] = bench_stream(
                    static fn (): iterable => $streamFn($data['S5']),
                    $runs,
                );
            }
            $results[$id]['echo-stream'] = bench_stream(static function () use ($data): Generator {
                yield '<main class="s5"><h1>'.\htmlspecialchars($data['S5']['title'], \ENT_QUOTES | \ENT_SUBSTITUTE | \ENT_HTML5, 'UTF-8').'</h1>';
                foreach (['one', 'two', 'three'] as $label) {
                    \usleep($data['S5']['ms'] * 1000);
                    yield '<div class="boundary">'.$label.' resolved</div>';
                }
                yield '</main>';
            }, $runs);
        }

        // Template engines (S1–S4) and the echo floor.
        foreach ($scenario['competitors'] as $name => $render) {
            if ($name === 'echo') {
                $results[$id]['echo'] = bench_one($render, $runs);

                continue;
            }
            $results[$id][$name] = bench_one(static fn (): string => $render($engines[$name]), $runs);
        }
    }

    return ['setting' => $setting, 'runs' => $runs, 'warmup' => WARMUP, 'results' => $results, 'notes' => $notes];
}

/** Time one string-rendering callable: median/p95 over RUNS + peak memory. */
function bench_one(callable $render, int $runs): array
{
    \gc_collect_cycles();
    \memory_reset_peak_usage();
    $times = time_runs($render, $runs, WARMUP);
    $render(); // one more so peak reflects a fresh render's footprint
    $peak = \memory_get_peak_usage(true);

    return [
        'median_ns' => Metrics::median($times),
        'p95_ns' => Metrics::percentile($times, 95),
        'min_ns' => \min($times),
        'peak_bytes' => $peak,
    ];
}

/** Time a chunk-streaming callable: TTFB + total over RUNS. */
function bench_stream(callable $render, int $runs): array
{
    $ttfbs = [];
    $totals = [];
    for ($i = 0; $i < WARMUP; $i++) {
        foreach ($render() as $chunk) {
        }
    }
    for ($i = 0; $i < $runs; $i++) {
        $t0 = \hrtime(true);
        $first = null;
        foreach ($render() as $chunk) {
            if ($first === null) {
                $first = (float) (\hrtime(true) - $t0);
            }
        }
        $total = (float) (\hrtime(true) - $t0);
        $ttfbs[] = $first ?? $total;
        $totals[] = $total;
    }

    return [
        'ttfb_median_ns' => Metrics::median($ttfbs),
        'ttfb_p95_ns' => Metrics::percentile($ttfbs, 95),
        'total_median_ns' => Metrics::median($totals),
        'total_p95_ns' => Metrics::percentile($totals, 95),
    ];
}

/** Compile throughput: files/s for a cold and a warm build of 100 sources. */
function compile_throughput(string $bin, string $workDir): array
{
    $count = 100;
    \is_dir($workDir) || \mkdir($workDir, 0777, true);
    $base = Scenarios::all($workDir)['S2']['source'];
    $files = [];
    for ($i = 0; $i < $count; $i++) {
        $path = \sprintf('%s/s2-%03d.lintl.php', $workDir, $i);
        // A unique constant keeps every file's content distinct.
        \file_put_contents($path, \str_replace('1,000-row', "row-set $i", $base));
        $files[] = $path;
    }
    $measure = static function () use ($bin, $files, $workDir, $count): float {
        $t0 = \hrtime(true);
        $process = \proc_open([$bin, 'build', '--out', "$workDir/out", ...$files], [1 => ['pipe', 'w'], 2 => ['pipe', 'w']], $pipes);
        if (! \is_resource($process)) {
            throw new RuntimeException('compile-throughput: could not run the lintl binary');
        }
        \stream_get_contents($pipes[1]);
        $err = (string) \stream_get_contents($pipes[2]);
        \fclose($pipes[1]);
        \fclose($pipes[2]);
        $exit = \proc_close($process);
        $elapsed = \hrtime(true) - $t0;
        if ($exit !== 0) {
            throw new RuntimeException("compile-throughput build failed (exit $exit): $err");
        }

        return $elapsed > 0 ? $count / ($elapsed / 1e9) : 0.0;
    };
    $cold = $measure();
    $warm = $measure();

    return ['files' => $count, 'cold_files_per_s' => $cold, 'warm_files_per_s' => $warm];
}

/**
 * Opcode-count probe (§9.2): opcache.opt_debug_level prints the optimized
 * opcode sequence when the runtime's opcache supports it. Where it does not
 * (e.g. a stripped static CLI build), the metric is recorded as null and
 * the methodology says so — no number is invented.
 */
function opcode_counts(array $compiled): array
{
    $out = [];
    foreach ($compiled as $id => $path) {
        $process = \proc_open(
            [PHP_BINARY, '-d', 'opcache.enable_cli=1', '-d', 'opcache.opt_debug_level=0x20000', '-r', "require '$path';"],
            [1 => ['pipe', 'w'], 2 => ['pipe', 'w']],
            $pipes,
        );
        if (! \is_resource($process)) {
            $out[$id] = null;

            continue;
        }
        $dump = (string) \stream_get_contents($pipes[1]).(string) \stream_get_contents($pipes[2]);
        \fclose($pipes[1]);
        \fclose($pipes[2]);
        \proc_close($process);
        $count = \preg_match_all('/^#|\bZEND_[A-Z_]+\b/m', $dump);
        $out[$id] = $count > 0 ? $count : null;
    }

    return $out;
}

// ---- dispatch --------------------------------------------------------------

$argv0 = $_SERVER['argv'] ?? [];
$mode = $argv0[1] ?? 'orchestrator';

if ($mode === 'worker') {
    $setting = (string) (\getenv('SETTING') ?: 'opcache-on');
    $workRoot = \sys_get_temp_dir().'/lintl-bench-'.\getmypid();
    $compiled = "$workRoot/compiled";
    $worker = run_worker($setting, $compiled);
    // S5 note: the compiled S5 stream lane lives with the worker; drop scratch.
    \fwrite(STDOUT, \json_encode($worker, \JSON_THROW_ON_ERROR)."\n");

    exit(0);
}

// ---- orchestrator ----------------------------------------------------------

$bin = lintl_bin();
if ($bin === null) {
    \fwrite(STDERR, "bench/run.php: no lintl binary (set LINTL_BIN or build target/release/lintl)\n");
    exit(1);
}
$settings = [
    'opcache-off' => ['-d', 'opcache.enable_cli=0'],
    'opcache-on' => ['-d', 'opcache.enable_cli=1', '-d', 'opcache.jit=off'],
    'opcache-jit' => ['-d', 'opcache.enable_cli=1', '-d', 'opcache.jit=tracing', '-d', 'opcache.jit_buffer_size=32M'],
];
$modeName = \getenv('BENCH_MODE') ?: 'native';
$resultsDir = __DIR__.'/results';
\is_dir($resultsDir) || \mkdir($resultsDir, 0777, true);

$all = [];
$settingsUsed = [];
foreach ($settings as $name => $flags) {
    \putenv("SETTING=$name");
    $cmd = [PHP_BINARY, ...$flags, __FILE__, 'worker'];
    $process = \proc_open($cmd, [1 => ['pipe', 'w'], 2 => ['pipe', 'w']], $pipes);
    if (! \is_resource($process)) {
        throw new RuntimeException("could not start worker for $name");
    }
    $json = (string) \stream_get_contents($pipes[1]);
    $err = (string) \stream_get_contents($pipes[2]);
    \fclose($pipes[1]);
    \fclose($pipes[2]);
    $exit = \proc_close($process);
    if ($exit !== 0) {
        \fwrite(STDERR, "worker $name failed (exit $exit):\n$err\n");
        exit(1);
    }
    $all[$name] = \json_decode($json, true, 512, \JSON_THROW_ON_ERROR);
    $settingsUsed[] = $name;
}

// Compile throughput + opcode probe run outside the settings matrix.
$throughput = compile_throughput($bin, \sys_get_temp_dir().'/lintl-bench-throughput');
$compiledForProbe = [];
foreach (['S1', 'S2', 'S3', 'S4', 'S5'] as $id) {
    $probeDir = \sys_get_temp_dir().'/lintl-bench-probe';
    $compiledForProbe[$id] = compile_scenario($bin, $probeDir, $id, Scenarios::all($probeDir)[$id]['source'], 'string');
}
$opcodes = opcode_counts($compiledForProbe);

// The RoadRunner progressive per-boundary timing (Phase 11 M2): the real
// RR transport queues the flush-ordered chunks; the lane records the
// flush-time marks and asserts the progressive invariant (placeholder N
// before resolved N-1) INSIDE the harness — a broken flusher fails the
// bench run instead of producing numbers.
$progressive = ProgressiveTiming::run();
\file_put_contents("$resultsDir/progressive-timing.json", \json_encode($progressive, \JSON_PRETTY_PRINT | \JSON_THROW_ON_ERROR)."\n");

$version = \PHP_VERSION;
$date = \date('Y-m-d');
$provenance = [
    'date' => $date,
    'mode' => $modeName,
    'php' => $version,
    'php_binary' => PHP_BINARY,
    'os' => \php_uname('s').' '.\php_uname('r').' '.\php_uname('m'),
    'hostname' => \php_uname('n'),
    'lintl_bin' => $bin,
    'lintl_version' => \trim((string) \shell_exec(\escapeshellarg($bin).' --version 2>&1')),
    'settings' => $settingsUsed,
    'runs' => RUNS,
    'warmup' => WARMUP,
    's5_slow_ms' => Scenarios::s5Data()['ms'],
];

$slug = \preg_replace('/[^a-z0-9.]+/i', '-', "$modeName-$version-$date");
$jsonPath = "$resultsDir/results-$slug.json";
\file_put_contents($jsonPath, \json_encode([
    'provenance' => $provenance,
    'compile_throughput' => $throughput,
    'opcode_counts' => $opcodes,
    'settings' => $all,
], \JSON_PRETTY_PRINT | \JSON_THROW_ON_ERROR)."\n");

// ---- markdown + svg artifacts ---------------------------------------------

$md = "# Bench results ($modeName) — PHP $version, $date\n\n";
$md .= 'Hardware/OS: `'.$provenance['os'].'`, host `'.$provenance['hostname'].'`; lintl `'.$provenance['lintl_version'].'`; 30 runs per point, 3 warm-up renders; S5 boundaries sleep '.Scenarios::s5Data()['ms'].' ms each. Raw JSON: `'.\basename($jsonPath)."`.\n\n";
$md .= "> Publishing rule (§9.2): no number appears in README/launch posts unless `make bench` reproduces it and the methodology page links this raw JSON.\n\n";
$titles = [
    'S1' => 'S1 static-heavy page — ns/render',
    'S2' => 'S2 1,000-row table — ns/render',
    'S3' => 'S3 component tree (depth 10 × fan-out 3) — ns/render',
    'S4' => 'S4 attribute-heavy links — ns/render',
];
foreach ($titles as $id => $title) {
    $mediansBySetting = [];
    foreach ($all as $setting => $payload) {
        $mediansBySetting[$setting] = \array_map(static fn (array $r) => $r['median_ns'], $payload['results'][$id]);
    }
    $md .= Metrics::summaryTable($titles[$id], $mediansBySetting, 'ns/render');
    $chart = Metrics::barSvg($title.' ('.$settingsUsed[1].')', $mediansBySetting[$settingsUsed[1]] ?? [], 'ns/render');
    \file_put_contents("$resultsDir/chart-$id.svg", $chart);
}

// S5: TTFB + total.
$s5bySettingTtfb = [];
$s5bySettingTotal = [];
foreach ($all as $setting => $payload) {
    $lanes = $payload['results']['S5'];
    $s5bySettingTtfb[$setting] = \array_map(static fn (array $r) => $r['ttfb_median_ns'], $lanes);
    $s5bySettingTotal[$setting] = \array_map(static fn (array $r) => $r['total_median_ns'], $lanes);
}
$md .= Metrics::summaryTable('S5 streaming TTFB (time to first chunk)', $s5bySettingTtfb, 'TTFB');
$md .= Metrics::summaryTable('S5 total render time', $s5bySettingTotal, 'total');
\file_put_contents("$resultsDir/chart-S5.svg", Metrics::barSvg('S5 TTFB ('.$settingsUsed[1].')', $s5bySettingTtfb[$settingsUsed[1]] ?? [], 'TTFB'));

$md .= "### Compile throughput (lintl build, 100 sources)\n\n| metric | value |\n| --- | --- |\n";
$md .= '| cold files/s | '.\sprintf('%.0f', $throughput['cold_files_per_s'])." |\n";
$md .= '| warm files/s | '.\sprintf('%.0f', $throughput['warm_files_per_s'])." |\n\n";

$md .= "### Opcode counts (opcache.opt_debug_level=0x20000)\n\n";
$anyOpcode = false;
foreach ($opcodes as $id => $count) {
    $md .= "- $id: ".($count === null ? 'not measurable on this runtime (instrumentation unavailable) — see bench/README.md' : $count.' opcodes')."\n";
    $anyOpcode = $anyOpcode || $count !== null;
}
if (! $anyOpcode) {
    $md .= "\nThe runtime in use does not expose the opcode dump; the metric stays null here and is attempted again by the CI bench job on the pinned image.\n";
}
$md .= "### S6 RoadRunner progressive per-boundary timing (the Phase 11 lane)\n\n";
$md .= "| metric | value |\n| --- | --- |\n";
$md .= '| boundaries | '.$progressive['boundaries']." |\n";
$md .= '| TTFB — first placeholder | '.\sprintf('%.1f', $progressive['ttfb_placeholder_ns'] / 1000)." µs |\n";
$md .= '| TTFB — first resolved chunk | '.\sprintf('%.1f', $progressive['ttfb_resolved_ns'] / 1e6)." ms |\n";
$md .= '| total — fully resolved page | '.\sprintf('%.1f', $progressive['total_ns'] / 1e6)." ms |\n";
$md .= '| chunks (wire = flush order) | '.$progressive['chunks']." |\n";
$md .= '\nProgressive invariant: placeholder N flushed before resolved N-1 — asserted by the lane itself ('
    .\json_encode($progressive['invariants']).'); raw facts: `progressive-timing.json`.\n\n';

foreach ($all as $setting => $payload) {
    foreach ($payload['notes'] as $note) {
        $md .= "- [$setting] $note\n";
    }
}

$mdPath = "$resultsDir/summary-$slug.md";
\file_put_contents($mdPath, $md);

\fwrite(STDOUT, 'bench: wrote '.\basename($jsonPath).', '.\basename($mdPath)." and chart SVGs to bench/results/\n");
