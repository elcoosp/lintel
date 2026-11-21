# Launch posts (T-19, architecture doc §16)

A commit is **3–280 characters**, plain text: one hook, one link. The three
variants below are the §16 templates with `{N}` filled **only** from
reproducible sources. Each variant was measured with `wc -m` (the raw
character counts, emoji-free) and every claim links to something you can run
or read — never adjectives (§16 credibility rule).

## Post #1 — the hook

```
Lintel: JSX for PHP 8.6. <Card user={$u}/> compiles to plain string concat — auto-escaped, no VDOM, line numbers preserved. Rust compiler, WASM playground → lintel.sh
```

- length: `wc -m` = **166** ≤ 280
- claims → evidence: "compiles to plain string concat" and "line numbers
  preserved" are the golden fixtures in `crates/lintel-core/tests/golden.rs`
  + the line-preservation invariant `crates/lintel-core/tests/line_preservation.rs`;
  "auto-escaped" is the §7.2 threat corpus (`php/runtime/tests/ThreatCorpusTest.php`);
  the playground link is the T-13 bundle (`playground/`, size budget
  asserted by `crates/lintel-wasm/tests/size_budget.rs`).
- bench → evidence (§9.2 publishing rule): the compile's speed lives in
  `bench/results/` — **S1 median: 915 ns** per render (opcache-off,
  30 runs), against Blade's 13.93 µs on the same page — reproducible with
  `make bench`, raw JSON
  `bench/results/results-native-8.4.11-2026-11-20.json`.

## Post #2 — security demo

```
Wrote a JSX compiler for PHP. Output is readable PHP, XSS-safe by default (31 hostile payloads in the threat corpus, 0 get through). Try it in the browser: lintel.sh/play
```

- length: `wc -m` = **170** ≤ 280
- `{N}` = **31**: the §7.2 threat-corpus case count — 29 test methods in
  `php/runtime/tests/ThreatCorpusTest.php` (1 of them a 3-case
  `#[DataProvider]` for scheme obfuscations), every one asserting the
  payload prints **escaped** (`phpunit --list-tests` counts 31). The
  committed demo carries the same vectors condensed:
  `lintel init --demo 2` writes `demo2-xss-gallery.lintel.php`, whose
  `PAYLOADS` const holds 8 entries (`javascript:`, `JaVaScRiPt:`,
  `data:text/html;base64`, `vbscript:`, tab-obfuscated `java\tscript:`,
  `<script>`, attribute breakout `" onmouseover=`, `</script>`) — run it:
  the demo renders every payload escaped. The count is cross-checked by
  `scripts/validate-launch-assets.sh` against the corpus and the
  docs-site security page (31 = 31).

## Post #3 — the RFC story

```
Native JSX-in-PHP is an RFC for 8.7/9. Lintel lets you write it today and run `lintel migrate --to=native` when it lands. lintel.sh
```

- length: `wc -m` = **131** ≤ 280
- claims → evidence: "an RFC for 8.7/9" is tracked with sources in
  `docs/rfc-alignment.md` (T-00); `lintel migrate --to=native` **exists**
  — a thin alias over `build --target=rfc`, asserted by
  `crates/lintel-cli/tests/cli.rs::migrate_is_a_thin_alias_for_build_target_rfc`
  — so the post never claims a nonexistent command (launch honesty, §16).

## Character counts (verified)

```
$ wc -m post1.txt post2.txt post3.txt
 166 post1.txt
 170 post2.txt
 131 post3.txt
```

(Re-verify after any edit: a commit over 280 characters is not a commit.)
