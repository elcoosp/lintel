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
    context-aware `\Lintel\c/a/u/cls/sty/srcset/attrs` runtime helpers.
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
  [`Output::warnings`]. A mismatched close tag that near-misses the open
  tag in scope (`</sapn>` for `</span>`) gets a "did you mean" help line.
- **`ValidationRule` seam** — the validation pass is a registry of rule
  strategies ([`ValidationRule`](crates/lintel-core/src/validate.rs)):
  `compile_with` runs consumer rules alongside (or instead of) the built-in
  set, flowing through the same severity split and `lintel-ignore-next`
  choke point as the built-in lints.
- **`SourceMap` positions** — every diagnostic's byte offset maps to a
  1-based line/column through the public, renderer-parity
  [`SourceMap`](crates/lintel-diag/src/lib.rs) — the offset→position
  converter CLI/LSP consumers previously had to re-implement.
- **`lintel-ignore-next` suppression** — a `// lintel-ignore-next` line
  comment drops the validation diagnostics anchored on the following line,
  and the file-level `// lintel-ignore-file` drops every validation
  diagnostic in the file (the rule walk short-circuits); parse errors are
  never suppressible. See
  [`docs/ignore-directives.md`](docs/ignore-directives.md).
- **Escape elision** — dynamic children that are provably scalar at compile
  time (`(int)`/`(integer)`/`(float)`/`(double)` casts spanning the whole
  expression, `count()` calls) fold to the raw PHP expression instead of
  the `\Lintel\c()` helper, on both markup targets; everything else keeps
  the helper, conservative by design (doc §7.3 "honest version"). See
  [`docs/escape-elision.md`](docs/escape-elision.md).
- **`lintel` CLI (T-10 subset)** — `lintel build` compiles files/directories
  to stdout, `--out DIR` or `--in-place`; `lintel check` is the
  diagnostics-only CI gate (exit 1 on errors, warnings advisory). Sources
  walk in path order; diagnostics render through the same
  [`Diagnostic::render`](crates/lintel-diag/src/lib.rs) snippets.

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
cargo run -p lintel-core --example ignore_file       # lintel-ignore-file directive
cargo run -p lintel-core --example srcset_attr       # srcset per-URL attribute routing
cargo run -p lintel-core --example elision          # escape elision of scalar children
cargo run -p lintel-core --example parse_errors     # parse diagnostics + did-you-mean
cargo run -p lintel-core --example custom_rule      # ValidationRule seam: custom lints
cargo run -p lintel-core --example custom_lowering  # Lowering seam: wrap the emit
cargo run -p lintel-core --example sourcemap        # SourceMap offset → line/col
cargo run -p lintel-core --example passthrough     # pass-through + line preservation
cargo run -p lintel-cli --example build_library    # the CLI engine as a library
cargo install --path crates/lintel-cli             # install the `lintel` binary
lintel build page.lintel.php                       # compile to stdout
lintel check src/                                  # diagnostics-only CI gate
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
crates/lintel-cli    the `lintel` binary: build/check over the core (T-10 subset)
docs/architecture.md master plan + §21 implementation status
docs/rfc-alignment.md  exact `rfc` lowering contract
docs/stream-alignment.md  exact `stream` lowering contract
docs/srcset-alignment.md  exact `srcset` attribute routing contract
docs/escape-elision.md  exact provably-scalar child folding contract
docs/ignore-directives.md  exact suppression directive contracts
```

## Status

The compiler core, the three lowering targets, the validation pass, the
`lintel-ignore-*` directives, escape elision and the minimal `lintel`
build/check CLI run today. The remaining CLI surface (watch/init/daemon),
WASM playground, LSP, PHP runtime and the PHP rendering benchmark harness
(§9.2) are future milestones (see `docs/architecture.md` §15 task cards and
§21 status table) — the criterion compiler-throughput benches in
`crates/lintel-core/benches/` already run today. The clippy gate is
`cargo clippy --workspace --all-targets --all-features --
-D warnings -W clippy::pedantic` with zero warnings.
