# Security model

HTML templating is the code path where untrusted data becomes markup, so
Lintel's design starts from one rule: **escape by default, and reject the
sinks escaping cannot fix.** Every dynamic value in emitted markup goes
through a context-aware helper; every helper ships in
`php/runtime/src/functions.php` and every one is exercised by the §7.2
PHPUnit threat corpus in CI — 31 hand-written tests over the vector list
below, PLUS the M1 expanded corpus of 893 REAL published payloads (OWASP
XSS Filter Evasion Cheat Sheet + PortSwigger Web Security Academy XSS
cheat sheet), each compiled with the real `lintel` binary and executed
under real PHP to assert inertness — plus PHPStan level max over the
runtime and the M1 independent oracle (Lintel's `c()`/`a()` compared
byte-for-byte against Blade's `{{ }}` and Twig's autoescape).

## The context matrix

| Context | Compiler emits | Rule |
|---|---|---|
| HTML text | `\Lintel\c()` (or a compile-time fold) | `htmlspecialchars(ENT_QUOTES \| ENT_SUBSTITUTE \| ENT_HTML5)` |
| Quoted attribute | `\Lintel\a()` | same escaping + `null`/`false` omit, `true` emits bare |
| URL attributes (`href`, `src`, `action`, `poster`, …) | `\Lintel\u()` | strips whitespace/control bytes before probing, then allowlists `http, https, mailto, tel, sms` + relative; anything else renders `about:invalid#blocked` |
| `srcset` | `\Lintel\srcset()` | splits candidates (a comma separates only when followed by whitespace or end), scheme-checks each URL |
| `class` / `style` | `\Lintel\cls()` / `\Lintel\sty()` | list/map helpers; style values charset-restricted |
| spread `{...$attrs}` | `\Lintel\attrs()` | each entry re-runs the attribute rules — a spread key named `onclick` is still a compile-time error |

Percent-encoding is deliberately **not** treated as a defense against
`javascript:` — the allowlist decides on the scheme, not on encodings, and
obfuscated forms (`java\tscript:`, `JaVaScRiPt:`) resolve to blocked. An
entity-encoded payload like `&#x6A;avascript:` never gets that far: it
arrives as a *string value*, the `&` escapes, and the text renders inert.

## The rules that refuse to escape

Escaping cannot fix a value that is *supposed* to be code, so those sinks
are rejected at compile time:

- **Dynamic event handlers** (`onclick={$handler}`) are an error, LNT0301 —
  untrusted data would become executable JavaScript. The fix is
  `data-action={...}` plus a listener, or an `hx-*`/`x-on:*` binding.
- **Static handler strings** (`onclick="track('cta')"`) compile with a
  warning, LNT0302 — inline JS bypasses CSP nonces and cannot be
  tree-shaken.
- **Void elements with children** (`<br>x</br>`) are an error, LNT0401.
- **`<script>` bodies** never interpolate `{}`; data crosses only as JSON
  islands with the `JSON_HEX_*` flags, or through a `<Script data={...}>`
  component.

## Escape elision — the honest version

Escaping has a cost, so the compiler folds what it can **prove** at compile
time: string and integer literals, `(int)`/`(float)` casts spanning the
whole expression, `count()` calls, and children that are exactly a
parameter the enclosing function declares as non-nullable `string` (the
scanner tracks function scopes and un-proves any name the body can rebind).
Everything else keeps the helper — a false negative costs one call at
render time, while a false positive would smuggle markup into the output.
The later PHPStan-backed pass must report its elision rate in the
benchmarks rather than widen the proof silently.

## The one deliberate raw sink

`Html::unsafeRaw()` exists for pre-sanitized HTML (think a mature
sanitizer's output). It is the only path that does not escape, it is a
single greppable name, and it is the thing a Semgrep/PHPStan rule ships
with. Everything else in a Lintel codebase is escaped by construction.

## What the threat corpus covers

`java\tscript:`, `JaVaScRiPt:`, `data:text/html`, `vbscript:`,
attribute-breakout (`" onmouseover=`), `</script>` inside JSON islands, NUL
bytes, invalid UTF-8 (`ENT_SUBSTITUTE` renders the replacement character),
spread with an `onclick` key, `<`-in-attribute-name injection via spread
keys. Each vector is a named PHPUnit test that fails CI on regression; the
[XSS gallery demo](/playground/) renders the same payloads in the browser.
The full diagnostic catalogue lives in the [error index](/errors/).

## The M1 expanded corpus — independent, published, attributed

The §7.2 hand-written corpus (31 tests) was authored by the same agent that
wrote the escaper, so oracle and implementation shared a blind spot by
construction. The M1 release-readiness card closes that gap two ways:

- **893 REAL published payloads** (OWASP XSS Filter Evasion Cheat Sheet, CC
  BY-SA 4.0; PortSwigger Web Security Academy XSS cheat sheet) — each
  compiled with the real `lintel` binary, the compiled PHP executed under
  real PHP, and the rendered output asserted inert (no payload-injected
  `<script>`/`<img>`/`<svg>`/`<iframe>`/`<style>`/`<object>`/`<embed>` tag,
  no payload-injected `on\w+=` event handler, no live `javascript:`/
  `vbscript:`/`data:text/html` scheme at the start of a rendered URL
  attribute). A compile-time validator rejection (LNT0301 etc.) is the
  strongest inertness and is counted as a passing vector with its code
  pinned. Run `bash scripts/php-gates.sh test` (the `ExpandedThreatCorpusTest`
  lane, driven by `php/runtime/tests/ExpandedThreatCorpus.php`).
- **An independent second oracle** (`IndependentOracleTest`): the same
  dynamic values rendered through Blade's `{{ }}` (Laravel's
  `htmlspecialchars`) and Twig's autoescape, compared byte-for-byte against
  Lintel's `c()`/`a()`. A disagreement with a third-party engine is a
  finding, not a rounding error. The oracle's first finding: Lintel escapes
  `'` as `&apos;` (it uses `ENT_QUOTES | ENT_SUBSTITUTE | ENT_HTML5`, the
  modern HTML5-correct entity); Blade and Twig escape `'` as `&#039;`
  (`ENT_QUOTES` without `ENT_HTML5`, the older numeric entity). Both are
  valid HTML5 escaping of the apostrophe and render identically in every
  browser — an investigated, inert encoding-style choice, not an escape
  hole. The oracle normalizes the two equivalent apostrophe entities before
  the byte-for-byte comparison so a REAL divergence (which would be a bug)
  is never masked.
