# Benchmarks (`/bench`) — the §9.2 harness

This directory is the task-card T-14 deliverable: a reproducible PHP
rendering benchmark. `make bench` (repo root) reproduces every number in
`bench/results/` — that is the publishing rule (architecture doc §9.2): **no
number appears in README/launch posts unless this harness reproduces it on
the pinned runtime and the methodology page links the raw JSON.**

## Scenarios (doc §9.2)

| ID | Scenario | What it measures |
|---|---|---|
| S1 | static-heavy page (95% static) | literal folding |
| S2 | 1,000-row table with escaping | `c()`/`a()` helper overhead |
| S3 | component tree, depth 10 × fan-out 3 | `Html` allocation |
| S4 | attribute-heavy links (class, href, spread) | helper cost |
| S5 | streaming TTFB with 3 slow boundaries | the `stream` target |

Competitors per §9.2: hand-written `echo` (the floor), Blade (compiled,
illuminate/view), Twig (compiled), Plates, the lintl `--target=rfc`
lowering, and the lintl string target. S5 adds a hand-written
generator-chunk floor and the lintl stream target.

## Settings matrix

`opcache-off`, `opcache-on` (JIT off), `opcache-jit` (tracing). Each setting
runs in its own PHP subprocess (`run.php` re-executes itself as a worker
with `-d` flags), 30 timed renders per point after 3 warm-ups.

## Metrics

- **ns/render**: median, p95, and min over 30 runs (`hrtime(true)`).
- **peak memory**: `memory_reset_peak_usage()` + `memory_get_peak_usage(true)`
  around the timed loop (PHP ≥ 8.2).
- **opcode count**: the harness probes
  `opcache.opt_debug_level=0x20000` over each compiled output. Where the
  runtime's opcache does not expose the dump (a stripped static CLI build
  does not), the metric is recorded as `null` in the JSON and the summary
  says so — no number is invented. The CI bench job re-probes on the pinned
  image.
- **compile throughput**: `lintl build --out` over 100 distinct sources,
  cold (fresh process, cold page cache) and warm (second run), files/s.

## Runtime pinning

- **Local/native mode** (`BENCH_MODE=native`, the default when no Docker is
  available): whatever `php` is on PATH. The committed
  `results/results-native-*.json` record the exact PHP version, binary
  path, OS and machine — treat them as one machine's data point, not a
  claim.
- **Docker-pinned mode**: `bench/Dockerfile` pins `php:8.6-cli` **by
  digest**. `make bench` builds and runs it when a Docker daemon is
  available and writes results tagged `docker`. That is the reproduction
  the publishing rule trusts; the native run is what the sandbox could
  produce.

## The `rfc` competitor's reference shape

The RFC (Native Markup Expressions, under discussion, target 8.7/9) ships
no runtime, so `lib/rfc_reference.php` provides a **measurement-only**
reference implementation of `new \Markup\Element(tag, attrs, children)`
(docs/rfc-alignment.md's lowering shape) with the same escape-by-default
contract. It exists only so the rfc lane is *executed*, not skipped; it is
not part of the compiler, the runtime or any shipped package.

## S5 authoring note (measured limitation)

The v0.1 stream emit lowers JSX to `static function (): \Generator`
closures **without a use-capture** (docs/stream-alignment.md as shipped),
so a streamed component body cannot see its own locals or even its own
function's parameters. The S5 stream lane therefore reads its data through
a superglobal (`lib/Scenarios.php::s5`); the emit fix (emit `use (...)` for
the referenced locals) is recorded in the architecture doc §21.2 backlog —
the compiler core is frozen this session.

## Running

```sh
# native (no Docker):
LINTL_BIN=/path/to/lintl php bench/run.php

# pinned (Docker):
make bench
```

Artifacts land in `bench/results/`: `results-<mode>-<phpver>-<date>.json`
(raw), `summary-<mode>-<phpver>-<date>.md` (tables), `chart-<S>.svg` (one
bar chart per scenario at the `opcache-on` setting).
