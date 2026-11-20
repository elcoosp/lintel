# Changelog

All notable changes to Lintel are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the project
versions with SemVer, and the language spec versions separately
(architecture doc §12.4).

## [Unreleased]

### Added

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
