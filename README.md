<div align="center">
  <img src="https://cdn.jsdelivr.net/gh/elcoosp/lintel@main/docs/logo.png" alt="Lintel logo" width="300"/>
  <p>
    <strong>Markup is an expression: an ahead-of-time JSX compiler for PHP, written in Rust.</strong><br/>
    Write components as JSX inside ordinary PHP and compile <code>.lintel.php</code> sources to plain PHP:
    string concatenation by default, generators for streaming SSR, or <code>\Markup\Element</code> calls aligned
    with the native PHP markup RFC. Non-JSX bytes pass through verbatim and every line keeps its number, so
    Xdebug, PHPStan and stack traces point at the code you wrote. The v1.0 feature set ships today:
    the compiler, the PHP runtime helpers, streaming SSR with Suspense, the watch daemon, the LSP with
    the tree-sitter lane, the WASM playground and the production deployment guides.
  </p>
  <p>
    <a href="https://github.com/elcoosp/lintel/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/elcoosp/lintel/ci.yml?style=flat-square&label=CI" alt="CI status"/></a>
    <a href="Cargo.toml"><img src="https://img.shields.io/badge/rustc-1.80%2B-orange?style=flat-square" alt="Minimum supported Rust version: 1.80"/></a>
    <a href="Cargo.toml"><img src="https://img.shields.io/badge/license-MIT_OR_Apache--2.0-blue?style=flat-square" alt="License: MIT OR Apache-2.0"/></a>
    <a href="docs/architecture.md"><img src="https://img.shields.io/badge/docs-architecture-blue?style=flat-square" alt="Documentation"/></a>
  </p>
  <p>
    <a href="#quickstart">Quickstart</a> •
    <a href="#features">Features</a> •
    <a href="#usage">Usage</a> •
    <a href="#status">Status</a> •
    <a href="docs/architecture.md">Docs</a>
  </p>
</div>

## Demo

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

Both files are ten lines: every token after the JSX keeps its line number. Static markup folds into string literals, the `string`-typed `$label` escapes directly without the generic helper, and the nested `<i>` inside the child expression is transpiled recursively.

## Quickstart

Requires Rust 1.80+; no PHP is needed to compile. The crates are not published yet, so install from a clone:

```bash
git clone https://github.com/elcoosp/lintel
cd lintel
cargo install --path crates/lintel-cli
```

Scaffold a demo page and compile it in the same command:

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

- **Plain PHP out, your line numbers intact.** The output keeps the source's exact line count and non-JSX bytes are byte-identical, so Xdebug breakpoints, PHPStan results and stack traces stay on the author's lines ([line-preservation](crates/lintel-core/tests/line_preservation.rs) and [pass-through](crates/lintel-core/tests/passthrough.rs) tests).
- **Escaped by default, unsafe markup rejected at compile time.** Dynamic children and attributes route through context-aware helpers (`c`, `a`, `u`, `srcset`, `cls`, `sty`, `attrs`); a dynamic `on*` handler is a hard error (`LNT0301`), a static one a warning (`LNT0302`).
- **Typed component checks before PHP would fatal.** In-file signatures catch unknown props (`LNT0203`), children passed to a component with no `children` parameter (`LNT0204`) and omitted required parameters (`LNT0205`).
- **Three lowering targets from one source.** String concatenation, streaming generators, or RFC-parity `\Markup\Element` calls — the last is the migration rehearsal for the native PHP markup RFC.
- **Provably-safe escape elision.** `(int)`/`(float)` casts, `count()` calls and non-nullable `string` parameters fold without the generic helper; anything the body can rebind keeps it ([contract](docs/escape-elision.md)).
- **Diagnostics machines can read.** `--format=json` emits JSON Lines with stable codes and 1-based positions; `SourceMap` exposes the same offset-to-line/column math the renderer uses.
- **Extensible seams.** Inject custom `ValidationRule` lints, wrap the `Lowering` emit, or walk the AST with `NodeVisitor` — each has a runnable [example](crates/lintel-core/examples/).

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

The three targets lower the same source differently:

| Target             | Output                                                                             | Contract                                     |
| ------------------ | ---------------------------------------------------------------------------------- | -------------------------------------------- |
| `string` (default) | `new \Lintel\Html(…)` built from merged literals and helper calls                  | escape-by-default helpers                    |
| `stream`           | An immediately-invoked `static function (): \Generator` closure yielding chunks    | [stream-alignment](docs/stream-alignment.md) |
| `rfc`              | `new \Markup\Element(…)` and named-argument component calls, no `\Lintel\` helpers | [rfc-alignment](docs/rfc-alignment.md)       |

## Usage

### CLI

| Command                 | What it does                                                                                     |
| ----------------------- | ------------------------------------------------------------------------------------------------ |
| `lintel init [DIR]`     | Scaffold `hello.lintel.php` and compile it to `hello.php`; refuses to clobber an existing source |
| `lintel build <paths>…` | Compile to stdout, `--out DIR`, or `--in-place` (`Page.lintel.php` → `Page.php`)                 |
| `lintel check <paths>…` | Diagnostics-only CI gate; nothing is written                                                     |

Paths are files or directories (walked recursively in sorted order). Flags: `--target=string|stream|rfc` and `--format=text|json` on both `build` and `check`. Exit codes: `0` clean (warnings advisory), `1` at least one file failed, `2` usage error. Run `lintel --help` for the full usage text.

### From Rust

```rust
use lintel_core::{compile, Options, Target};

let src = "<?php\nreturn <b class=\"x\">{$name}</b>;\n";
let out = compile(src, &Options::default()).unwrap();      // string target
assert!(out.php.contains("new \\Lintel\\Html("));

let stream = compile(src, &Options { target: Target::Stream }).unwrap();
let rfc = compile(src, &Options { target: Target::Rfc }).unwrap();
```

`compile` returns every error-severity diagnostic instead of panicking; warnings ride along on `out.warnings`. `compile_with` swaps the validation registry, and `scan_with` swaps the lowering — the same seams the CLI uses.

### Diagnostics

A dynamic event handler fails the compile with a stable code, a caret snippet and an actionable help line:

```text
error[LNT0301]: dynamic event-handler attribute `onclick` is not allowed
  ┌─ input:2:16
  │
2 │ return <button onclick={$handler}>go</button>;
  ·                ^
  = help: use `data-action={...}` (or an `hx-*` / `x-on:*` binding) and attach behaviour in JavaScript
  = docs: https://lintel.sh/e/LNT0301
```

`lintel check --format=json` emits the same diagnostic as one JSON object per line on stderr (exit codes unchanged):

```json
{
	"code": "LNT0301",
	"severity": "error",
	"message": "dynamic event-handler attribute `onclick` is not allowed",
	"offset": 21,
	"line": 2,
	"col": 16,
	"help": "use `data-action={...}` (or an `hx-*` / `x-on:*` binding) and attach behaviour in JavaScript",
	"docs": "https://lintel.sh/e/LNT0301"
}
```

Thirteen stable `LNTxxxx` codes cover parse errors, component/attribute validation, security and content-model checks. A `// lintel-ignore-next` comment suppresses validation diagnostics on the following line and `// lintel-ignore-file` silences a whole file; parse errors are never suppressible ([contract](docs/ignore-directives.md)).

### Going further

Every seam has a runnable example under [`crates/lintel-core/examples/`](crates/lintel-core/examples/) and [`crates/lintel-cli/examples/`](crates/lintel-cli/examples/): custom rules, custom lowering, the AST visitor, `SourceMap`, escape elision, suppressions, the streaming and RFC targets, and the CLI engine as a library. Run one with `cargo run -p lintel-core --example <name>`.

## Status

> [!NOTE]
> Lintel is at v1.0 readiness. Implemented and tested end to end: the scanner, parser, validation pass, all three lowering targets (`string`/`stream`/`rfc`), escape elision, suppression directives, JSON diagnostics, the `NodeVisitor` and rule-registry seams, the `lintel init/build/check/analyze/stats/rules/migrate/watch` CLI, the content-addressed incremental build cache, the PHP runtime (`\Lintel\Html` and the `c/a/u/cls/sty/srcset/attrs` helpers) with the §7.2 threat corpus, streaming with `<Suspense>` and the four server-adapter transports (RoadRunner end-to-end in CI), the Laravel and Symfony bridges, the persistent LSP workspace index with the tree-sitter lane, the WASM playground, the benchmark harness and the deployment guides. CI runs actionlint, `cargo fmt --check`, pedantic Clippy with `-D warnings`, cargo-deny, the full Rust and PHP suites, the fuzz lane over the seeded corpus and the docs link check on every push. The [implementation status table](docs/architecture.md) (§21) reconciles shipped versus planned.

Questions and bugs: [GitHub issues](https://github.com/elcoosp/lintel/issues).

## Learn more

- [Architecture and roadmap](docs/architecture.md) — master plan, language spec and §21 status table
- [RFC alignment](docs/rfc-alignment.md) · [Streaming](docs/stream-alignment.md) · [srcset routing](docs/srcset-alignment.md)
- [Escape elision](docs/escape-elision.md) · [Ignore directives](docs/ignore-directives.md)
- [JSON diagnostics](docs/diagnostics-json.md) · [Node visitor](docs/node-visitor.md)
