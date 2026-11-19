# Lintel

**Markup is an expression.** A Rust-powered, ahead-of-time JSX compiler for
PHP: `.lintel.php` sources scan, parse and lower to plain, vendor-free PHP
that runs under OPcache with the author's line numbers preserved.

> This repository is the compiler core. The full product plan, language spec
> and roadmap live in [`docs/architecture.md`](docs/architecture.md); the
> shipped-versus-planned reconciliation lives in its §21.

## What works today (v0.1/0.2 core)

- **PHP-aware scanner** — strings, heredocs/nowdocs, comments, `#[…]
  attributes`, inline-HTML interleave and `?>` transitions pass through
  byte-identical; JSX is recognized only in expression-start position.
- **JSX parser** — elements, fragments, attribute forms (static, dynamic,
  boolean, spread), raw-text `<script>`/`<style>` bodies.
- **Three lowering targets** ([`Target`](crates/lintel-core/src/lib.rs)):
  - `String` (default) — `new \Lintel\Html(...)` string-concat AOT with the
    context-aware `\Lintel\c/a/u/cls/sty/attrs` runtime helpers.
  - `Stream` — streaming SSR: the root becomes an immediately-invoked
    `static function (): \Generator` closure yielding markup one chunk at a
    time; see [`docs/stream-alignment.md`](docs/stream-alignment.md).
  - `Rfc` — parity lowering to `new \Markup\Element(...)` constructor calls
    and named-argument component calls; see
    [`docs/rfc-alignment.md`](docs/rfc-alignment.md).
- **Compile-time validation** with stable `LNTxxxx` diagnostics:
  builtin-shadow (`LNT0201`), invalid component prop (`LNT0202`), signature
  checks for in-file components — unknown props (`LNT0203`), children
  without a `children` parameter (`LNT0204`), parameters declared without a
  default that the invocation omits (`LNT0205`) — dynamic event handlers
  (`LNT0301`), static handler warning (`LNT0302`), void children
  (`LNT0401`). Errors fail the compile; warnings ride along on
  [`Output::warnings`].
- **`lintel-ignore-next` suppression** — a `// lintel-ignore-next` line
  comment drops the validation diagnostics anchored on the following line;
  parse errors are never suppressible. See
  [`docs/ignore-directives.md`](docs/ignore-directives.md).

## Quick start

```sh
cargo test --workspace --all-features          # test suite
cargo test --workspace --benches               # bench smoke
cargo bench -p lintel-core                     # criterion throughput benches
cargo run -p lintel-core --example compile_demo    # compile a component
cargo run -p lintel-core --example stream_target   # generator streaming target
cargo run -p lintel-core --example rfc_target      # both lowering targets
cargo run -p lintel-core --example security_lint   # rendered diagnostics
cargo run -p lintel-core --example warnings_lint   # warning diagnostics
cargo run -p lintel-core --example signature_lint  # signature validation
cargo run -p lintel-core --example suppressions_lint  # lintel-ignore-next directives
cargo run -p lintel-core --example passthrough     # pass-through + line preservation
```

```rust
use lintel_core::{compile, Options, Target};

let src = "<?php\nreturn <b class=\"x\">{$name}</b>;\n";
let out = compile(src, &Options::default()).unwrap();        // string target
let rfc = compile(src, &Options { target: Target::Rfc }).unwrap();
```

## Guarantees

1. **Verbatim pass-through** — PHP without JSX is emitted byte-for-byte
   identical (asserted across both targets).
2. **Line preservation** — output has exactly as many lines as the source,
   so Xdebug/PHPStan/Ignition report the author's line numbers.
3. **Escape-by-default** — only provably literal text folds into the
   output; dynamic values go through the runtime helpers (string target).

## Layout

```
crates/lintel-core   scan → parse → validate → emit (pure, no I/O)
crates/lintel-diag   stable LNT codes, miette-style renderer, severities
docs/architecture.md master plan + §21 implementation status
docs/rfc-alignment.md  exact `rfc` lowering contract
docs/stream-alignment.md  exact `stream` lowering contract
docs/ignore-directives.md  exact `lintel-ignore-next` contract
```

## Status

CLI, WASM playground, LSP, PHP runtime and the benchmark harness are future
milestones (see `docs/architecture.md` §15 task cards and §21 status table).
The clippy gate is `cargo clippy --workspace --all-targets --all-features --
-D warnings -W clippy::pedantic` with zero warnings.
