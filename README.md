<div align="center">
  <img src="https://cdn.jsdelivr.net/gh/elcoosp/lintel@main/docs/logo.png" alt="Lintel logo" width="300"/>
  <p>
    <strong>An ahead-of-time JSX compiler for PHP, written in Rust, that emits plain, line-preserving PHP.</strong><br/>
    Write components as JSX inside ordinary PHP, compile <code>.lintel.php</code> sources to plain PHP, and keep every line number so Xdebug, PHPStan and stack traces point at the code you wrote. One source lowers three ways: string concatenation by default, generators for streaming SSR, or <code>\Markup\Element</code> calls aligned with the native PHP markup RFC.
  </p>
  <p>
    <a href="https://github.com/elcoosp/lintel/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/elcoosp/lintel/ci.yml?style=flat-square&label=CI" alt="CI status"/></a>
    <a href="Cargo.toml"><img src="https://img.shields.io/badge/version-0.1.0-blue?style=flat-square" alt="Version 0.1.0"/></a>
    <a href="Cargo.toml"><img src="https://img.shields.io/badge/license-MIT_OR_Apache--2.0-blue?style=flat-square" alt="License: MIT OR Apache-2.0"/></a>
    <a href="docs/architecture.md"><img src="https://img.shields.io/badge/docs-architecture-blue?style=flat-square" alt="Documentation"/></a>
  </p>
  <p>
    <a href="#quickstart">Quickstart</a>
    &nbsp;&bull;&nbsp;
    <a href="#features">Features</a>
    &nbsp;&bull;&nbsp;
    <a href="#how-it-works">How it works</a>
    &nbsp;&bull;&nbsp;
    <a href="#usage">Usage</a>
    &nbsp;&bull;&nbsp;
    <a href="#status">Status</a>
    &nbsp;&bull;&nbsp;
    <a href="#learn-more">Learn more</a>
  </p>
</div>

## Demo

The `edit → refresh → compiled view` loop, captured from a real terminal by the [`docs/demo.tape`](docs/demo.tape) render:

<p align="center">
  <img src="https://cdn.jsdelivr.net/gh/elcoosp/lintel@main/docs/demo.gif" alt="Lintel demo: edit a .lintel.php source, rebuild, and view the compiled page" width="640"/>
</p>

## Quickstart

Requires Rust 1.80+ (a stable toolchain) to compile; no PHP is needed to build. The crates are not yet published, so install from a clone:

```bash
git clone https://github.com/elcoosp/lintel
cd lintel
cargo install --path crates/lintel-cli
```

Scaffold a runnable demo and compile it in the same command:

```bash
lintel init my-demo
```

```text
wrote my-demo/hello.lintel.php (20 lines of .lintel.php)
compiled my-demo/hello.php — edit the source and run `lintel build my-demo/hello.lintel.php` to rebuild
```

Edit `my-demo/hello.lintel.php`, then rebuild or run the diagnostics-only CI gate:

```bash
lintel build my-demo/hello.lintel.php   # compiled PHP to stdout
lintel check my-demo/                   # exit 1 on errors, warnings advisory
```

## Features

- **Plain PHP out, your line numbers intact.** The output keeps the source's exact line count and non-JSX bytes pass through verbatim, so Xdebug breakpoints, PHPStan results and stack traces stay on the author's lines. `lintel init`'s demo is 20 lines of source and 20 lines of compiled PHP; the [line-preservation](crates/lintel-core/tests/line_preservation.rs) and [pass-through](crates/lintel-core/tests/passthrough.rs) suites enforce the invariant on every target.
- **Type-driven escape elision.** Provably-safe scalars skip the runtime helper: `(int)`/`(float)` casts and `count()` calls fold to a raw operand, and a typed `string` parameter escapes directly through `htmlspecialchars` instead of the generic `c()`. Anything the body can rebind keeps the helper ([contract](docs/escape-elision.md), [elision](crates/lintel-core/tests/elision.rs) and [typed-elision](crates/lintel-core/tests/typed_elision.rs) tests).
- **Incremental builds are I/O-bound.** The content-addressed cache (`SHA-256` over source + target) replays a warm compile in ~1.2 ms against a cold ~4.8 ms for 24 files — roughly 4x faster, with cold/warm throughput of 23k/25k files/s native. `check` and `analyze` never read the cache, and `--no-cache` bypasses it ([cache source](crates/lintel-cli/src/cache.rs), the `build_cache` criterion bench).
- **Three lowering targets from one source.** String concatenation (the default), streaming generators for SSR with `<Suspense>`, or RFC-parity `\Markup\Element` calls — the migration rehearsal for the native PHP markup RFC ([stream](docs/stream-alignment.md) and [rfc](docs/rfc-alignment.md) contracts).
- **Typed component checks before PHP would fatal.** In-file signatures catch unknown props (`LNT0203`), children passed to a component with no `children` parameter (`LNT0204`) and omitted required parameters (`LNT0205`) at compile time, on the invocation's source line.
- **Diagnostics machines can read.** `--format=json` emits JSON Lines with stable `LNTxxxx` codes and 1-based positions; `SourceMap` exposes the same offset-to-line/column math the renderer uses. A `// lintel-ignore-next` comment suppresses validation diagnostics on the following line and `// lintel-ignore-file` silences a whole file; parse errors are never suppressible ([JSON contract](docs/diagnostics-json.md), [ignore directives](docs/ignore-directives.md)).
- **Framework bridges and an editor that knows the language.** The Laravel bridge ships `artisan lintel:build` and a service provider; the Symfony bridge exposes `lintel_render('Badge', {name: $user})` as a Twig function. The LSP keeps a persistent workspace index and parses with the upstream tree-sitter PHP grammar, so rename, references and cross-file `<Component />` resolution work on closed files ([bridges](php/blade), [LSP](crates/lintel-lsp)).

## How it works

`lintel-core` is a pure function — no I/O, no PHP invocation. A PHP-aware scanner copies every non-JSX byte verbatim and recognizes JSX only where an expression can start (after `return`, `echo`, `=`, `=>`, operators — never after an operand, so `$a < $b` stays a comparison). It parses elements, fragments, attributes and children into an AST, a registry of validation rules walks the tree, and a target-specific lowering emits line-preserving PHP.

```mermaid
flowchart LR
  A[".lintel.php"] --> B["scan: PHP-aware, verbatim pass-through"]
  B --> C["parse: JSX at expression start"]
  C --> D["validate: LNT rules + suppressions"]
  D --> E["emit: string | stream | rfc"]
  E --> F["plain .php, line-preserving"]
```

| Crate                                | Role                                                                         |
| ------------------------------------ | ---------------------------------------------------------------------------- |
| [`lintel-core`](crates/lintel-core/) | Scanner, parser, validation pass and line-preserving emitter (pure, no I/O)  |
| [`lintel-diag`](crates/lintel-diag/) | Stable `LNTxxxx` codes, dependency-free text and JSON renderers, `SourceMap` |
| [`lintel-cli`](crates/lintel-cli/)   | The `lintel` binary and an embeddable engine (`lintel_cli::run`)             |
| [`lintel-lsp`](crates/lintel-lsp/)   | The language server with the tree-sitter lane and workspace index            |

The three targets lower the same source differently:

| Target             | Output                                                                             | Contract                                     |
| ------------------ | ---------------------------------------------------------------------------------- | -------------------------------------------- |
| `string` (default) | `new \Lintel\Html(…)` built from merged literals and helper calls                  | escape-by-default helpers                    |
| `stream`           | An immediately-invoked `static function (): \Generator` closure yielding chunks    | [stream-alignment](docs/stream-alignment.md) |
| `rfc`              | `new \Markup\Element(…)` and named-argument component calls, no `\Lintel\` helpers | [rfc-alignment](docs/rfc-alignment.md)       |

## Usage

### A basic component

A component compiled by the default `string` target — the executable golden in [`tests/golden.rs`](crates/lintel-core/tests/golden.rs):

```php
<?php
function Badge(string $label, bool $verified = false): Html {
    return (
        <span class="badge">
            {$label}
            {$verified && <i class="icon-check" />}
        </span>
    );
}
```

compiles to:

```php
<?php
function Badge(string $label, bool $verified = false): Html {
    return (
        new \Lintel\Html('<span class="badge">'
 . \htmlspecialchars($label, \ENT_QUOTES | \ENT_SUBSTITUTE | \ENT_HTML5)
 . \Lintel\c($verified && new \Lintel\Html('<i class="icon-check"></i>'))
 . '</span>')
    );
}
```

Both files are ten lines. Static markup folds into string literals, the `string`-typed `$label` escapes directly without the generic helper, and the nested `<i>` inside the child expression transpiles recursively.

### Streaming with Suspense

Compile a page with the `stream` target so rendering produces a generator that yields chunks as they resolve:

```bash
lintel build --target=stream --out public/dist pages/
```

A `<Suspense>` boundary flushes its placeholder first, then streams the resolved content in place once the slow work finishes. The shape the lowering emits (from the [stream contract](docs/stream-alignment.md)) and the runnable demo ([`scripts/suspense-demo.php`](scripts/suspense-demo.php)):

```php
$page = (static function (): \Generator {
    yield '<!doctype html><html><body><h1>Report</h1>';
    yield new \Lintel\Suspense(
        'Loading the report…',
        static function (): \Generator {
            yield '<ul><li>user: ' . \Lintel\c($payload) . '</li></ul>';
        },
    );
    yield '<p>footer — flushed while the boundary was still pending</p>';
    yield '</body></html>';
})();
echo \Lintel\stream($page);
```

The chunk order is the protocol: the shell and placeholder print first, the footer prints while the boundary is pending, and the resolved content arrives and swaps in. The deployment guides pin the per-server wiring that keeps the stream unbuffered ([FPM](docs/deployment/fpm.md), [RoadRunner](docs/deployment/roadrunner.md), [Swoole](docs/deployment/swoole.md), [FrankenPHP](docs/deployment/frankenphp.md)).

### Framework bridge snippet

Render a Lintel component from a Twig template (the [Symfony bridge](php/twig)):

```twig
{{ lintel_render('Badge', {label: user.name, verified: user.verified}) }}
```

`lintel_render` resolves the component, spreads the props as PHP named arguments, and prints the result through the runtime's `c()`, so a Twig-embedded component is not an escape bypass. The Laravel bridge mirrors this with `artisan lintel:build`, which compiles the app's `.lintel.php` sources into the configured cache dir and writes the `lintel.json` manifest ([bridge source](php/blade)).

### From Rust

```rust
use lintel_core::{compile, Options, Target};

let src = "<?php\nreturn <b class=\"x\">{$name}</b>;\n";
let out = compile(src, &Options::default()).unwrap();      // string target
assert!(out.php.contains("new \\Lintel\\Html("));

let stream = compile(src, &Options { target: Target::Stream }).unwrap();
let rfc = compile(src, &Options { target: Target::Rfc }).unwrap();
```

`compile` returns every error-severity diagnostic instead of panicking; warnings ride along on `out.warnings`. `compile_with` swaps the validation registry, and `scan_with` swaps the lowering — the same seams the CLI uses. Every seam has a runnable example under [`crates/lintel-core/examples/`](crates/lintel-core/examples/) and [`crates/lintel-cli/examples/`](crates/lintel-cli/examples/).

## Status

> [!NOTE]
> **0.1.0 — early but tested.** Supports PHP 8.4+ and Rust 1.80+ on Linux, macOS, and Windows. Zero external users, zero production soak; the `lintel` crate name on crates.io is taken by an unrelated package, so the maintainer is running a community naming process (`scripts/rename-readiness.sh <candidate>` reports the rename surface). Maturity evidence at this tag: 625 tests across the Rust crates (516) and the six PHP packages (109), pedantic Clippy zero-warning, PHPStan level-max clean, cargo-deny clean, actionlint + the docs-site link check + the claims sweep green on every push.

Implemented and tested end to end: the scanner, parser, validation pass, all three lowering targets (`string`/`stream`/`rfc`), escape elision, suppression directives, JSON diagnostics, the `NodeVisitor` and rule-registry seams, the `lintel init/build/check/analyze/migrate/watch` CLI, the content-addressed incremental build cache, the PHP runtime (`\Lintel\Html` and the `c/a/u/cls/sty/srcset/attrs` helpers) with the §7.2 threat corpus, streaming with `<Suspense>` and the four server-adapter transports, the Laravel and Symfony bridges, the persistent LSP workspace index with the tree-sitter lane, the WASM playground, the benchmark harness and the deployment guides. CI runs actionlint, `cargo fmt --check`, pedantic Clippy with `-D warnings`, cargo-deny, the full Rust and PHP suites, the fuzz lane over the seeded corpus and the docs link check on every push. The [implementation status table](docs/architecture.md) reconciles shipped versus planned.

The crates.io and Packagist publishes are pending maintainer action (the registry token and the package-name review gate the release workflow); install from source until then. Questions and bugs: [GitHub issues](https://github.com/elcoosp/lintel/issues).

## Learn more

- [Architecture and roadmap](docs/architecture.md) — the master plan, the language spec and the §21 status table
- [Deployment guides](docs/deployment/) — FPM, RoadRunner, Swoole and FrankenPHP, end to end
- [FAQ](docs/faq.md) — the common questions and the design rationale
- [RFC alignment](docs/rfc-alignment.md) · [Streaming](docs/stream-alignment.md) · [Escape elision](docs/escape-elision.md)
- [Ignore directives](docs/ignore-directives.md) · [JSON diagnostics](docs/diagnostics-json.md) · [Node visitor](docs/node-visitor.md)
