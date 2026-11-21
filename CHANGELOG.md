# Changelog

All notable changes to Lintel are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the project
versions with SemVer, and the language spec versions separately
(architecture doc §12.4).

## [Unreleased]

### Added

- **§10 server adapters** (`php/runtime/src/transports.php`): the
  `SwooleFlushTransport`, `ReactPHPFlushTransport`, `FrankenPHPFlushTransport`
  and `RoadRunnerFlushTransport` lanes on the existing `FlushTransport`
  seam — each only owns HOW a chunk reaches the wire, so streaming never
  bypasses the escapers. The RoadRunner lane is exercised end-to-end
  against a real `rr serve` application server (the php job's new
  `adapters` CI step installs rr and runs the suite with `LINTEL_E2E_RR=1`;
  the FrankenPHP lane runs over a real `php -S` server; the Swoole and
  ReactPHP lanes run against structural fakes — the extensions ship no
  autoloadable surface).
- **The RFC convergence runtime** (`php/markup`, `markup/markup`):
  `\Markup\Element` / `\Markup\Fragment` / `\Markup\Html` + `render()` —
  the runtime side the `--target=rfc` lowering needed
  (`docs/rfc-alignment.md`). Escape-by-default at print time with the same
  attribute routing the string target's helpers enforce; the e2e compiles
  one fixture with BOTH targets and asserts `\Markup\render(...)` renders
  the string target's bytes (`\Lintel\c(...)`). The package is standalone:
  RFC output runs with zero Lintel dependency.
- **The Laravel Blade bridge** (`php/blade`, `lintel/blade`): the
  `@lintel('Badge', ['name' => $user])` directive — compiles to a bridge
  render call, loads `.lintel.php` components through the §6.10 dev
  loader (compile-if-stale; prod builds autoload), renders through `c()`.
- **The Symfony Twig bridge** (`php/twig`, `lintel/twig`): the
  `lintel_render('Badge', {name: user})` Twig function — same loader +
  escaper contract, marked safe for markup so Twig does not double-escape
  the runtime's output.
- **§10 streaming & suspense runtime** (`docs/stream-alignment.md`):
  `Lintel\stream($tree, ?callable $flush = null)` walks a stream-target
  render tree progressively (chunk order is the flush order); the
  `<Suspense>` boundary (`new \Lintel\Suspense(fallback: …, work: …)`)
  flushes its placeholder immediately, resolves the work in a fiber pumped
  round-robin, and flushes the resolved content **out of order** inside
  `<template id="lt-N-r">…</template><script>lintelSwap("lt-N")</script>`
  with a one-time 184-byte swap runtime replacing the placeholder in place.
  Resolved values render through `c()` — no streaming path bypasses the
  escapers (streaming threat cases in the §7.2 corpus) — the sync target
  runs boundaries inline, and the flush-order e2e lives in
  `php/runtime/tests/StreamingSuspenseTest.php`.
- **§10 flush transports** (`Lintel\FlushTransport` + `stream_via`): the
  CLI lane (`CliFlushTransport`) and the FPM lane (`FpmFlushTransport`,
  `X-Accel-Buffering: no` + buffered-output teardown), the FPM adapter
  exercised over a real `php -S` HTTP server in
  `php/runtime/tests/StreamingTransportTest.php`; the four server adapters
  (RoadRunner/Swoole/FrankenPHP/ReactPHP) landed on top of the same seam.
- The §10 suspense demo: `php scripts/suspense-demo.php` (capture) or
  served with `php -S` for the progressive HTTP view.
- **Type-driven escape elision** (§7.3-Y2, `docs/type-elision.md`): the
  `lintel analyze` path — `php/lintel-phpstan/bin/analyze` runs the PHPStan
  bridge's `LintelTypesCollector` over the prepared cache and writes
  `<source>.lintel-types.json`; `lintel analyze <paths> [--types PATH]
  [--target=…][--format=text|json]` compiles with the proven
  `int`/`float`/`numeric-string`/`literal` children folded past the v0.1
  ladder (never over it), reports the elision counters (children /
  considered / elided / rate) on stderr, and compiles byte-identical to
  `lintel build` when the report is absent. `bool` deliberately keeps the
  helper: `c()` renders bools as `''` while raw interpolation renders
  `'1'` — no fold preserves the output. The `analyze_children` bench
  drift-checks the counters.

### Fixed

- The pedantic-clippy gate regression in the newest CLI code (four
  warnings): `lintel-cli`'s unused `types_flag` helper is used again by the
  `--types` parse arms (the inline duplicates are gone), its mangled doc
  comment is restored to `parse_args`, the duplicated `--to` handling is
  the shared `migrate_to` helper (`parse_args` back under the
  `too_many_lines` budget), and `tests/stale_hints.rs` uses the
  message-carrying `assert!(…is_empty(), …)` and drops a needless borrow.
  The gate (`clippy --workspace --all-targets --all-features -- -D
  warnings -W clippy::pedantic`) is zero-warning again, no `#[allow]`
  anywhere.
- The `analyze_children` bench drift-check was red on `main` (never able
  to fire): the hint scanner grepped for `page {{$page}} of` — the
  double-brace spelling as written in Rust source — while the source
  builder's `writeln!` format escaping emits the literal `page {$page} of`
  (single braces), so the scan found zero hints and the bench died on
  `assert_eq!(hints.len(), 20)` before measuring. The scanner now matches
  the emitted bytes and the drift-check runs (40 dynamic children / 20
  elided / rate 0.5).
- Root PHPStan level max is clean again: the bridge's `json_decode` shapes
  decode through `Bridge::decodeDiagnostic` into validated field shapes
  (no ignores, no level changes), the Plugin's `prod.artifacts` lookup
  validates the `prod` object first, and the Composer autoloader map reads
  its documented shape directly.
- `scripts/php-gates.sh` `php -l` no longer sweeps the `.lintel.php` JSX
  fixtures (compiler input, not PHP — the exclusion phpstan.neon already
  documents), and its subcommand mode propagates failures instead of
  exiting 0 on red (a CI blind spot).

## [0.1.0] — 2026-11-20

First public release: the MVP scope of the 14-day plan (architecture doc
§15) — compiler core, CLI, PHP runtime, dev loader, playground, benchmark
harness and docs site v0.

### Compiler — `lintel-core`

- PHP-aware scanner: strings, heredocs/nowdocs, comments, `#[attributes]`,
  inline HTML and `?>` pass through **byte-identically** when a file holds
  no JSX (T-03 golden suite).
- JSX parser + string-target emit with JSX whitespace rules (T-04), raw-text
  `<script>`/`<style>` bodies (§4.7.1) and line-preserving output — every
  non-JSX token keeps its source line (T-06 invariant test).
- `--target=stream` lowers components to generator closures whose `use`
  lists carry every referenced local, transitively through nested children
  closures (F1, §21.2).
- `--target=rfc` emits `\Markup\*` lowering for RFC parity experiments.
- Attribute helpers: static folding (§4.4), URL allowlist (`u()`), srcset
  candidate validation, `cls()`/`sty()` maps, spread routing with `on*` and
  unsafe-name rejection (§7.1/§7.2 threat corpus).
- Escape elision for typed `string` parameters, `(int)`/`(float)` casts and
  `count()` (§7.3 v0.1).
- Diagnostics with miette rendering, JSON output (`lintel build --format
  json`), source maps (`--map`), visitor and rule-registry seams,
  cross-file signatures, component inventory and suppression directives.

### CLI — `lintel-cli`

- `lintel init` scaffolds a runnable demo; `lintel build` (with blake3
  cache, rayon parallelism, `--in-place`/`--out`), `lintel check`,
  `lintel watch`, `lintel stats` and the `init --demo N` gallery (T-10,
  T-15).

### PHP packages

- `lintel/runtime` — the only production dependency: `Html`, `c()/a()/u()/
  srcset()/cls()/sty()/attrs()` (§6.8), threat-corpus PHPUnit suite, PHPStan
  level max clean.
- `lintel/lintel` — Composer installer plugin (verified binary download)
  and the no-watcher dev loader `Lintel\Dev\Loader::boot()` (T-11).

### Tooling & distribution

- GitHub Actions: workflow-lint (actionlint), rust (fmt/clippy pedantic/
  deny/tests/benches), php (composer validate, `php -l`, PHPUnit, PHPStan
  max), dx (fresh plain-PHP and Laravel skeletons), bench (Docker-pinned
  PHP 8.6 digest), wasm (size budget + bundle rebuild), docs (site build +
  link check), editors (VS Code grammar tokenization), fuzz (nightly,
  ten-minute `compile()` acceptance).
- WASM playground with side-by-side output, diagnostics panel, permalinks
  and offline support; the bundle + page gzip far under the 1 MB budget
  (T-13).
- Docs site v0 with registry-driven error index and benchmark methodology
  (T-17).
- VS Code TextMate grammar + `lintel-check` problem matcher, tokenized by
  vscode-textmate in CI (T-16).
- Differential acceptance (T-09 §13.2): 220 property-based random templates
  executed on the string **and** stream targets and compared byte-for-byte
  with a hand-written reference PHP renderer, plus 24 isolated regression
  cases for the class-list, spread and self-closing divergence classes.
- Release engineering (T-18): dual MIT/Apache-2.0 license files,
  `SECURITY.md`, `CODE_OF_CONDUCT.md`, signed-release workflow with
  per-platform checksums and GitHub build-provenance attestation.

[0.1.0]: https://github.com/elcoosp/lintel/releases/tag/v0.1.0
