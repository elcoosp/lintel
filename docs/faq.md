# FAQ

The three questions every launch thread asks (architecture doc §16), with
answers that link to something you can run.

## How is this different from the native JSX RFC?

Lintl is a userland implementation of the *same tag-case heuristic,
fragments and named-arg components* the RFC proposes — it is evidence for
that discussion, not a competitor to it. The differences that matter:

- **Lowering.** The RFC compiles to `new \Markup\Element` values; Lintl's
  default target compiles to plain string concat. The `--target=rfc` output
  mirrors the RFC lowering for parity experiments, and
  `lintl migrate --to=native` is a thin alias over it (asserted by
  `crates/lintl-cli/tests/cli.rs::migrate_is_a_thin_alias_for_build_target_rfc`
  — `crates/lintl-cli/tests/cli.rs`), so a codebase that outlives Lintl
  can walk to the native runtime when it lands.
- **Availability.** The RFC targets PHP 8.7/9 and is still under
  discussion; Lintl runs on PHP ≥ 8.6 today. `docs/rfc-alignment.md`
  tracks the RFC point by point and is kept current on purpose (T-60).
- **Tooling.** The RFC specifies a language; Lintl ships the surrounding
  machinery — diagnostics with source maps, `check` for CI, inventory
  stats, suppression directives, a WASM playground. Those are compiler
  features the native runtime will not carry (§17's differentiation note).

## Why not just use Blade / Twig?

Template engines and Lintl solve different problems:

- **Escape-by-context.** Blade's `{{ }}` escapes; but every raw spot
  (`{!! !!}`, `@php`, component slots) reopens the hole, and nothing checks
  *attribute context* — a `javascript:` URL or an `on*` handler inside a
  spread arrives unvalidated. Lintl routes every dynamic attribute through
  its helper contract (§7.1): URL allowlist, `on*` rejection, style charset
  — enforced by the executable threat corpus
  (`php/runtime/tests/ThreatCorpusTest.php`).
- **One language.** A Lintl component is a PHP function returning markup
  (`function Card(...): Html`), so IDE navigation, PHPStan and autoloading
  work on the *source*, not on a compiled template cache. There is no
  second syntax to learn — JSX appears only where an expression starts
  (§4.1).
- **Line numbers.** Compiled output keeps every non-JSX token on its source
  line (`crates/lintl-core/tests/line_preservation.rs`), so stack traces
  and PHPStan results point at your lines. Blade/Twig map through
  compiled templates.
- **Honest speed.** Lintl renders S1 (static-heavy) at ~0.9 µs/render and
  S2 (1,000-row table) at ~1.3 ms/render on the pinned harness — ahead of
  Blade/Twig there, behind hand-written `echo`. The full matrix, method
  and raw JSON are committed in `bench/results/` — reproduce with
  `make bench` (§9.2 publishing rule).

## Is the binary safe to install?

The trust model is layered (§12, §7's "Binary-via-Composer distrust" row):

1. **Production needs no binary at all.** `lintl/runtime` is pure PHP —
   the compiled output it executes is plain string concat. The binary is a
   *build-time* tool only.
2. **Checksums + attestation.** Every release ships `SHA256SUMS` and
   GitHub build-provenance attestations (`.github/workflows/release.yml`);
   the Composer installer plugin verifies the download before it lands in
   `vendor/bin/lintl` (`php/lintl/src/Installer/BinaryInstaller.php`).
   Pin the digest in `composer.json` `extra` for reproducible CI (§12.3).
3. **Alternative paths.** If a binary-via-Composer step is unacceptable:
   `cargo install lintl-cli` (build from the auditable source), or compile
   in CI and ship the artifacts yourself.
4. **No runtime network.** The compiler never phones home; the dev loader
   (`Lintl\Dev\Loader`) reads local files, and the security reporting
   policy lives in `SECURITY.md`.
