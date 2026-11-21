# LINTEL — Master Architecture & Execution Plan

> **Markup is an expression.** A Rust-powered, ahead-of-time JSX compiler for PHP 8.6+.
> Written for execution by an AI agent. Read §0 and §19 first, then execute §15 task cards in order.

| | |
|---|---|
| Plan date | 2026-11-19 (PHP 8.6 GA day) |
| Project name | **Lintel** (CLI: `lintel`, file ext: `.lintel.php`, Composer: `lintel/lintel`, runtime: `lintel/runtime`) |
| Horizon | 14-day MVP → launch → 36-month roadmap |
| Core language | Rust (compiler, CLI, LSP, WASM) + PHP 8.6 (runtime, adapters) |

---

## 0. Read this first: what changed after research

I verified the three source documents against live sources. Several claims need correcting **before** an agent builds on them.

### 0.1 Strategy-changing discovery: a native PHP RFC exists

A **"Native Markup Expressions"** RFC (author Liam Hammett, opened 2026-07-14 on internals, status *Under Discussion*, target *PHP 8.7/9.0*) proposes JSX-like syntax in the engine itself, with a working php-src PR (#22661). Key properties from the RFC text:

- Lowercase tags = HTML elements, capitalized = components (same heuristic as JSX).
- Markup lowers at compile time to `new \Markup\Element('button', ['class' => 'btn'], ['Sign in'])`; fragments `<>…</>`; escape-by-default; HTML-only (not XML).
- Components are called with named arguments, so signatures are type-checkable.
- Internals pushback exists (no runtime validation of tags/attributes; perf of instantiating every element).

**Consequence:** none of the source documents mention this. Lintel must **not** compete with the RFC; it must be its **userland proving ground and migration path**:

1. **Syntax-compatible** with the RFC wherever the RFC is stable (track it; see §4.9).
2. **Differentiated by things a core RFC will not do**: AOT string folding (no element objects), context-aware escaping (URL/JS/CSS), compile-time HTML validity/a11y checking, streaming/Suspense, line-preserving codegen, LSP/formatter, framework adapters.
3. **`lintel migrate --to=native`** rewrites `.lintel.php` → plain `.php` the day the RFC ships. "Zero lock-in" is the pitch to skeptics, and it neutralizes the biggest risk (§17).

### 0.2 Corrections to the source documents

| Claim in source docs | Verdict | Plan response |
|---|---|---|
| PHP 8.6 releases Nov 19, 2026; has Partial Function Application (`?`, `...`), `Time\Duration`, a Polling API, `clamp()`, readonly property defaults, parameter doc comments | **Confirmed** (php.net 8.6 announcement, Laravel News, stitcher.io) | Use PFA, param doc comments. |
| PHP 8.5 pipe operator `\|>` exists; PFA is its missing half | **Confirmed** (stitcher.io, Medium) | Pipes + PFA are legal inside `{…}`; support in lexer. |
| `Io\Poll` is a streaming engine for HTML | **Overstated.** Release page: *"scalable I/O multiplexing… available to event loop libraries like ReactPHP and AMPHP."* It is a low-level primitive, not an HTTP streaming API. | Streaming is built on **Generators + SAPI flush + adapters** (FrankenPHP/RoadRunner/Swoole/Amp/React). `Io\Poll` is only used inside optional async adapters (§10). |
| `Uri\WhatWg\url_percent_encode()` "prevents `javascript:` XSS" | **Wrong.** Percent-encoding does not neutralize a dangerous *scheme*. | Scheme **allowlist** + control-char stripping + attribute escaping (§7). |
| "Infer `int`/`bool` types to skip escaping" | **Not possible in general** without a type analyzer. | Escape-by-default with runtime type switch; skip only for provably safe cases (literals, typed params, casts); deeper elision via PHPStan integration in Y2 (§7.3). |
| "5×–10× faster than Blade/Twig", "5 ms for 500 components", "0.3 ms daemon call" | **Unverified targets, not facts.** | Treated as *hypotheses*. Nothing is published until measured with the harness in §9. |
| dev-null.sh posts "up to 4,000 characters" | **Wrong.** Commit body is **3–280 characters** (Symfony bridge README; news coverage). Sign-in via GitHub; commit from terminal signed with an SSH key on your GitHub account. | Launch copy rewritten for 280 chars (§16). |
| dev-null.sh creators | Nuno Maduro, Brent Roose, Matthieu Napoli built it in a public 48h sprint (Laravel News, runtimewire). The "Pauline Vos" claim is **unverified** — dropped. | Don't tag people in launch copy; earn attention with the demo. |
| "Nobody has built a PHP JSX compiler" | **False.** Prior art: `syntaxx/phpx-compiler` (JSX→PHP + WASM, v0.1.0, ~57 installs), `pre/phpx` (2021, abandoned), `phpmystic/liqx` (JSX-style templates), `lmc/twigx-bundle` (Twig + JSX-ish). All small. | Position as **"the first compiler-grade, RFC-aligned, performance-first one"**, not "the first". |
| Name candidates "BladeX", "HyperJSX", "Zest" | `BladeX` is confusable with Laravel's Blade ecosystem. | Chosen name: **Lintel** (§2). |
| Doc 1 shows components returning `string` | **Unsafe design**: loses "already-escaped" provenance → double-escape or XSS. | Components return `\Lintel\Html` (a tiny safe-string value object) (§6.4). |
| Doc 3: "you have already written three compilers in Rust" | Taken from the document as user context; cannot verify. | Plan assumes strong Rust ability but keeps the core small and heavily tested so any agent can extend it. |

### 0.3 Decisions made (resolving the conflict between the 3 docs)

| Decision | Choice | Why |
|---|---|---|
| Compiler language | **Rust** (workspace), compile to native + WASM | Speed, WASM playground for free, credibility; the PHP-only route (Doc 1/2) cannot serve an LSP and a playground without a PHP runtime. |
| Parse full PHP? | **No.** A PHP-*aware scanner* that passes PHP through verbatim and only parses JSX. | Full PHP 8.6 grammar is a moving target; verbatim pass-through = never break user code. |
| Output | **String-concat AOT** by default; `--target=rfc` lowers to `\Markup\Element`; `--target=stream` emits generators | Perf default + forward-compat + streaming. |
| Components | **Functions first** (named-arg calls), classes in v0.3 | Matches RFC semantics; simplest correct thing. |
| Prod vs dev | **Prod ships compiled `.php` only** (no Rust, no runtime compiler). Dev uses watcher/daemon. | Removes the "binary in production" objection. |
| Syntax extension | `.lintel.php` | Editors treat it as PHP out of the box; avoids the crowded `.phpx` namespace. |

---

## 1. Research summary (verified, with sources)

**Language/ecosystem**
- PHP 8.6 GA 2026-11-19; PFA v2 RFC passed 33–0; every `?` becomes a *required* parameter of the closure; args are evaluated **eagerly at partial creation** (footgun to document). — php.net/releases/8.6, laravel-news.com/php-8-6, levelup.gitconnected.com article
- Parameter doc comments via `ReflectionParameter::getDocComment()`; readonly property defaults; `SortDirection` enum; Polling API. — stitcher.io/blog (2026-07-29), daily.dev
- **Native Markup Expressions RFC** — wiki.php.net/rfc/native_markup_expressions, externals.io/message/131931, php-src PR #22661.
- XHP history (Facebook) — why a C-extension/fork stalled in PHP while thriving in Hack; the RFC itself discusses why an extension can't prove it out (source-rewriting breaks line numbers and `token_get_all()` tooling). **Lintel's line-preserving codegen is a direct answer to that criticism.**

**Prior art in userland** — see §0.2. All small; none is compiler-grade, RFC-aligned, or has LSP/streaming/benchmarks.

**Launch channel**
- dev-null.sh: GitHub login, ≤280-char plain-text posts, chronological feed, open-source, CLI posting with SSH signing. Built by Maduro/Roose/Napoli in a public sprint. Community = PHP/Laravel/Symfony developers. — laravel-news.com/social-network-php-48-hours, packagist.org/packages/gromnan/devnull-notifier, runtimewire.com
- Implication: a 280-char post must carry **one screenshot-worthy idea + one link**. The playground URL is the product.

**What the PHP community rewards** (inferred from FrankenPHP/Mago/Pest/Pint success; treat as hypothesis): a single install command, honest benchmarks, great error messages, and "feels like it was always part of the language."

---

## 2. Name

**Lintel** — the horizontal beam that spans an opening and carries the load above it. Markup spanning PHP and HTML; structural, small, strong.

- Short, pronounceable, unique in the PHP space (no known major PHP project of that name — **Day-0 task: verify** Packagist `lintel/*`, GitHub org, `lintel.sh`/`lintel.dev`, crates.io, npm, trademarks).
- Brand voice: terse, engineering-first. Tagline **"Markup is an expression."** Alt: *"JSX for PHP. Compiled. Escaped. Gone by runtime."*
- CLI verbs read well: `lintel build`, `lintel watch`, `lintel check`, `lintel fmt`.
- Fallback shortlist if taken: **Pylon**, **Vellum**, **Gable**, **Chevron** (⚠ existing `chevron/widgets` Packagist package and a Python "chevron" mustache lib).

---

## 3. Product principles & non-goals

1. **Valid PHP outside the braces.** Never rewrite what isn't JSX.
2. **The compiler is a transpiler, not a language.** Output must be readable, debuggable PHP.
3. **Secure by default; unsafe is loud** (`Html::unsafeRaw()` greppable).
4. **Line-preserving.** Stack traces, Xdebug, and PHPStan report the *author's* line numbers.
5. **No runtime compiler in prod.** Zero extra moving parts at deploy.
6. **Measured, not claimed.** Every perf statement links to a reproducible benchmark.
7. **Exit ramp:** `lintel migrate --to=native`.

**Non-goals:** a Virtual DOM; client-side hydration framework (we interoperate with HTMX/Alpine/Datastar); a new template *language* with its own control flow (control flow is PHP); XML/SVG-namespace generality in v1.

---

## 4. Language specification (v1)

### 4.1 Where JSX may appear
JSX is recognized **only in expression-start position**: after `return`, `echo`, `print`, `yield`, `=`, `=>`, `(`, `,`, `[`, `?`, `:`, `.`, `&&`, `||`, `??`, `and/or/xor`, `|>`, and operators, or at the start of an argument. After an *operand* (variable, literal, `)`, `]`, identifier, number), `<` is the less-than operator. This single rule resolves `$a < $b` vs `return <div/>`.

### 4.2 Elements
```php
return <section class="hero" id={$id}>
    <h1>{$title}</h1>
    <img src={$src} alt="" />
</section>;
```
- Lowercase first letter ⇒ HTML element. Void elements (`br, hr, img, input, meta, link, area, base, col, embed, source, track, wbr`) emit without a close tag; `<div />` on non-void emits `<div></div>`.
- Capitalized (or contains `\`) ⇒ **component**.

### 4.3 Fragments
`<>…</>` — no wrapper element.

### 4.4 Attributes
| Form | Meaning |
|---|---|
| `name="str"` / `name='str'` | static, folded at compile time (`"` re-escaped to `&quot;`) |
| `name={expr}` | dynamic; `null`/`false` omit the attribute, `true` emits bare attr, else escaped |
| `name` (bare) | boolean `true` |
| `{...$attrs}` | spread (array); each entry goes through the same attribute rules |
| `class={['a', 'b' => $cond]}` | class-list helper (strings, lists, `['cls' => bool]`) |
| `style={['color' => 'red']}` | style map, values validated (§7.2) |
| `href`, `src`, `action`, `formaction`, `poster`, `cite`, `data`, `srcset`(parsed per-URL) | **URL attributes**: scheme allowlist (§7.1) |
| `onclick={…}` etc. | **compile error** for dynamic values (use `data-*` + JS); static strings allowed with a lint warning |

### 4.5 Children `{expr}`
Accepts: `Html`, `string` (escaped), `int|float` (cast), `null|bool` (empty — same as React), `iterable` (each child rendered recursively), `Stringable` (escaped), anything else → `InvalidArgumentException` with source line.

### 4.6 Whitespace
Exactly JSX's rules: lines are trimmed; blank lines dropped; line breaks between text and an element collapse; whitespace on a single line is preserved. This is the part people get wrong — golden-test it (T-04).

### 4.7 Components
```php
function Card(string $title, ?Html $children = null, string $variant = 'plain'): Html {
    return <div class={['card', "card--$variant"]}>
        <h2>{$title}</h2>{$children}
    </div>;
}

echo <Card title="Hi" variant="wide"><p>Body</p></Card>;
```
- Attributes → **named arguments**. Hyphenated names (`data-x`) can't be named args → use spread: `<Card {...['data-x' => 1]} />` (compiler error message suggests this).
- Children → named argument `children` (an `Html`), passed **only if present**.
- `{...$props}` on a component → PHP argument unpacking with string keys.
- ⚠ **Builtin shadowing:** `<Header>` would call PHP's `header()` (function names are case-insensitive). The compiler ships the PHP builtin function list and raises `LNT0201` unless the component is declared in the file or imported via `use function`.
- Classes (v0.3): `<Card>` resolves to a class with `__invoke` or static `render` when imported by `use`.

### 4.7.1 Raw-text elements
`<script>` and `<style>` bodies are **verbatim** (no `{}` interpolation — JS/CSS braces conflict). Data goes through `<script type="application/json">{$data}</script>` (v0.2: JSON-encoded with `JSON_HEX_TAG|JSON_HEX_AMP|JSON_HEX_APOS|JSON_HEX_QUOT`) or the `<Script data={…}/>` component.

### 4.8 Comments
`{/* comment */}` produces nothing.

### 4.9 RFC alignment table (keep current — task T-60)
| Feature | RFC | Lintel |
|---|---|---|
| Tag-case heuristic | ✔ | ✔ |
| Fragments | ✔ | ✔ |
| Named-arg components | ✔ | ✔ |
| Escape-by-default | ✔ | ✔ + context-aware |
| Lowering | `new \Markup\Element` | string concat (default), `--target=rfc` for parity |
| Value type | `\Markup\Html` | `\Lintel\Html` (adapter to `Markup\Html` when present) |

*The RFC is a moving target — the agent must re-read the RFC and php-src PR before implementing `--target=rfc` and record the exact class names in `docs/rfc-alignment.md`.*

---

## 5. System architecture

```
                    ┌────────────────────────── dev ───────────────────────────┐
 Foo.lintel.php ──► │ lintel watch / daemon / stream-wrapper loader / LSP / WASM │
                    └──────────────┬─────────────────────────────────────────────┘
                                   ▼
        ┌──────────────────── lintel-core (Rust, no I/O) ────────────────────┐
        │ 1. scan   PHP-aware scanner: strings, heredoc, comments, #[attr],  │
        │           prev-token class → JSX start detection                   │
        │ 2. parse  JSX recursive descent (elements, attrs, children, exprs) │
        │ 3. lower  whitespace normalize → Parts (Lit | Dyn) + line offsets   │
        │ 4. opt    fold literals, merge statics, escape elision, validate    │
        │ 5. emit   target: string | stream | rfc  (line-preserving)         │
        │ 6. diag   miette/ariadne diagnostics with LNT codes                 │
        └───────────────────────────────┬────────────────────────────────────┘
                                        ▼
                         Foo.php  (+ Foo.php.map, optional)
                                        │
 prod:  `lintel build` → vendor-free PHP + lintel/runtime (≈300 LOC) ───────────► OPcache
```

### 5.1 Repository layout (monorepo)

```
lintel/
├─ AGENTS.md                      # §18 operating manual (copy verbatim)
├─ Cargo.toml                     # workspace
├─ crates/
│  ├─ lintel-core/                # scan, parse, lower, opt, emit  (no_std-friendly, no I/O)
│  ├─ lintel-diag/                # diagnostic types + renderers
│  ├─ lintel-cli/                 # build/watch/check/fmt/init/bench/daemon/migrate
│  ├─ lintel-wasm/                # wasm-bindgen wrapper → playground
│  ├─ lintel-lsp/                 # tower-lsp server (Y1 Q3)
│  ├─ lintel-fmt/                 # formatter (Y1 Q4)
│  └─ lintel-html-spec/           # HTML content-model + a11y tables (generated)
├─ php/
│  ├─ runtime/                    # composer: lintel/runtime  (prod dependency, <400 LOC)
│  ├─ lintel/                     # composer: lintel/lintel   (dev: installer plugin, loader, daemon client)
│  ├─ phpstan/                    # composer: lintel/phpstan
│  ├─ laravel/  symfony/  psr15/  # adapters
├─ editors/ vscode/ phpstorm/ tree-sitter-lintel/
├─ playground/                    # static site + wasm
├─ bench/                         # Docker, PHP 8.6, scenarios, results/
├─ tests/
│  ├─ golden/                     # *.lintel.php → *.expected.php (+ *.stdout)
│  ├─ differential/               # compile → php -l → run → compare to hand-written reference
│  └─ fuzz/                       # cargo-fuzz targets
└─ docs/
```

### 5.2 Compile model
- `compile(src: &str, opts: &Options) -> Result<Output, Vec<Diagnostic>>` — pure function. This is the contract shared by CLI, daemon, LSP, WASM.
- **Incremental** (Y2): file-level content-hash cache first; query-based (Salsa-style) later for LSP.

### 5.3 Targets
| Target | Output | Use |
|---|---|---|
| `string` (default) | functions return `Html` wrapping one concatenated string | fastest sync rendering |
| `rfc` | `new \Markup\Element(...)` | forward-compat/parity tests; migration rehearsal |
| `stream` | Generators yielding chunks, `Suspense` boundaries | streaming SSR (§10) |

---

## 6. Core code (reference skeleton)

> ⚠ **Status of this code:** written to be correct by construction, but **not compiled or run** (no Rust/PHP toolchain was available while preparing this plan). Task T-01…T-06 require the agent to make it compile, add tests, and fix discrepancies. Treat it as a precise design, not a finished artifact.

### 6.1 `Cargo.toml` (workspace)
```toml
[workspace]
resolver = "2"
members = ["crates/lintel-core", "crates/lintel-diag", "crates/lintel-cli", "crates/lintel-wasm"]

[workspace.package]
edition = "2021"
license = "MIT OR Apache-2.0"
rust-version = "1.80"

[profile.release]
lto = "fat"
codegen-units = 1
panic = "abort"
opt-level = 3
```

### 6.2 `lintel-core/src/ast.rs`
```rust
/// Line offsets are relative to the start of the root JSX expression (0-based).
#[derive(Debug, Clone)]
pub enum Node {
    /// Whitespace-normalized text, emitted verbatim (author-trusted, entities pass through).
    Text { text: String, line: u32 },
    /// `{ php }` — `php` is already transpiled (nested JSX replaced).
    Expr { php: String, line: u32 },
    Element(Element),
    Fragment { children: Vec<Node>, line: u32 },
}

#[derive(Debug, Clone)]
pub struct Element {
    pub name: String,          // as written, e.g. "div", "Card", "Ui\\Button"
    pub is_component: bool,
    pub attrs: Vec<Attr>,
    pub children: Vec<Node>,
    pub self_closing: bool,
    pub line: u32,
    pub end_line: u32,
}

#[derive(Debug, Clone)]
pub enum Attr {
    Str   { name: String, value: String, line: u32 },
    Bool  { name: String, line: u32 },
    Expr  { name: String, php: String, line: u32 },
    Spread{ php: String, line: u32 },
}
```

### 6.3 `lintel-core/src/scan.rs` — PHP-aware scanner + JSX parser
```rust
use crate::ast::*;
use lintel_diag::{Diagnostic, Code};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Prev { Start, Operand }

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stop { Eof, Brace }

const START_KEYWORDS: &[&str] = &[
    "return", "echo", "print", "yield", "and", "or", "xor", "case", "else", "throw", "match", "fn",
];

pub struct Scanner<'s> {
    pub(crate) src: &'s str,
    b: &'s [u8],
    pub(crate) pos: usize,
    pub(crate) diags: Vec<Diagnostic>,
}

type R<T> = Result<T, ()>; // details are pushed into `diags`

impl<'s> Scanner<'s> {
    pub fn new(src: &'s str) -> Self {
        Self { src, b: src.as_bytes(), pos: 0, diags: vec![] }
    }

    /// Entry point: whole file → transpiled PHP.
    pub fn transpile_file(&mut self) -> R<String> {
        let mut out = String::with_capacity(self.src.len() + self.src.len() / 4);
        while self.pos < self.b.len() {
            self.copy_inline_html(&mut out);
            if self.pos >= self.b.len() { break; }
            self.php(&mut out, Stop::Eof)?;
        }
        Ok(out)
    }

    /// Copy until `<?php` / `<?=` (inclusive of the open tag).
    fn copy_inline_html(&mut self, out: &mut String) {
        let start = self.pos;
        let rest = &self.src[self.pos..];
        let idx = rest.find("<?php").map(|i| (i, 5)).into_iter()
            .chain(rest.find("<?=").map(|i| (i, 3)))
            .min_by_key(|(i, _)| *i);
        match idx {
            Some((i, n)) => { self.pos = start + i + n; }
            None => { self.pos = self.b.len(); }
        }
        out.push_str(&self.src[start..self.pos]);
    }

    /// Scan PHP, copying verbatim, transforming JSX at expression starts.
    fn php(&mut self, out: &mut String, stop: Stop) -> R<()> {
        let mut depth: usize = 0;
        let mut prev = Prev::Start;
        while self.pos < self.b.len() {
            let c = self.b[self.pos];
            let n1 = self.b.get(self.pos + 1).copied().unwrap_or(0);
            match c {
                b'\'' => { self.copy_single_quoted(out); prev = Prev::Operand; }
                b'"'  => { self.copy_double_quoted(out)?; prev = Prev::Operand; }
                b'`'  => { self.copy_until_unescaped(out, b'`'); prev = Prev::Operand; }
                b'/' if n1 == b'/' => self.copy_line_comment(out),
                b'#' if n1 != b'[' => self.copy_line_comment(out),
                b'/' if n1 == b'*' => self.copy_block_comment(out),
                b'<' if n1 == b'<' && self.b.get(self.pos + 2) == Some(&b'<') => {
                    self.copy_heredoc(out)?; prev = Prev::Operand;
                }
                b'<' if prev == Prev::Start && self.looks_like_tag() => {
                    let line0 = 0u32;
                    let start_pos = self.pos;
                    let node = self.parse_node(line0)?;
                    let total_lines = self.src[start_pos..self.pos].matches('\n').count() as u32;
                    out.push_str(&crate::emit::emit_root(&node, total_lines));
                    prev = Prev::Operand;
                }
                b'?' if n1 == b'>' && stop == Stop::Eof => {
                    out.push_str("?>"); self.pos += 2; return Ok(()); // back to inline HTML
                }
                b'{' => { depth += 1; out.push('{'); self.pos += 1; prev = Prev::Start; }
                b'}' => {
                    if depth == 0 && stop == Stop::Brace { return Ok(()); } // caller consumes '}'
                    depth = depth.saturating_sub(1);
                    out.push('}'); self.pos += 1; prev = Prev::Operand;
                }
                b'$' | b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'\\' | 0x80..=0xff => {
                    let w = self.take_word();
                    prev = if !w.starts_with('$') && START_KEYWORDS.iter().any(|k| w.eq_ignore_ascii_case(k)) {
                        Prev::Start } else { Prev::Operand };
                    out.push_str(w);
                }
                b'0'..=b'9' => {
                    let s = self.pos;
                    while self.pos < self.b.len() && (self.b[self.pos].is_ascii_alphanumeric() || matches!(self.b[self.pos], b'.' | b'_')) { self.pos += 1; }
                    out.push_str(&self.src[s..self.pos]); prev = Prev::Operand;
                }
                b' ' | b'\t' | b'\r' | b'\n' => { out.push(c as char); self.pos += 1; }
                b')' | b']' => { out.push(c as char); self.pos += 1; prev = Prev::Operand; }
                _ => { // other punctuation: next token starts an expression
                    let ch = self.next_char(); out.push(ch); prev = Prev::Start;
                }
            }
        }
        Ok(())
    }

    fn next_char(&mut self) -> char {
        let ch = self.src[self.pos..].chars().next().unwrap();
        self.pos += ch.len_utf8();
        ch
    }

    fn take_word(&mut self) -> &'s str {
        let s = self.pos;
        if self.b[self.pos] == b'$' { self.pos += 1; }
        while self.pos < self.b.len() {
            let c = self.b[self.pos];
            if c.is_ascii_alphanumeric() || c == b'_' || c == b'\\' || c >= 0x80 { self.pos += 1 } else { break }
        }
        if self.pos == s { self.pos += 1; } // lone '$'
        &self.src[s..self.pos]
    }

    /// At '<': fragment `<>` or `<Name`. Rejects `<=`, `<<`, `< `, `<>` as operator is handled by Prev.
    fn looks_like_tag(&self) -> bool {
        match self.b.get(self.pos + 1) {
            Some(b'>') => true,
            Some(c) => c.is_ascii_alphabetic() || *c == b'_',
            None => false,
        }
    }

    // ---------- JSX parser ----------

    fn line_of(&self, upto: usize, from: usize) -> u32 { self.src[from..upto].matches('\n').count() as u32 }

    pub(crate) fn parse_node(&mut self, base: u32) -> R<Node> {
        let root_start = self.pos;
        self.parse_element_or_fragment(root_start, base)
    }

    fn parse_element_or_fragment(&mut self, root: usize, _base: u32) -> R<Node> {
        let line = self.line_of(self.pos, root);
        debug_assert_eq!(self.b[self.pos], b'<');
        self.pos += 1;
        if self.b[self.pos] == b'>' {
            self.pos += 1;
            let children = self.parse_children(root, "")?;
            return Ok(Node::Fragment { children, line });
        }
        let name = self.take_tag_name();
        let is_component = name.chars().next().map_or(false, |c| c.is_ascii_uppercase()) || name.contains('\\');
        let mut attrs = vec![];
        loop {
            self.skip_ws();
            match self.b.get(self.pos) {
                None => return self.err(Code::UnterminatedTag, "unterminated tag"),
                Some(b'/') if self.b.get(self.pos + 1) == Some(&b'>') => {
                    self.pos += 2;
                    let end_line = self.line_of(self.pos, root);
                    return Ok(Node::Element(Element { name, is_component, attrs, children: vec![], self_closing: true, line, end_line }));
                }
                Some(b'>') => { self.pos += 1; break; }
                Some(b'{') => { // spread {...$x}
                    let aline = self.line_of(self.pos, root);
                    self.pos += 1;
                    let mut php = String::new();
                    self.php(&mut php, Stop::Brace)?; self.expect(b'}')?;
                    let t = php.trim_start();
                    if let Some(rest) = t.strip_prefix("...") { attrs.push(Attr::Spread { php: rest.to_string(), line: aline }); }
                    else { return self.err(Code::BadSpread, "expected `{...$expr}`"); }
                }
                Some(_) => attrs.push(self.parse_attr(root)?),
            }
        }
        let raw_text = matches!(name.as_str(), "script" | "style");
        let children = if raw_text { self.parse_raw_children(root, &name)? } else { self.parse_children(root, &name)? };
        let end_line = self.line_of(self.pos, root);
        Ok(Node::Element(Element { name, is_component, attrs, children, self_closing: false, line, end_line }))
    }

    fn parse_attr(&mut self, root: usize) -> R<Attr> {
        let line = self.line_of(self.pos, root);
        let s = self.pos;
        while self.pos < self.b.len() && (self.b[self.pos].is_ascii_alphanumeric() || matches!(self.b[self.pos], b'-' | b'_' | b':' | b'.' | b'@')) { self.pos += 1; }
        if s == self.pos { return self.err(Code::BadAttr, "expected attribute name"); }
        let name = self.src[s..self.pos].to_string();
        self.skip_ws();
        if self.b.get(self.pos) != Some(&b'=') { return Ok(Attr::Bool { name, line }); }
        self.pos += 1; self.skip_ws();
        match self.b.get(self.pos) {
            Some(q @ (b'"' | b'\'')) => {
                let q = *q; self.pos += 1; let vs = self.pos;
                while self.pos < self.b.len() && self.b[self.pos] != q { self.pos += 1; }
                let value = self.src[vs..self.pos].to_string(); self.pos += 1;
                Ok(Attr::Str { name, value, line })
            }
            Some(b'{') => {
                self.pos += 1; let mut php = String::new();
                self.php(&mut php, Stop::Brace)?; self.expect(b'}')?;
                Ok(Attr::Expr { name, php, line })
            }
            _ => self.err(Code::BadAttr, "attribute value must be \"…\", '…' or {expr}"),
        }
    }

    fn parse_children(&mut self, root: usize, parent: &str) -> R<Vec<Node>> {
        let mut out: Vec<Node> = vec![];
        let mut text_start = self.pos;
        loop {
            if self.pos >= self.b.len() { return self.err(Code::UnterminatedTag, "unclosed element"); }
            match self.b[self.pos] {
                b'<' if self.b.get(self.pos + 1) == Some(&b'/') => {
                    self.flush_text(&mut out, text_start, root);
                    self.pos += 2; let n = self.take_tag_name(); self.skip_ws(); self.expect(b'>')?;
                    if n != parent { return self.err(Code::MismatchedClose, &format!("expected </{parent}>, found </{n}>")); }
                    return Ok(out);
                }
                b'<' => {
                    self.flush_text(&mut out, text_start, root);
                    out.push(self.parse_element_or_fragment(root, 0)?);
                    text_start = self.pos;
                }
                b'{' => {
                    self.flush_text(&mut out, text_start, root);
                    let line = self.line_of(self.pos, root);
                    self.pos += 1; let mut php = String::new();
                    self.php(&mut php, Stop::Brace)?; self.expect(b'}')?;
                    let t = php.trim();
                    let is_comment = t.starts_with("/*") && t.ends_with("*/") && t.matches("*/").count() == 1;
                    if !t.is_empty() && !is_comment { out.push(Node::Expr { php, line }); }
                    text_start = self.pos;
                }
                _ => self.pos += 1,
            }
        }
    }

    fn parse_raw_children(&mut self, root: usize, parent: &str) -> R<Vec<Node>> {
        let close = format!("</{parent}");
        let start = self.pos;
        let rel = self.src[start..].find(&close).ok_or_else(|| { let _ = self.err::<()>(Code::UnterminatedTag, "unclosed raw-text element"); })?;
        let text = self.src[start..start + rel].to_string();
        let line = self.line_of(start, root);
        self.pos = start + rel + close.len(); self.skip_ws(); self.expect(b'>')?;
        Ok(if text.is_empty() { vec![] } else { vec![Node::Text { text, line }] })
    }

    fn flush_text(&mut self, out: &mut Vec<Node>, from: usize, root: usize) {
        if from >= self.pos { return; }
        let raw = &self.src[from..self.pos];
        if let Some(t) = crate::text::normalize_jsx_text(raw) {
            out.push(Node::Text { text: t, line: self.line_of(from, root) });
        }
    }

    fn take_tag_name(&mut self) -> String {
        let s = self.pos;
        while self.pos < self.b.len() && (self.b[self.pos].is_ascii_alphanumeric() || matches!(self.b[self.pos], b'_' | b'-' | b':' | b'\\' | b'.')) { self.pos += 1; }
        self.src[s..self.pos].to_string()
    }
    fn skip_ws(&mut self) { while self.pos < self.b.len() && self.b[self.pos].is_ascii_whitespace() { self.pos += 1; } }
    fn expect(&mut self, c: u8) -> R<()> {
        if self.b.get(self.pos) == Some(&c) { self.pos += 1; Ok(()) } else { self.err(Code::Expected, &format!("expected `{}`", c as char)) }
    }
    fn err<T>(&mut self, code: Code, msg: &str) -> R<T> {
        self.diags.push(Diagnostic::new(code, msg, self.pos)); Err(())
    }

    // ---------- verbatim copiers ----------
    fn copy_single_quoted(&mut self, out: &mut String) {
        let s = self.pos; self.pos += 1;
        while self.pos < self.b.len() {
            match self.b[self.pos] { b'\\' => self.pos += 2, b'\'' => { self.pos += 1; break } _ => self.pos += 1 }
        }
        let e = self.pos.min(self.b.len()); out.push_str(&self.src[s..e]);
    }
    /// Double-quoted with `{$expr["k"]}` interpolation support (nested quotes inside braces).
    fn copy_double_quoted(&mut self, out: &mut String) -> R<()> {
        let s = self.pos; self.pos += 1; let mut brace = 0usize;
        while self.pos < self.b.len() {
            let c = self.b[self.pos];
            match c {
                b'\\' => self.pos += 2,
                b'"' if brace == 0 => { self.pos += 1; break }
                b'{' if self.b.get(self.pos + 1) == Some(&b'$') => { brace += 1; self.pos += 1 }
                b'}' if brace > 0 => { brace -= 1; self.pos += 1 }
                b'\'' | b'"' if brace > 0 => { // nested string inside {$a['x']}
                    let q = c; self.pos += 1;
                    while self.pos < self.b.len() && self.b[self.pos] != q { if self.b[self.pos] == b'\\' { self.pos += 1 } self.pos += 1 }
                    self.pos += 1;
                }
                _ => self.pos += 1,
            }
        }
        let e = self.pos.min(self.b.len()); out.push_str(&self.src[s..e]); Ok(())
    }
    fn copy_until_unescaped(&mut self, out: &mut String, q: u8) {
        let s = self.pos; self.pos += 1;
        while self.pos < self.b.len() { if self.b[self.pos] == b'\\' { self.pos += 2; continue } if self.b[self.pos] == q { self.pos += 1; break } self.pos += 1 }
        let e = self.pos.min(self.b.len()); out.push_str(&self.src[s..e]);
    }
    fn copy_line_comment(&mut self, out: &mut String) {
        let s = self.pos;
        while self.pos < self.b.len() && self.b[self.pos] != b'\n' && !(self.b[self.pos] == b'?' && self.b.get(self.pos + 1) == Some(&b'>')) { self.pos += 1 }
        out.push_str(&self.src[s..self.pos]);
    }
    fn copy_block_comment(&mut self, out: &mut String) {
        let s = self.pos;
        self.pos = self.src[s + 2..].find("*/").map_or(self.b.len(), |i| s + 2 + i + 2);
        out.push_str(&self.src[s..self.pos]);
    }
    /// `<<<LABEL` / `<<<"LABEL"` / `<<<'LABEL'` … closing label (PHP ≥7.3 allows indentation).
    fn copy_heredoc(&mut self, out: &mut String) -> R<()> {
        let s = self.pos; self.pos += 3;
        while self.b.get(self.pos).map_or(false, |c| *c == b' ' || *c == b'\t') { self.pos += 1 }
        let q = self.b.get(self.pos).copied().filter(|c| *c == b'"' || *c == b'\'');
        if q.is_some() { self.pos += 1 }
        let ls = self.pos;
        while self.pos < self.b.len() && (self.b[self.pos].is_ascii_alphanumeric() || self.b[self.pos] == b'_') { self.pos += 1 }
        let label = self.src[ls..self.pos].to_string();
        if q.is_some() { self.pos += 1 }
        // find a line that, after optional indentation, starts with the label followed by a non-identifier char
        let mut cur = self.pos;
        while let Some(nl) = self.src[cur..].find('\n') {
            let ls2 = cur + nl + 1;
            let trimmed = self.src[ls2..].trim_start_matches([' ', '\t']);
            if trimmed.starts_with(&label) && !trimmed[label.len()..].starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_') {
                let end = ls2 + (self.src[ls2..].len() - trimmed.len()) + label.len();
                self.pos = end; out.push_str(&self.src[s..end]); return Ok(());
            }
            cur = ls2;
        }
        self.pos = self.b.len(); out.push_str(&self.src[s..]); Ok(())
    }
}
```

### 6.4 `lintel-core/src/text.rs` — JSX whitespace rule
```rust
/// JSX text normalization. Returns None if the text is entirely collapsible whitespace.
pub fn normalize_jsx_text(raw: &str) -> Option<String> {
    if !raw.contains('\n') {
        return if raw.is_empty() { None } else { Some(raw.to_string()) }; // single-line: preserved as-is
    }
    let lines: Vec<&str> = raw.split('\n').collect();
    let last = lines.len() - 1;
    let mut kept: Vec<String> = vec![];
    for (i, l) in lines.iter().enumerate() {
        let mut t: &str = l.trim_matches(|c| c == '\r');
        if i != 0 { t = t.trim_start_matches([' ', '\t']); }
        if i != last { t = t.trim_end_matches([' ', '\t']); }
        if !t.is_empty() { kept.push(t.to_string()); }
    }
    if kept.is_empty() { None } else { Some(kept.join(" ")) }
}
```

### 6.5 `lintel-core/src/emit.rs` — lowering, folding, line-preserving emit (string target)
```rust
use crate::ast::*;

enum Part { Lit(String), Dyn(String) }
struct Parts(Vec<(u32, Part)>); // (line offset, part)

const VOID: &[&str] = &["area","base","br","col","embed","hr","img","input","link","meta","source","track","wbr"];
const URL_ATTRS: &[&str] = &["href","src","action","formaction","poster","cite","data","manifest","ping","background"];

pub fn emit_root(node: &Node, total_lines: u32) -> String {
    let mut p = Parts(vec![]);
    lower(node, &mut p);
    format!("new \\Lintel\\Html({})", concat(p, 0, total_lines))
}

fn lower(n: &Node, p: &mut Parts) {
    match n {
        Node::Text { text, line } => p.0.push((*line, Part::Lit(text.clone()))),
        Node::Expr { php, line } => p.0.push((*line, dyn_child(php))),
        Node::Fragment { children, .. } => for c in children { lower(c, p) },
        Node::Element(e) if e.is_component => component(e, p),
        Node::Element(e) => html_element(e, p),
    }
}

/// Fold trivially-literal child expressions at compile time.
fn dyn_child(php: &str) -> Part {
    let t = php.trim();
    if let Some(s) = simple_single_quoted(t) { return Part::Lit(html_escape(&s)); }
    if t.parse::<i64>().is_ok() { return Part::Lit(t.to_string()); }
    Part::Dyn(format!("\\Lintel\\c({})", t))
}

fn html_element(e: &Element, p: &mut Parts) {
    let tag = e.name.as_str();
    p.0.push((e.line, Part::Lit(format!("<{tag}"))));
    for a in &e.attrs {
        match a {
            Attr::Str { name, value, line } => p.0.push((*line, Part::Lit(format!(" {name}=\"{}\"", value.replace('"', "&quot;"))))),
            Attr::Bool { name, line } => p.0.push((*line, Part::Lit(format!(" {name}")))),
            Attr::Spread { php, line } => p.0.push((*line, Part::Dyn(format!("\\Lintel\\attrs({})", php.trim())))),
            Attr::Expr { name, php, line } => {
                let t = php.trim();
                if let Some(s) = simple_single_quoted(t) { // folded literal
                    p.0.push((*line, Part::Lit(format!(" {name}=\"{}\"", html_escape(&s))))); continue;
                }
                let lname = name.to_ascii_lowercase();
                let f = if URL_ATTRS.contains(&lname.as_str()) { "u" } else if lname == "class" { "cls" } else if lname == "style" { "sty" } else { "a" };
                let call = if f == "cls" || f == "sty" { format!("\\Lintel\\{f}({t})") } else { format!("\\Lintel\\{f}('{}', {t})", name.replace('\'', "")) };
                p.0.push((*line, Part::Dyn(call)));
                // NOTE: `onclick={…}` etc. → diagnostic LNT0301 raised in the validate pass (opt.rs)
            }
        }
    }
    if e.self_closing || VOID.contains(&tag) {
        if VOID.contains(&tag) { p.0.push((e.line, Part::Lit(">".into()))); }
        else { p.0.push((e.line, Part::Lit(format!("></{tag}>")))); }
        return;
    }
    p.0.push((e.line, Part::Lit(">".into())));
    for c in &e.children { lower(c, p) }
    p.0.push((e.end_line, Part::Lit(format!("</{tag}>"))));
}

fn component(e: &Element, p: &mut Parts) {
    let mut args: Vec<String> = vec![];
    for a in &e.attrs {
        match a {
            Attr::Str { name, value, .. } => args.push(format!("{}: {}", camel_ok(name), php_str(value))),
            Attr::Bool { name, .. } => args.push(format!("{}: true", camel_ok(name))),
            Attr::Expr { name, php, .. } => args.push(format!("{}: {}", camel_ok(name), php.trim())),
            Attr::Spread { php, .. } => args.push(format!("...({})", php.trim())),
        }
    }
    if !e.children.is_empty() {
        let mut inner = Parts(vec![]);
        for c in &e.children { lower(c, &mut inner) }
        args.push(format!("children: new \\Lintel\\Html({})", concat(inner, e.line, e.end_line)));
    }
    p.0.push((e.line, Part::Dyn(format!("\\Lintel\\c({}({}))", e.name, args.join(", ")))));
}

/// Merge adjacent literals, interleave newlines so emitted line numbers match the source.
fn concat(parts: Parts, base: u32, total: u32) -> String {
    let mut merged: Vec<(u32, Part)> = vec![];
    for (line, part) in parts.0 {
        match (merged.last_mut(), part) {
            (Some((_, Part::Lit(a))), Part::Lit(b)) => a.push_str(&b),
            (_, part) => merged.push((line, part)),
        }
    }
    let mut out = String::new(); let mut cur = base; let mut first = true;
    if merged.is_empty() { out.push_str("''"); }
    for (line, part) in merged {
        if !first { out.push_str(" . "); }
        while cur < base.max(line) { out.push('\n'); cur += 1; }
        match part {
            Part::Lit(s) => { cur += s.matches('\n').count() as u32; out.push_str(&php_str(&s)); }
            Part::Dyn(d) => { cur += d.matches('\n').count() as u32; out.push_str(&d); }
        }
        first = false;
    }
    while cur < total { out.push('\n'); cur += 1; }
    out
}

fn php_str(s: &str) -> String { format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'")) }
fn html_escape(s: &str) -> String { s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#039;") }
fn camel_ok(n: &str) -> &str { n } // hyphenated names → LNT0202 in validate pass
fn simple_single_quoted(t: &str) -> Option<String> {
    let b = t.as_bytes();
    if b.len() >= 2 && b[0] == b'\'' && b[b.len() - 1] == b'\'' && !t[1..t.len() - 1].contains(['\\', '\'']) {
        Some(t[1..t.len() - 1].to_string())
    } else { None }
}
```

### 6.6 `lintel-diag` (shape)
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Code { UnterminatedTag, MismatchedClose, BadAttr, BadSpread, Expected,
    BuiltinShadow /*LNT0201*/, HyphenArg /*LNT0202*/, DynamicHandler /*LNT0301*/, VoidChildren /*LNT0401*/ }
pub struct Diagnostic { pub code: Code, pub message: String, pub offset: usize, pub help: Option<String> }
```
Render with `miette` in the CLI (source snippet, underline, `help:` line). Every diagnostic has a stable `LNTxxxx` code and a docs page `lintel.sh/e/LNTxxxx`.

### 6.7 `lintel-cli` (shape)
```rust
// lintel build [paths] [--target string|rfc|stream] [--out DIR|--in-place] [--map]
// lintel watch [paths]                  # notify + debounce, incremental
// lintel check [paths]                  # diagnostics only, exit code for CI
// lintel daemon --socket PATH           # length-prefixed JSON: {"path":…,"mtime":…} → {"php":…}|{"diag":[…]}
// lintel fmt | init | bench | migrate --to native
fn main() -> miette::Result<()> { /* clap derive; rayon::par_iter over files; blake3 content-hash cache */ }
```

### 6.8 PHP runtime — `php/runtime/src/functions.php` (the whole prod dependency)
```php
<?php declare(strict_types=1);
namespace Lintel;

final class Html implements \Stringable
{
    public function __construct(public readonly string $s) {}
    public function __toString(): string { return $this->s; }
    /** Greppable escape hatch. */
    public static function unsafeRaw(string $html): self { return new self($html); }
}

const FLAGS = ENT_QUOTES | ENT_SUBSTITUTE | ENT_HTML5;

function e(string $s): string { return \htmlspecialchars($s, FLAGS, 'UTF-8'); }

/** Render any child value to an HTML string. */
function c(mixed $v): string
{
    if ($v instanceof Html) return $v->s;
    if (\is_string($v))     return \htmlspecialchars($v, FLAGS, 'UTF-8');
    if ($v === null || \is_bool($v)) return '';
    if (\is_int($v) || \is_float($v)) return (string) $v;
    if (\is_iterable($v)) { $o = ''; foreach ($v as $x) { $o .= c($x); } return $o; }
    if ($v instanceof \Stringable) return \htmlspecialchars((string) $v, FLAGS, 'UTF-8');
    throw new \InvalidArgumentException('Cannot render ' . \get_debug_type($v) . ' as a child');
}

/** Generic attribute. */
function a(string $name, mixed $v): string
{
    if ($v === null || $v === false) return '';
    if ($v === true) return ' ' . $name;
    if (\is_array($v)) throw new \InvalidArgumentException("Attribute \"$name\" received an array");
    return ' ' . $name . '="' . \htmlspecialchars((string) $v, FLAGS, 'UTF-8') . '"';
}

/** URL attribute: scheme allowlist. Percent-encoding is NOT a defense against javascript:. */
function u(string $name, mixed $v): string
{
    if ($v === null || $v === false) return '';
    $s = (string) $v;
    $probe = \preg_replace('/[\x00-\x20\x7f]+/', '', $s) ?? '';   // browsers ignore ws/ctl inside schemes
    if (\preg_match('/^([a-z][a-z0-9+.\-]*):/i', $probe, $m)
        && !\in_array(\strtolower($m[1]), ['http', 'https', 'mailto', 'tel', 'sms'], true)) {
        $s = 'about:invalid#blocked';
    }
    return ' ' . $name . '="' . \htmlspecialchars($s, FLAGS, 'UTF-8') . '"';
}

/** class={…}: string | list | ['cls' => bool] */
function cls(mixed $v): string
{
    $out = [];
    $walk = static function (mixed $x) use (&$out, &$walk): void {
        if (\is_string($x) && $x !== '') { $out[] = $x; }
        elseif (\is_array($x)) {
            foreach ($x as $k => $val) {
                if (\is_int($k)) { $walk($val); }
                elseif ($val) { $out[] = $k; }
            }
        }
    };
    $walk($v);
    return $out === [] ? '' : ' class="' . \htmlspecialchars(\implode(' ', $out), FLAGS, 'UTF-8') . '"';
}

/** style={['prop' => 'value']} — values restricted to a safe charset. */
function sty(mixed $v): string
{
    if (!\is_array($v)) return a('style', $v);
    $o = [];
    foreach ($v as $k => $val) {
        if ($val === null || $val === false) continue;
        $val = (string) $val;
        if (!\preg_match('/^[\w\s#%.,()\-\/+*]*$/u', $val) || !\preg_match('/^[a-z\-]+$/i', (string) $k)) {
            throw new \InvalidArgumentException("Unsafe style value for \"$k\"");
        }
        $o[] = $k . ':' . $val;
    }
    return $o === [] ? '' : ' style="' . \htmlspecialchars(\implode(';', $o), FLAGS, 'UTF-8') . '"';
}

/** {...$attrs}: routes each entry through the right attribute function. */
function attrs(array $m): string
{
    $o = '';
    foreach ($m as $k => $v) {
        $k = (string) $k;
        if (!\preg_match('/^[a-zA-Z_:][\w:.\-]*$/', $k) || \str_starts_with(\strtolower($k), 'on')) {
            throw new \InvalidArgumentException("Unsafe attribute name \"$k\"");
        }
        $lk = \strtolower($k);
        $o .= match (true) {
            \in_array($lk, ['href', 'src', 'action', 'formaction', 'poster', 'cite', 'data'], true) => u($k, $v),
            $lk === 'class' => cls($v),
            $lk === 'style' => sty($v),
            default => a($k, $v),
        };
    }
    return $o;
}
```

### 6.9 Compile example (golden test seed)
Source:
```php
function Badge(string $label, bool $verified = false): Html {
    return (
        <span class="badge">
            {$label}
            {$verified && <i class="icon-check" />}
        </span>
    );
}
```
Expected output (line-preserving; note the nested JSX in the child expression was transpiled by the recursive scan):
```php
function Badge(string $label, bool $verified = false): Html {
    return (
        new \Lintel\Html('<span class="badge">' . \Lintel\c($label)
 . \Lintel\c($verified && new \Lintel\Html('<i class="icon-check"></i>'))
 . '</span>'
)
    );
}
```
(Source is 5 lines inside the parens; output keeps every following line number identical.)

### 6.10 Dev loader (PHP) — `php/lintel/src/Loader.php`
```php
<?php declare(strict_types=1);
namespace Lintel\Dev;

/** Dev-only. In prod, `lintel build` output is autoloaded by Composer as plain PHP. */
final class Loader
{
    private static ?string $sock = null;

    public static function boot(string $cacheDir, ?string $socket = null): void
    {
        self::$sock = $socket;
        // Composer classmap/PSR-4 can't find *.lintel.php; add a fallback autoloader.
        \spl_autoload_register(static function (string $class) use ($cacheDir): void {
            foreach (\Composer\Autoload\ClassLoader::class::getRegisteredLoaders() ?? [] as $l) {
                foreach ($l->getPrefixesPsr4() as $prefix => $dirs) {
                    if (!\str_starts_with($class, $prefix)) continue;
                    $rel = \str_replace('\\', '/', \substr($class, \strlen($prefix))) . '.lintel.php';
                    foreach ($dirs as $d) {
                        if (\is_file($f = $d . '/' . $rel)) { self::require($f, $cacheDir); return; }
                    }
                }
            }
        }, true, true);
    }

    /** Compile if stale (mtime+size key), then require the cached PHP. OPcache caches it like any file. */
    public static function require(string $file, string $cacheDir): mixed
    {
        $key = $cacheDir . '/' . \hash('xxh3', $file) . '-' . \filemtime($file) . '.php';
        if (!\is_file($key)) {
            $php = self::compile($file); // daemon over unix socket, else `lintel build --stdout`
            \file_put_contents($key . '.tmp', $php); \rename($key . '.tmp', $key);
        }
        return require $key;
    }

    private static function compile(string $file): string { /* socket client; fall back to proc_open */ }
}
```
*Known limitation to resolve in T-31:* function-only components are not autoloadable by PSR-4. `lintel.json` lists `functions` globs; the loader requires them eagerly in dev and `lintel build` emits a Composer `files` manifest for prod.

---

## 7. Security model (the part that earns trust)

### 7.1 Context-aware escaping matrix
| Context | Compiler emits | Rule |
|---|---|---|
| HTML text | `c()` / literal fold | `htmlspecialchars(ENT_QUOTES|SUBSTITUTE|HTML5)` |
| Quoted attr value | `a()` | same + bool/null semantics |
| URL attr | `u()` | strip ws/ctl for probing → allowlist `http,https,mailto,tel,sms` + relative; else `about:invalid#blocked` |
| `srcset` | `srcset()` (v0.2) | split candidates, validate each URL |
| `class` / `style` | `cls()` / `sty()` | list/map helpers; style values charset-restricted |
| `on*` handler, dynamic | **compile error LNT0301** | no JS-in-attribute from data |
| `<script>` data | JSON islands only | `JSON_HEX_*` flags |
| `<style>` data | not interpolated | CSS vars via `style={['--x' => …]}` |
| Raw | `Html::unsafeRaw()` | single greppable sink; Semgrep/PHPStan rule ships with Lintel |

### 7.2 Threat tests (must pass in CI; T-07)
`java\tscript:`, `JaVaScRiPt:`, `&#x6A;avascript:` (entity-encoded input is a *string value* so `&` is escaped → inert), `data:text/html`, `vbscript:`, attribute-breakout `" onmouseover=`, `</script>` in JSON, NUL bytes, invalid UTF-8 (`ENT_SUBSTITUTE`), spread with `onclick` key, `<`-in-attribute-name injection via spread keys.

### 7.3 Escape elision (honest version)
- **v0.1:** fold literals; direct `htmlspecialchars` when the expression is a typed parameter of the enclosing function (scanner records `string $x` from the signature); `(int)`/`(float)` casts and `count()` skip escaping.
- **Y2:** a `lintel analyze` pass using PHPStan's type info (via the PHPStan extension) provides exact types → elide escaping where the type is `int|float|bool|numeric-string|literal`. Benchmarks must report elision rate.

---

## 8. Developer experience (best-in-class checklist)

1. **Install:** `composer require --dev lintel/lintel` + `composer require lintel/runtime`. The installer plugin downloads the right binary (SHA-256 + signature verified; offline cache; `LINTEL_BIN` override). Prod needs only `lintel/runtime`.
2. **Zero-config dev:** `Lintel\Dev\Loader::boot()` — edit, refresh, done. No watcher required; `lintel watch` is optional for IDE/PHPStan freshness.
3. **Errors that teach** (miette style):
```
error[LNT0301]: dynamic event-handler attributes are not allowed
  ┌─ resources/Button.lintel.php:12:20
  │
12│   <button onclick={$handler}>
  │           ^^^^^^^^^^^^^^^^^ untrusted data would become executable JS
  = help: use `data-action={$name}` and attach behaviour in JS (or HTMX/Alpine)
  = docs: https://lintel.sh/e/LNT0301
```
4. **Line-preserving output** ⇒ Xdebug breakpoints, stack traces, Ignition/Symfony error pages, PHPStan results all point at the author's lines. Debug `--map` writes `.map` for column-exactness.
5. **Editor support:** TextMate grammar + VS Code extension (Y1 Q2), tree-sitter grammar (Y1 Q3), LSP (diagnostics, hover with prop types from the component signature, go-to-definition on `<Card>`, rename, completion for props/attributes/aria), JetBrains plugin (Y2).
6. **`lintel fmt`:** deterministic, opinionated (Prettier-like JSX layout; leaves PHP to PHP-CS-Fixer/Pint/Mago).
7. **HTML validity & a11y diagnostics** (the XHP promise, done at compile time): content-model checks (`<li>` outside list, `<a>` in `<a>`, `<p>` containing block), missing `alt`, `<button>` without type in forms, duplicate `id`, label/for linkage. Backed by generated tables in `lintel-html-spec`.
8. **Playground** (WASM): paste `.lintel.php`, see compiled PHP, diagnostics, and optionally run via php-wasm. Permalinks encode source in the URL hash.
9. **Hot reload:** dev middleware injects an SSE client; HTMX/morphdom-style DOM morph, state-preserving.
10. **Testing helpers:** `Lintel\Testing\render($html)->assertSee()`, DOM-snapshot assertions, component "stories".

---

## 9. Performance plan

### 9.1 Principles
- Hot path = literal concatenation + a handful of tiny functions. Components cost one `Html` allocation per call (not per element).
- Everything folds that can fold; measure instead of assuming (e.g., whether calling `\Lintel\c()` beats inline `is_string` branches under OPcache/JIT — **decide by benchmark**, the Zend engine's inlining behavior varies by version).

### 9.2 Benchmark harness (`/bench`, Docker, PHP 8.6 pinned)
| Scenario | Why |
|---|---|
| S1 static-heavy page (95% static) | measures folding |
| S2 1,000-row table w/ escaping | measures `c()`/`a()` overhead |
| S3 component depth 10 × fan-out 3 | measures `Html` allocation |
| S4 attribute-heavy (`class`, `href`, spread) | measures helper cost |
| S5 streaming TTFB with 3 slow Suspense boundaries | measures §10 |

Competitors: hand-written `echo`/`ob_start` (the floor), Blade (compiled), Twig (compiled), Plates, `--target=rfc` (RFC-style lowering), Lintel string target.
Settings matrix: OPcache on/off, JIT off/tracing, PHP 8.6.x pinned by digest. Metrics: ns/render (median, p95, 30 runs), peak memory, opcode count, compile throughput (files/s, cold & warm). Output: JSON + auto-generated Markdown + SVG charts committed to `bench/results/`.
**Publishing rule:** no number appears in README/launch posts unless `make bench` reproduces it on CI hardware and the methodology page links the raw JSON.

### 9.3 Compiler performance targets (hypotheses to *validate*, not claims)
- Single typical component file compile: tens of µs; 500-file cold build: well under 100 ms with `rayon`; warm (cache hit): I/O-bound.
- Daemon round-trip vs `proc_open` spawn: measure both; only ship the daemon if the win is material.
- Allocation strategy: start with plain `String`; adopt `bumpalo` only if profiling shows allocator pressure (premature otherwise).

---

## 10. Streaming & Suspense (v0.3 → v1.0)

**Model.** `--target=stream` compiles components to Generators yielding string chunks. A `<Suspense fallback={…}>` boundary:
1. Emits fallback immediately inside `<div id="lt-1">…</div>` and flushes.
2. Continues rendering siblings; resolves the boundary's async work (Fiber-based `await`, or adapter-provided promises).
3. Streams `<template id="lt-1-r">…</template><script>/*swap*/</script>` (out-of-order, like React's protocol), ≈250 bytes of one-time runtime script.
**Runtimes.** FPM (flush + `X-Accel-Buffering: no`), FrankenPHP, RoadRunner, Swoole/OpenSwoole, ReactPHP/AMPHP via adapters. **`Io\Poll`** (PHP 8.6) is used *inside* the ReactPHP/Amp adapter layers where those libraries adopt it — not by Lintel directly.
**Compat:** the same source compiles to sync or stream; sync target runs Suspense boundaries inline (no fallback), so a page works everywhere.

---

## 11. Ecosystem & integrations
- **Laravel:** `Route::get('/', fn() => <Home />)`, Blade interop (`<x-…>` ↔ Lintel components both directions), Vite dev-tag helpers, Livewire/Inertia coexistence notes, `php artisan lintel:*` wrappers.
- **Symfony:** Twig interop, `#[AsLintelComponent]`, Profiler panel (render tree/time).
- **PSR-15 middleware**, **PSR-7 response renderer**, **Slim/Mezzio** examples.
- **HTMX/Alpine/Datastar:** attribute-name allowlist handles `hx-*`, `@click`, `x-data`, `data-on-*` (static strings OK; dynamic only through `a()` with the `on*` rule applied to real handlers, not to `hx-*`).
- **Email target** (Y2): table-based layout components + CSS inliner; **Markdown components** (`<Markdown>`), **SVG** with `viewBox`/case-sensitivity rules.
- **Design system:** `lintel/ui` headless, accessible components (Dialog, Tabs, Combobox) — doubles as the best demo of the language.

---

## 12. Distribution
1. **Binary:** GitHub Releases (linux x64/arm64 musl, macOS x64/arm64, Windows x64), reproducible builds, `cosign` signatures, SBOM.
2. **Composer plugin** downloads + verifies + caches in `vendor/bin/lintel`. Alternatives for locked-down CI: `curl | sh` installer, Docker image, Homebrew tap, `npx lintel`, `cargo install lintel-cli`.
3. **Supply-chain stance** (since a binary arrives via Composer): pinned versions, checksum file in `composer.json` `extra`, no network at runtime, documented trust model.
4. **Versioning:** SemVer; the **language spec** versions separately (`lintel-lang 1.0`); the compiler states the spec version it implements.

---

## 13. Quality system (how an agent stays correct)
1. **Golden tests** `tests/golden/NAME.lintel.php` → `NAME.expected.php`; `cargo insta`-style review workflow. Cover: whitespace rules (≥40 cases), nested JSX in expressions, heredoc/nowdoc/strings with `<`, `$a < $b`, generics-like `<` in comments, `?>` handling, attribute forms, void elements, fragments, spread, pipes+PFA inside `{}`.
2. **Differential tests:** compile → `php -l` → execute → compare stdout with a hand-written reference PHP renderer, for ≥200 generated templates (property-based: random trees → render via a reference model).
3. **Fuzzing:** `cargo-fuzz` on `compile()`; invariant: never panics, never loops, output of valid input passes `php -l`.
4. **Line-preservation invariant test:** for each golden, the line number of every non-JSX token is identical in input and output.
5. **Security corpus** (§7.2) as executable tests.
6. **PHP matrix:** 8.6 (primary), 8.5 (runtime only needs no 8.6 features unless PFA used in source).
7. **CI gates:** `cargo test`, `clippy -D warnings`, `cargo deny`, `fuzz smoke 60s`, PHPUnit, PHPStan max, bench regression (>10% = fail), docs link check.

---

## 14. Docs & community
`lintel.sh`: Quick start (5 min), Language spec, Cookbook (forms, tables, layouts, HTMX), Security model, Migration guides (Blade/Twig → Lintel; Lintel → native RFC), Error index (`/e/LNTxxxx`), Benchmarks page (methodology + raw data), RFC-alignment tracker. Discord/GitHub Discussions; "Good first issue" labels seeded by the agent.

---

## 15. MVP — 14-day execution plan (task cards)

**MVP definition ("viral-ready"):** compiler core + CLI + PHP runtime + dev loader + playground + reproducible benchmark + 5 killer demos + docs site v0. Nothing else ships before launch.

> Each card = **Goal / Deliverable / Acceptance**. Agent: complete in order; open one PR per card; don't start a card until its predecessors' acceptance tests are green.

### Days 0–2 — Foundations
- **T-00 Reality check.** Verify name availability (Packagist, GitHub, crates.io, npm, domains). Re-read the RFC + php-src PR #22661; write `docs/rfc-alignment.md`. *Accept:* doc lists exact class names/lowering for elements, fragments, components.
- **T-01 Workspace + CI.** §5.1 layout, §6.1 Cargo, GitHub Actions (cargo test/clippy/fmt/deny, PHP 8.6 container). *Accept:* green CI on empty tests.
- **T-02 Diagnostics crate.** §6.6 + miette renderer + code registry file `docs/errors/LNT*.md`. *Accept:* unit snapshot of rendered error.
- **T-03 Scanner (PHP-aware).** §6.3 non-JSX paths: strings, heredoc/nowdoc, comments, attributes `#[`, inline HTML, `?>`. *Accept:* 60 golden pass-through cases; output **byte-identical** to input when no JSX.
- **T-04 Text normalization.** §6.4. *Accept:* 40+ whitespace cases checked against React's JSX transform behavior (record expected outputs from Babel once, commit as fixtures).

### Days 3–6 — Compiler
- **T-05 JSX parser + emit (string target).** §6.2–6.5: elements, fragments, attrs, children, expressions, nested JSX in expressions, components with named args, raw-text elements. *Accept:* golden suite; every output passes `php -l`.
- **T-06 Line preservation.** *Accept:* invariant test (§13.4) green on all goldens.
- **T-07 Runtime + security corpus.** §6.8, §7.2. *Accept:* PHPUnit suite incl. all threat vectors; PHPStan max clean.
- **T-08 Validation pass.** LNT0201 (builtin shadow), LNT0202 (hyphenated component prop), LNT0301 (dynamic `on*`), void-with-children, mismatched tags with "did you mean". *Accept:* each has a failing-fixture test and docs page.
- **T-09 Differential + fuzz.** §13.2–13.3. *Accept:* 200 generated templates equal reference; 10 min fuzz clean.

### Days 7–9 — DX
- **T-10 CLI.** `build/check/watch/init`; rayon parallelism; blake3 cache; `--in-place` and `--out`. *Accept:* `lintel init` yields a runnable demo in one command.
- **T-11 Composer packages + installer plugin + dev Loader.** §6.10, §12. *Accept:* fresh Laravel and plain-PHP skeleton: `composer require` → edit → refresh works with no watcher.
- **T-12 Daemon (optional gate).** Implement only if T-14 shows spawn overhead matters. *Accept:* measured p50/p99 vs spawn.
- **T-13 WASM + Playground.** `wasm-pack`, static site, permalinks, side-by-side output, diagnostics panel, "copy compiled", examples dropdown. *Accept:* loads <1 MB gz; works offline after first load.

### Days 10–12 — Proof
- **T-14 Benchmark harness.** §9. *Accept:* `make bench` reproduces; JSON + charts committed; methodology doc.
- **T-15 Five demos** (each ≤ 60 lines, screenshot-worthy): (1) component library page, (2) XSS-attack gallery showing every payload neutralized, (3) 10k-row table, (4) HTMX todo, (5) compile-error showcase. *Accept:* each runs from `lintel init --demo N`.
- **T-16 VS Code extension (grammar only) + `lintel check` problem-matcher.** *Accept:* JSX highlighted inside `.lintel.php`.

### Days 13–14 — Launch hardening
- **T-17 Docs site v0** (quick start, spec, security, benchmarks, errors).
- **T-18 Release engineering:** signed binaries, Composer tags, `v0.1.0`, changelog, SECURITY.md, CODE_OF_CONDUCT, license.
- **T-19 Launch assets:** 20-second terminal GIF (edit → refresh → compiled view), OG image, 3 post variants (§16), FAQ ("vs RFC?", "vs Blade?", "is the binary safe?").
- **T-20 Dry run:** fresh-machine install test on Linux/macOS/Windows; 10-minute "new developer" script.

---

## 16. Launch playbook (dev-null.sh + beyond)

**Reality constraints:** a commit is **3–280 characters**, plain text; post via signed CLI commit or the web. So: one hook, one link.

**Post templates** (agent: fill `{N}` only from `bench/results`; verify ≤280 chars with `wc -m`):
1. `Lintel: JSX for PHP 8.6. <Card user={$u}/> compiles to plain string concat — auto-escaped, no VDOM, line numbers preserved. Rust compiler, WASM playground → lintel.sh` 
2. `Wrote a JSX compiler for PHP. Output is readable PHP, XSS-safe by default ({N} attack payloads in the demo, 0 get through). Try it in the browser: lintel.sh/play`
3. `Native JSX-in-PHP is an RFC for 8.7/9. Lintel lets you write it today and run \`lintel migrate --to=native\` when it lands. lintel.sh`

**Sequence**
1. **T-13 playground first** — the link is the product.
2. **Day 14 (launch):** dev-null post #1 + thread-style follow-up commits (benchmarks, security demo, error messages). Also: r/PHP, Hacker News ("Show HN: Lintel — JSX for PHP, compiled to string concat"), Laravel/Symfony communities, PHP Weekly, Laravel News tip.
3. **Engage the RFC discussion respectfully:** share Lintel as a userland implementation for experience reports (valuable data point for the RFC authors), **don't** position as a competitor.
4. **Week 2:** publish a technical write-up ("How a JSX compiler stays line-accurate"), the benchmark methodology post, and a "Blade → Lintel" migration guide.
5. **Credibility rule:** answer every skeptical comment with a reproducible link, never adjectives.

---

## 17. Risks & mitigations
| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Native RFC lands and obsoletes userland | Med (8.7/9.0, still under discussion) | High | RFC alignment table; `migrate --to=native`; differentiate on tooling (LSP, fmt, validation, streaming, adapters) which core won't ship. |
| Internals/community reads it as "competing" | Med | Med | Cooperative framing, contribute test cases/experience to the RFC. |
| Binary-via-Composer distrust | High | Med | Signed releases, pinned checksums, prod needs no binary, doc'd trust model, `cargo install` path. |
| Perf claims fail under scrutiny | Med | High | §9.2 publishing rule; reproducible CI benchmarks; report where we lose. |
| Scanner misparses exotic PHP | Med | High | Verbatim pass-through default; fuzzing; differential tests; `// lintel-ignore-next` escape; conservative JSX-start heuristic. |
| Function components can't be PSR-4 autoloaded | High | Med | `lintel.json` manifest + generated `files` autoload (T-31). |
| `<Header>` vs `header()` style collisions | High | Med | LNT0201 + docs; encourage namespaced components. |
| Eager PFA args surprise users (`?` evaluates other args immediately) | Med | Low | Docs callout from the PHP 8.6 coverage; lint for side-effecting args in PFA. |
| Single-maintainer/agent bus factor | High | High | AGENTS.md, ADRs, exhaustive goldens, small core crate. |

---

## 17.1 Multi-year roadmap

### Year 1 — "Make it real" (v0.1 → v1.0)
| Quarter | Release | Exit criteria |
|---|---|---|
| **Q0 (weeks 1–2)** | **v0.1 MVP + launch** | §15 complete; playground live; benchmarks published |
| **Q1** | v0.2 | Component **signature scanning** (typed-prop checks at compile time, `children` validation, builtin shadow resolution), `srcset`, JSON islands, class components (`use`-resolved), `--target=rfc`; VS Code extension w/ diagnostics; **PHPStan extension** (analyzes compiled output, maps errors back via line-preservation) |
| **Q2** | v0.3 | **Streaming + Suspense** (§10), FrankenPHP/RoadRunner adapters; **Laravel & Symfony packages**; incremental cache; `lintel fmt` alpha |
| **Q3** | v0.5 | **LSP** (diagnostics, hover, definition, rename, completion); tree-sitter grammar; HTML-validity + a11y lints; hot-reload middleware; testing helpers |
| **Q4** | **v1.0** | Language spec 1.0 frozen; stability promise; 3 production case studies; benchmark suite adopted in CI by ≥2 external projects; docs complete |

### Year 2 — "Own the DX"
- **JetBrains plugin** (PhpStorm: parsing via `.lintel.php` language injection → full PSI later), formatter GA, import organizer for components.
- **Type-driven escape elision** via PHPStan type info (§7.3) + published elision-rate stats.
- **`lintel/ui`** accessible headless component library + docs/storybook ("Lintel Studio": live component explorer rendered by Lintel itself).
- **Email target**, **Markdown components**, **i18n extraction** (`<T>` + ICU), **asset pipeline hooks** (Vite/esbuild manifests), **CSP nonce propagation** (auto `nonce` on inline scripts/styles).
- **Island conventions** (HTMX/Datastar/Alpine presets), partial-render endpoints (`<Fragment id>` addressable renders).
- **Query-based incremental compiler** (Salsa-style) for LSP latency; persistent daemon GA.
- **Blade/Twig → Lintel codemods.**

### Year 3 — "Convergence & scale"
- **RFC convergence:** if the RFC is accepted → `lintel migrate --to=native` GA, lint rules to keep code portable, dual-mode compilation in CI; if rejected → v2 with deeper language features (typed slots, scoped context via PFA-friendly providers, compile-time HTML contracts).
- **Observability:** render-tree profiler, per-component timing in Symfony Profiler / Laravel Telescope / OpenTelemetry spans.
- **Edge/portable:** compile pre-rendered static output (SSG mode), PHP-WASM runtime demos, serverless adapters (Bref, Vercel PHP).
- **Security hardening:** formal escaping-contract doc, third-party audit, bug bounty, Trusted Types-style policy mode for `Html::unsafeRaw`.
- **Governance:** move to org, steering group, spec RFC process (Lintel Evolution Proposals), maintainers beyond the founder.

---

## 18. `AGENTS.md` (copy into repo root)

```markdown
# Operating manual for AI agents working on Lintel

## Prime directives
1. Never change behavior without a failing test first.
2. Non-JSX PHP must pass through byte-identical. This is sacred.
3. Output line numbers must equal input line numbers (invariant test).
4. No performance claim without a reproducible benchmark in /bench.
5. Do not invent facts about PHP 8.6 / the RFC: re-read sources, cite them in docs/.

## Workflow per task card
Read card → write failing tests → implement smallest change → `make ci` locally →
update docs + error index → open PR with: What / Why / Tests / Benchmarks (if perf-relevant).

## Commands
make ci        # fmt, clippy -D warnings, test, php lint, phpunit, phpstan
make golden    # review/update snapshots (cargo insta review)
make fuzz      # 60s smoke
make bench     # Docker, pinned PHP 8.6 digest; writes bench/results/

## Style
Rust: no unsafe, no unwrap in lib code, thiserror for errors, small functions.
PHP: declare(strict_types=1), PHPStan level max, no dependencies in lintel/runtime.
Diagnostics: stable LNT codes, always include a `help:` line.

## When stuck
Write an ADR in docs/adr/ describing options + trade-offs; choose the simplest that keeps the invariants.
```

---

## 19. Open decisions for the human (answer these; defaults in bold)
1. **Name:** Lintel (verify availability in T-00) — or pick from the fallback list.
2. **Components:** functions-only in v0.1 (**yes**) vs. classes at launch.
3. **Daemon at launch:** only if T-14 shows spawn cost matters (**defer**).
4. **Licensing:** **MIT OR Apache-2.0** (PHP ecosystem norm is MIT).
5. **Launch timing:** ship v0.1 within the PHP 8.6 news cycle (**target ≈ Dec 3, 2026**) vs. wait for LSP (not recommended — the playground + benchmarks + security demo carry the launch).
6. **Claims policy:** publish only measured benchmarks (**yes — non-negotiable**).

---

## 20. Source list
- PHP 8.6 release announcement — https://www.php.net/releases/8.6/en.php
- What's new in PHP 8.6 (Laravel News) — https://laravel-news.com/php-8-6
- What's new in PHP 8.6 (stitcher.io, 2026-07-29) — https://stitcher.io/feed/posts/75164
- PHP RFC: Native Markup Expressions — https://wiki.php.net/rfc/native_markup_expressions
- Internals discussion — https://externals.io/message/131931 · https://externals.io/message/132172
- Implementation PR — https://github.com/php/php-src/pull/22661
- dev-null.sh build story — https://laravel-news.com/social-network-php-48-hours · https://runtimewire.com/article/dev-null-48-hour-php-social-network
- dev-null.sh commit constraints (3–280 chars, SSH signing) — https://packagist.org/packages/gromnan/devnull-notifier
- Prior art — https://packagist.org/packages/syntaxx/phpx-compiler · https://packagist.org/packages/pre/phpx · https://packagist.org/packages/phpmystic/liqx · https://packagist.org/packages/lmc/twigx-bundle
- Your three research documents (XHP/PHPX history, viral-DX brainstorm, Rust architecture) — incorporated, with corrections in §0.2.

---

## 21. Implementation status (agent-maintained; drift fixes live here)

> Updated 2026-11-20 after the first execution pass, again after the
> second pass (stream target + signature scanning), again after the
> third pass (ignore directives + required-parameter signature checks), and
> again after the fourth pass (validation-rule seam, `SourceMap` positions,
> "did you mean" close-tag suggestions), again after the fifth pass
> (file-level `lintel-ignore-file` suppression, `srcset` attribute routing,
> parse-error `help:` lines), again after the sixth pass (§7.3 escape
> elision for provably-scalar children, the minimal `lintel-cli` build/check
> binary), again after the seventh pass (machine-readable JSON
> diagnostics with the CLI `--format=json` flag, and the read-only
> `NodeVisitor` traversal seam), and again after the eighth pass (the
> §7.3 typed-parameter direct escape: the scanner tracks the enclosing
> function scope and folds `string`-typed parameter children to a direct
> `\htmlspecialchars(...)` call, and `lintel init` scaffolds a runnable
> demo), and again after the ninth pass (the component-inventory
> shape-report seam — `collect_inventory` over the declared-component
> table and the invocation walk, `docs/component-inventory.md` — and the
> `lintel stats` CLI subcommand that prints it, text or `--format=json`),
> and again after the tenth pass (the self-describing rule registry:
> `ValidationRule::meta` + `RuleMeta`, `docs/rule-registry.md`, and the
> `lintel rules` CLI subcommand), and again after the eleventh pass (the
> cross-file signature merge: `ScanContext::merged_with`,
> `docs/cross-file-signatures.md`), and again after the twelfth pass (the
> §6.8 PHP runtime transcription, the §7.2 PHPUnit threat corpus, PHPStan
> level max, and the `php -l` gates that close the never-executable T-05
> and T-10 acceptance halves — Rust integration tests wired through
> `PHP_BIN` plus a new CI `php` job), and again after the thirteenth pass
> (task-cards T-11, T-14 and T-15: the `php/lintel` composer package —
> installer plugin + no-watcher dev loader — the §9.2 `bench/` harness with
> committed JSON/Markdown/SVG results, the §18 Makefile, cargo-deny (T-01),
> and the five T-15 demos behind `lintel init --demo N`), and again after
> the seventeenth pass (the §10 streaming & suspense runtime — `Lintel\stream`,
> the `<Suspense>` boundary with out-of-order flushing and the in-place
> swap script, the §7.2 streaming threat cases, the `Lintel\FlushTransport`
> seam with the CLI and FPM lanes exercised over `php -S`, and the
> zero-warning repair of the pedantic-clippy gate the prior pass had left
> regressed in the newest CLI code), and again after the Phase 8 pass
> (the §17.1-Q3 testing helpers — `Lintel\Test\ComponentTester` —, the
> §17.1-Q3 advanced LSP surface — `textDocument/rename`,
> `textDocument/references`, `workspace/symbol` over the open workspace —,
> the Year-3 real `lintel migrate` — in-place RFC rewrite with
> `--dry-run`/`--backup` and migrated-set component resolution —, and the
> gate repair of the Phase 7 bool-elision fixture's report-resolution
> assertion: the fixture had typed the hostile prop `string`, which wins
> the §7.3 ladder before the report is consulted, so the mis-proof case
> could never fire — the fixture's prop is untyped now and the suite is
> green with no skips), and again after the Phase 9 pass (scale &
> production hardening: the persistent LSP workspace index — closed
> files renamable, referenceable, symbol-listed and definition-resolvable
> through a mtime+size-validated on-disk declaration cache —, the
> content-addressed build cache — `.lintel-cache/`, content+target
> SHA-256 keys, `--no-cache` bypass, the §9.3 warm-build bench —, the
> bare `--manifest` cwd-hygiene fix — the default anchors to the
> workspace being compiled, never the process cwd —, and
> `ComponentTester::renderStreaming()` — whose parity acceptance proved
> the stream artifact's `: Html` return type TypeErrored against its own
> generator body, now rewritten to `\Generator` on the same line), and
> again after the Phase 10 pass (production deployment & real-world
> readiness: the T-12 daemon hardened — a length-prefixed JSON protocol
> both directions (`docs/daemon-protocol.md`) with bounded client
> reconnect, the PHP loader's socket lane on the same framing, the
> recompiles running through the content-addressed `.lintel-cache/`
> build cache with the counters in `stats.json`, the shared mutable cache
> files flocked, and the background spawn's readiness probe no longer
> sending the `quit` frame that stopped the daemon it just verified —,
> the anonymous closure rewrite — braced `function (): Html` closures
> rewrite to `\Generator` under the stream target like named
> declarations, body-less arrow functions copy verbatim —, the monorepo
> `lsp.include` config — `lintel.json` glob patterns the LSP walk honors
> over the default vendored/hidden skip —, and the production deployment
> guides (`docs/deployment/` — FPM + nginx, RoadRunner, Swoole,
> FrankenPHP — with the CI `deployment` job running the RoadRunner guide
> end-to-end through a real `rr serve` worker pool)), and again after the
> Phase 11 pass (advanced LSP parsing, streaming timing & cross-platform
> completeness: the tree-sitter LSP lane — §17.1 Q3's "tree-sitter
> grammar" —, the RoadRunner progressive per-boundary timing lane
> closing the §10 carried note, and the `windows-daemon` CI job closing
> T-12's carried Windows-socket deferral).
> This section reconciles the plan above with what the repository actually
> contains, so readers do not treat §5–§6 as an inventory of shipped code.

### 21.1 What exists

| Plan item | Status | Where |
|---|---|---|
| T-01 workspace + CI | **done (subset)** | `Cargo.toml`, `.github/workflows/ci.yml` — members `lintel-diag`, `lintel-core` and (since the sixth pass) `lintel-cli`; `lintel-wasm`/`lintel-lsp`/`lintel-fmt`/`lintel-html-spec` are deferred (§6.1 lists four members, §5.1 lists seven: this repo now defines the truth per §21.1). CI clippy gate uses the renamed `-W clippy::pedantic` lint: the bare `-W pedantic` name is a `renamed-and-removed-lints` hard error on current stable toolchains |
| T-02 diagnostics crate | **done (with severities + JSON)** | `crates/lintel-diag` — `Code` (LNT0101–LNT0401 plus `LNT0205`), `Severity` (`Error`/`Warning`), `Diagnostic` (implements `std::error::Error` for CLI/LSP consumers), dependency-free miette-style `render()` and, since the seventh pass, dependency-free `to_json()` (one-line JSON object, renderer-parity positions, stable field order — `docs/diagnostics-json.md`); `docs/errors/LNT*.md` pages not yet written |
| T-03 PHP-aware scanner | **done** | `crates/lintel-core/src/scan.rs` — byte-identical pass-through is golden-tested; lowering is injected via `scan_with(src, &dyn Lowering)` instead of being hardcoded |
| T-04 text normalization | **done** | `crates/lintel-core/src/text.rs` + `tests/whitespace.rs` |
| T-05 parse + emit (string target) | **done** | `crates/lintel-core/src/{ast,scan,emit}.rs`, `compile()` contract in `lib.rs`; the `php -l` acceptance half is now executable and green: `tests/php_gate.rs` lints every golden output on all three targets through `PHP_BIN` (twelfth pass) |
| T-08 validation pass | **done (v0.1 subset + warnings + signatures)** | `crates/lintel-core/src/validate.rs` — LNT0201/0202/0203/0204/0205/0301/0302/0401; error-severity diagnostics fail `compile()`, warnings (static `on*` strings, LNT0302) ride along on `Output::warnings`. "Did you mean" close-tag suggestions are done: a mismatched close that shares the open tag's first character within a small edit distance carries a `help:` suggestion (`scan.rs::close_tag_help`, tested by `tests/parse_errors.rs`); the per-case docs pages the acceptance names are written — `docs/errors/LNT0102.md`, `LNT0201.md`, `LNT0202.md`, `LNT0301.md`, `LNT0401.md` (twelfth pass) |
| Validation-rule seam (T-08 extension) | **done** | `validate::ValidationRule` trait + eight built-in rule structs + `builtin_rules()` registry in `crates/lintel-core/src/validate.rs`; `validate_with(roots, context, rules)` and `compile_with(src, opts, rules)` inject a consumer registry the way `scan_with` injects a lowering. Rules run in slice order over the shared depth-first walk, then the `lintel-ignore-next` filter applies — custom rules inherit the severity split and suppression choke point. Tested by `tests/rules.rs`, demoed by `examples/custom_rule.rs` |
| `SourceMap` offset→position seam | **done** | `lintel_diag::SourceMap` (re-exported at the `lintel-core` root): byte offsets → 1-based line / char-based column / line text with clamp-and-floor semantics, byte-identical to what `Diagnostic::render` has always computed (which now delegates to it). The converter CLI/LSP consumers previously had to re-implement. Tested by `tests/sourcemap.rs` + inline `lintel-diag` tests, demoed by `examples/sourcemap.rs` |
| `// lintel-ignore-next` + `// lintel-ignore-file` (doc §17 mitigation) | **done (compiler subset)** | the scanner records one-line suppression windows into `ScanContext` (`scan::Suppression`, `add_suppression`/`suppressed_at`/`suppressions()`), and the file-level `lintel-ignore-file` directive latches `ScanContext::suppress_file`/`file_suppressed` so `validate` drops every validation diagnostic and short-circuits the rule walk; parse diagnostics (`LNT01xx`) are never suppressible. The exact contracts are recorded in `docs/ignore-directives.md` and tested by `tests/suppressions.rs` |
| `--target=rfc` (doc 5.3, §4.9) | **done** | `Target::Rfc` + the public `Lowering` trait in `crates/lintel-core/src/emit.rs`; the exact lowering contract is recorded in `docs/rfc-alignment.md` and tested by `tests/rfc_target.rs` — the `Lowering` seam itself (consumer-injected strategies through `scan_with`) is demoed by `examples/custom_lowering.rs` |
| `--target=stream` (doc 5.3, §10) | **done (compiler subset)** | `Target::Stream` + `StreamLowering` in `crates/lintel-core/src/emit.rs`; the exact generator-closure contract is recorded in `docs/stream-alignment.md` and tested by `tests/stream_target.rs`. §10's Suspense protocol ships on the runtime side (the row below); the compiler itself still emits plain generators with no boundary scaffolding |
| §10 streaming & suspense runtime (Q2 v0.3, seventeenth pass) | **done (runtime + CLI/FPM transports; live-server adapters CI-lane)** | `php/runtime/src/functions.php` — `Suspense` (the boundary value), `SuspenseFlusher` (`@internal`, the §10 flush loop: placeholder `<div id="lt-N">` flushed immediately, the work in a fiber pumped round-robin, the resolved content inside `<template id="lt-N-r">…</template><script>lintelSwap("lt-N")</script>` **out of order**, the one-time 184-byte swap runtime, resolved generators walking as markup streams, every other resolved value through `c()`), `stream($tree, ?callable $flush)` as the entry, `FlushTransport` + `CliFlushTransport` + `FpmFlushTransport` + `stream_via`, and the sync-compat branches in `c()` (a boundary consumed outside the flush loop runs inline). The exact contract is recorded in `docs/stream-alignment.md` ("The §10 flush runtime"); tests: `php/runtime/tests/StreamingSuspenseTest.php` (flush-order e2e, out-of-order resolution, once-only swap script, nested boundaries, sync compat), four streaming threat cases in `php/runtime/tests/ThreatCorpusTest.php` (late-flushed chunk, fallback, resolved subtree, generator-child trust boundary — the corpus is now 31 tests / 116+ assertions with the transport suite), `php/runtime/tests/StreamingTransportTest.php` (the seam lifecycle, the CLI lane, the FPM lane over a real `php -S` HTTP server: `X-Accel-Buffering: no` header, placeholder-before-template, escaped late payload), and the runnable demo `scripts/suspense-demo.php`. Live RoadRunner/Swoole/OpenSwoole/FrankenPHP adapters stay §17.1-CI work (no such server in the sandbox) |
| §17.1-Q2 server adapters (eighteenth pass) | **done (four adapters + the live RR lane)** | `php/runtime/src/transports.php` — `SwooleFlushTransport` (status + headers on `begin()`, one `write()` per chunk, `end()`; raw `Swoole\Http\Response` objects adapted structurally by `fromSwoole()` — ext-swoole ships no autoloadable interface, so the lane defines the narrow `SwooleResponse` contract and validates the raw surface up front), `ReactPHPFlushTransport` (`writeHead()` + per-chunk `write()` + `end()`, `fromReact()`), `FrankenPHPFlushTransport` (the echo + flush shape, `FRANKENPHP_WORKER`/`SERVER_SOFTWARE` detection) and `RoadRunnerFlushTransport` (the RR PHP worker is pull-based: `stream_via()` queues the flush-ordered chunks, `respond()` hands them to `HttpWorkerInterface::respond(status, generator, headers)` — one midflight stream chunk per queued chunk, HeadersList shape, `RR_MODE=http` detection). Each adapter only owns HOW a chunk reaches the wire — chunk content untouched, the escaper contract holds. Tests: `php/runtime/tests/AdapterTransportTest.php` (9 tests: structural lanes against fakes in `tests/Support/Fake*`, the FrankenPHP lane over a real `php -S` server, the RoadRunner lane **end-to-end against a real `rr serve` 2025.1 application server** — placeholder before template, escaped late payload, swap script as its own stream chunk; env-gated by `LINTEL_E2E_RR=1`, which the new php-job `adapters` CI step sets after installing rr; the skip lane names that step). The deferred adapters gap from the seventeenth pass's M2 is closed. `phpstan.neon` boots the packages' autoloaders (`bootstrapFiles`) — the repo root has no composer.json, so PHPStan could not see `Spiral\RoadRunner\*` before |
| §17.1 Year-3 RFC convergence — the runtime side (eighteenth pass) | **done (php/markup)** | `php/markup` (`markup/markup`) — `\Markup\Element` / `\Markup\Fragment` / `\Markup\Html` + `\Markup\render()`: the runtime the rfc lowering's output needed (`docs/rfc-alignment.md` "the runtime concern outside the compiler's output shape"). Escape-by-default is the runtime's contract: plain string children and scalar attribute values escape at print time, `\Markup\Html::unsafeRaw()` is the only trusted sink, attributes route by name under the same rules the string target's emitted helpers enforce (§4.4 URL_ATTRS, §7.1 srcset, `on*`/name-shape rejections), void elements render themselves and reject children. **Parity is executable**: `php/markup/tests/MarkupE2eTest.php` compiles ONE fixture with `--target=rfc` and the string target through the real binary and asserts `\Markup\render(...)` produces the same bytes `\Lintel\c(...)` does (hostile payload escaped, hostile URL blocked in both lanes); `MarkupRuntimeTest.php` pins the per-rule contract (17 tests / 42 assertions in the package). The package is standalone (no Lintel dependency — the zero-lock-in `lintel migrate --to=native` pitch); the php job installs/validates it and `scripts/php-gates.sh` runs its suite + PHPStan over it |
| §17.1-Q2 Laravel & Symfony packages — the interop cores (eighteenth pass) | **done (Blade + Twig bridges)** | `php/blade` (`lintel/blade`) — the `@lintel('Badge', ['name' => $user])` directive registered on a real `Illuminate\View\Compilers\BladeCompiler`, compiling to an echo of `Lintel\Blade\Directive::render(...)`; `php/twig` (`lintel/twig`) — the `lintel_render('Badge', {name: user})` Twig function (`is_safe: ['all']`: the runtime escapers own the contract, Twig must not double-escape). Both reuse the §6.10 dev loader (`Lintel\Dev\Loader::require`, compile-if-stale) for `.lintel.php` sources — prod builds autoload instead — spread arguments as PHP named arguments, and render through `c()`, so a framework-embedded component is not an escape bypass (the Twig lane renders a hostile-output component escaped). Tests: `php/blade/tests/BladeBridgeTest.php` + `php/twig/tests/TwigBridgeTest.php` (directive compile + eval render, loader cache reuse, hostile payloads, unknown-component throws). §17.1-Q2's remaining Laravel surface (`artisan lintel:*` wrappers, Vite helpers, Livewire notes) and Symfony's (`#[AsLintelComponent]`, Profiler panel) stay framework-lane work |
| §17.1 Q1 component signature scanning | **done (v0.2 subset)** | `scan::Signature` + `ScanContext::declare_signature/signature_of` in `crates/lintel-core/src/scan.rs`, consulted by `validate.rs`: `LNT0203` unknown prop, `LNT0204` unexpected children and `LNT0205` missing required parameter (a parameter declared without a default; spreads satisfy it, JSX children satisfy a `children` parameter) for components declared in the same file (PHP named arguments are case-sensitive); the signature table keys keep the author's spelling since the ninth pass (`declared_signatures`), and the typed-parameter half feeds the §7.3 elision below — *drift fix 2026-11-19: this row previously said "typed-parameter escape elision remains open", which the eighth pass and the §7.3 row had already retired* |
| §4.4/§7.1 `srcset` per-URL attribute routing | **done (compiler subset)** | `emit.rs::push_attr` routes dynamic `srcset={expr}` attributes through the dedicated `\Lintel\srcset()` helper on the string and stream targets (the runtime helper splits candidates and scheme-checks each URL); static values fold verbatim, the `rfc` target stays helper-free, component attributes stay named arguments. The exact contract is recorded in `docs/srcset-alignment.md` and tested by `tests/srcset.rs`; the `srcset()` runtime helper itself ships with the PHP runtime (T-07) |
| §7.3 escape elision (v0.1 "honest version" + the Y2 analyze pass) | **done (compiler subset + §17.1-Y2)** | `emit.rs::dyn_child` folds dynamic children that are provably safe — `(int)`/`(integer)`/`(float)`/`(double)` casts spanning the whole expression and `count()` calls fold to the raw PHP expression, and children that are exactly a parameter the enclosing function declares as a non-nullable `string` fold to a direct `\htmlspecialchars($name, \ENT_QUOTES \| \ENT_SUBSTITUTE \| \ENT_HTML5)` call — on both markup targets; everything else keeps `\Lintel\c()`, conservative by design. The scanner records parameter types (`scan::Signature::typed_strings`) and tracks a `scan::FunctionScope` per declared-function/braced-closure body, unproving names the body can rebind (assignment-shaped tokens, `global`/`static`/`unset`/`list`/`as`, destructuring) and clearing scopes under dynamic binding (`extract`, `$$name`, `include`/`require`, `eval`) and braceless `fn(...)=>` arrow bodies; the scope rides to the lowering through the `Lowering::emit_root_scoped` seam (default: ignore the scope). The exact contract is recorded in `docs/escape-elision.md` and tested by `tests/elision.rs`, `tests/typed_elision.rs` + the `emit` matcher unit tests. The PHPStan-backed Y2 pass (sixteenth pass): `crates/lintel-core/src/analyze.rs` + `emit.rs::AnalyzeLowering` + `compile_analyzed`/`scan_with_typed` consume the PHPStan bridge's proven child types (`php/lintel-phpstan/src/LintelTypesCollector.php` + `bin/analyze` → `<source>.lintel-types.json`); `lintel analyze` folds `int|float|numeric-string|literal` children strictly past the v0.1 ladder (`bool` deliberately keeps the helper: `c()` renders bools as `''`, raw interpolation renders `'1'` — no fold preserves the output), reports the elision counters (`children`/`considered`/`elided`/rate — the acceptance's elision-rate report, drift-checked by the `analyze` bench group), and `lintel check --types` renders stale report entries as advisories (`stale_hints`). Goldens + threat-corpus execution: `tests/analyze.rs`, `tests/stale_hints.rs`, `php/lintel-phpstan/tests/AnalyzeElisionTest.php`; the contract is `docs/type-elision.md` |
| T-10 CLI | **done (minimal subset)** | `crates/lintel-cli` — the `lintel` binary with `build` (stdout / `--out DIR` / `--in-place`) and `check` (diagnostics-only, exit 1 on errors, warnings advisory) over files and recursively-walked directories in sorted-path order; `stats` (since the ninth pass: the per-file component shape report — `declared`/`invoke` text table or one JSON object per file, exit 1 only on parse errors, validation lints play no part); `rules` (since the tenth pass: the built-in rule-registry listing, no paths, exit 0, text or `--format=json` — `docs/rule-registry.md`); `init` (since the eighth pass: scaffolds a runnable demo page and compiles it once, refusing to clobber); `--target=string|stream|rfc`, `--format=text|json` (since the seventh pass: machine-readable diagnostics on stderr, exit codes unchanged — `docs/diagnostics-json.md`), `--help`, `--version`. The engine lives in the library target so `tests/cli.rs` drives the exact code `main.rs` runs; the watch daemon shipped as T-12 below (hardened in the Phase 10 pass) — there is no rayon/blake3 layer (the build cache is the SHA-256 content-addressed `.lintel-cache/` engine), and a `lintel fmt` formatter stays §17.1-Q2 roadmap — the `runnable demo` acceptance half is now executable and green: `crates/lintel-cli/tests/php_demo.rs` runs `php -l` on the compiled demo and executes it under PHP asserting the rendered HTML (twelfth pass) |
| `NodeVisitor` read traversal (seventh pass) | **done** | `ast::NodeVisitor` trait + `visit_node`/`visit_roots` in `crates/lintel-core/src/ast.rs`, re-exported at the crate root: the exact top-down, depth-first, source-ordered walk the validation pass runs, as a read-only consumer contract (attributes ride on the element visit; root trees are disjoint, so every node is visited exactly once over one `scan()`). The contract is recorded in `docs/node-visitor.md`, tested by `tests/visitor.rs`, demoed by `examples/custom_visitor.rs` and benched by the `visitor` group |
| Component inventory (ninth pass) | **done** | `inventory::collect_inventory(roots, &context) → FileInventory` + `DeclaredComponent`/`Invocation`/`InventoryBuilder` in `crates/lintel-core/src/inventory.rs`, re-exported at the crate root: every declared component (the `declared_signatures` table, in declaration order, in the author's spelling) and every component invocation site (sorted by absolute source offset, with prop names, children/spread/`declared_in_file` flags), plus `FileInventory::to_json` under the `Diagnostic::to_json` contract (shared `lintel_diag::json_escape`). The report is read-only, infallible and suppression-independent. The contract is recorded in `docs/component-inventory.md`, tested by `tests/inventory.rs`, demoed by `examples/inventory.rs`, benched by the `inventory` group and shipped through the CLI's `lintel stats` command |
| Rule registry (tenth pass) | **done** | `validate::RuleMeta` (stable kebab-case `id`, the `codes` the rule emits, a one-line `summary`) + the provided `ValidationRule::meta` (default `None`, so consumer rules keep compiling) + `builtin_rule_metas`/`registry_metas` in `crates/lintel-core/src/validate.rs`, re-exported at the crate root; `RuleMeta::to_json` under the `Diagnostic::to_json` machine contract (stable field order `id`/`codes`/`summary`, shared `json_escape`). All eight built-in rules are classified, and `tests/rule_registry.rs` pins the completeness contract: the registry's `codes` union equals exactly the eight rule-emittable classes, and parse classes never enter the registry. The contract is recorded in `docs/rule-registry.md`, demoed by `examples/rule_registry.rs`, benched by the `rules` group and shipped through the CLI's `lintel rules` subcommand (no paths, exit 0, text or `--format=json`) — the machine-readable seed for the not-yet-written `docs/errors/LNT*.md` pages |
| Cross-file signature merge (eleventh pass) | **done** | `ScanContext::merged_with(&self, &ScanContext) → ScanContext` in `crates/lintel-core/src/scan.rs`: unions the declared-function set and the signature table for cross-file signature checks (`LNT0201`/`LNT0203`/`LNT0204`/`LNT0205` resolve against components declared in other files when a consumer validates with the merged context). The base file wins case-insensitive name collisions and keeps declaration order (documented non-commutative), the merge is read-only on both inputs, and suppression state deliberately does not merge (`lintel-ignore-*` windows and the file flag are per-source facts). Opt-in: `compile`/`validate_with` never merge on their own, so the §17.1 same-file boundary stands by default. The contract is recorded in `docs/cross-file-signatures.md`, tested by `tests/cross_file.rs`, demoed by `examples/cross_file.rs` and benched by the `cross_file` group |
| T-06 line preservation | **done** | `tests/line_preservation.rs` + every golden asserts line-count equality; the `rfc` target preserves lines too |
| T-07 runtime + security corpus | **done** | `php/runtime/src/functions.php` — the §6.8 transcription plus the `srcset()` helper §7.1 promises (`docs/srcset-alignment.md`) and a global `Html` alias so author-side unqualified `Html` type hints resolve; `php/runtime/tests/ThreatCorpusTest.php` — the §7.2 PHPUnit corpus (27 tests over every listed threat vector plus the §7.1 helper matrix); `phpstan.neon` — level max over `php/`, clean; `php/runtime/composer.json` — the dev-tooling lane (`composer test` / `composer stan`); CI gained the `php` job (setup-php, composer install, `PHP_BIN=php` wired into the Rust gates, PHPUnit, PHPStan) |
| T-11 Composer packages + installer plugin + dev Loader | **done** | `php/lintel` (`composer.json` for `lintel/lintel`, type composer-plugin): `src/Installer/Plugin.php` + `BinaryInstaller.php` (§12 download → SHA-256 verify → atomic executable cache at `<vendor>/bin/lintel`; `LINTEL_BIN` override; **no invented release URL** — no live URLs exist before T-18, an unconfigured install is a notice, and `extra.lintel.url` without `sha256` refuses) + `src/Dev/Loader.php` (§6.10: PSR-4 fallback autoloader over `ClassLoader::getRegisteredLoaders()`, mtime+size compile-on-stale cache, `lintel.json` functions-glob eager-require — the T-31 note — and the proc_open fallback, the T-12 daemon being gated). PHPUnit corpus 23 tests / 68 assertions (the installer download lane runs against a real local `php -S` fixture URL), PHPStan level max clean, `composer validate` clean. Acceptance executed for real: `scripts/dx-smoke.sh` drives a fresh plain-PHP skeleton AND a fresh `composer create-project laravel/laravel` skeleton through `composer require` → edit → refresh → recompiled, no watcher; the CI `dx` job replays it |
| T-31 PSR-4 function components | **done** | `crates/lintel-cli/src/manifest.rs` — `lintel build --manifest[=PATH]` scans successfully compiled sources for top-level `function Name(...): Html` (§6.10's T-31 note) and writes `lintel.json`: the user's `functions` globs preserved, `components` name→source, `prod.artifacts` source→artifact. Dev: `Loader::boot` eager-requires the sources **before** the PSR-4 fallback registers (class lane regression-guarded); prod: the plugin's `pre-autoload-dump` generates `<vendor>/lintel/functions.php` requiring each artifact (Composer's `$baseDir` convention) and appends it to the root package's `files` autoload — idempotent, in-memory — and records loaded sources in `$GLOBALS['lintelFromSource']`, which the loader consults to skip sources the shim already required (no lane double-declares a component). `scripts/dx-smoke.sh` proves the acceptance: a fresh Laravel skeleton with a component in `app/Components/` renders on refresh with no manual require |
| T-12 Watch/daemon mode | **done (unblocked by T-14's spawn evidence; hardened in the Phase 10 pass)** | `crates/lintel-cli/src/watch.rs` — `lintel watch [paths] [--socket PATH] [--foreground]`: background mode detaches itself (`process_group(0)`) and poll-connects for readiness (the probe opens and closes WITHOUT sending — the Phase 2 probe sent the `quit` frame and stopped the daemon it had just verified; `docs/daemon-protocol.md` pins the contract); the daemon binds a Unix socket (loopback-TCP fallback on Windows, compile-checked; its socket tests are DEFERRED-TO-CI/MAINTAINER — no Windows runner in the sandbox), watches the roots with `notify` (invalidation on notify's own thread through a shared cache — the serve loop is a plain blocking accept, no poll latency; events under `.lintel-cache/` never invalidate — the cache's own writes must not churn the map), keeps the §21.2 mtime+size cache (hits re-verify the stat key; a missed event self-heals), recompiles lazily per request, and answers the **length-prefixed JSON protocol** (ten ASCII digits, newline, payload — `{"path":…}` → `{"ok":true,"php":…}` / `{"ok":false,"diag":[…]}`, `{"quit":true}` stops it; a malformed frame closes the connection, the daemon serves on). Every miss consults the workspace's content-addressed `.lintel-cache/` build cache — the same engine `lintel build` uses: a watch-triggered rebuild of unchanged content never recompiles, a reverted edit serves instantly, and the counters land in `.lintel-cache/stats.json` (flocked, like the LSP index's `lsp-index.json`, via fs2 — a watch daemon, a concurrent build and parallel LSP sessions cannot interleave a partial write). The loader's §6.10 handoff is live on the same framing (`php/lintel/src/Dev/Loader.php`, two connect attempts then the proc_open fallback). Measured p50/p99 vs spawn (the acceptance criterion, `tests/watch.rs`): daemon **26 µs / 36 µs** vs spawn **724 µs / 973 µs**; the cache integration, the concurrent-clients-with-builds lane and the raw wire framing are pinned by `tests/watch.rs` |
| T-21 LSP foundation (subset) | **done (stretch)** | `crates/lintel-lsp` — `lsp-types` 0.97 + serde_json over injected `Read`/`Write` (stdio in production, in-memory mock clients in tests): `initialize` (full-text sync + hover + serverInfo), `textDocument/publishDiagnostics` on didOpen/didChange/didSave (errors and warnings; byte offsets → 0-based UTF-16 positions through the line-preservation guarantee; stable `LNTxxxx` code, `source: "lintel"`, `help:` in-message), `textDocument/hover` over `<ComponentName>` via `collect_inventory` + `ScanContext::signature_of` (markdown signature, required-`!` marks, §7.3 string-typed set), `-32601`/-`32700` errors, `shutdown`/`exit`. Same-file signatures at the foundation; everything this row once listed as Q3 backlog landed in later passes — cross-file folding (`ScanContext::merged_with`, eleventh pass), the persistent workspace index with definition/rename/references/workspace symbols (Phase 9/8), completion (the shipped five-capability surface) and the tree-sitter lane (Phase 11, the row below) |
| T-14 Benchmark harness | **done (native results committed; Docker-pinned run wired to CI)** | `bench/` — `run.php` (worker/orchestrator), `lib/` (the §9.2 scenarios S1–S5, Blade/Twig/Plates engines, the measurement-only `\Markup\Element` reference shape so the `rfc` lane is executed, not skipped, metrics/SVG/Markdown writers); OPcache/JIT matrix over three PHP subprocess settings; ns/render median+p95 over 30 runs, peak memory, compile throughput (cold/warm: 23k/25k files/s native), opcode-count probe (null on this stripped static runtime — instrumentation unavailable; the CI job re-probes on the pinned image); `bench/results/` commits the raw JSON + summary Markdown + five SVG charts from the native run with full provenance. `bench/Dockerfile` pins `php:8.6-cli` by digest (a real registry digest queried 2026-11-20); `make bench` runs Docker-pinned when a daemon exists, native otherwise; the CI `bench` job runs the pinned reproduction |
| T-15 Five demos | **done** | `lintel init --demo N` (1-5) scaffolds component-library, XSS-gallery (the §7.2 corpus, neutralized in text + URL positions), 10k-row table, HTMX todo, compile-error showcase (LNT cards as content); each ≤ 60 lines (max 31), compiles zero-diagnostics with line preservation pinned by a test, and **executes under PHP with asserted output** (`tests/php_demo.rs` reuses the php_demo gate pattern: `php -l`, render, per-demo assertions) |
| §18 Makefile | **done** | repo-root `Makefile` with the four documented targets: `ci` (fmt, clippy -D warnings, test, php lint, phpunit, phpstan), `golden` (the golden + line-preservation tests), `fuzz` (preflights cargo-fuzz and points at T-09, which owns the targets), `bench` (Docker-pinned §9.2 reproduction, native fallback); `scripts/php-gates.sh` backs the PHP half |
| T-01 cargo-deny drift | **closed** | `deny.toml` (advisories/licenses/bans/sources) + the CI cargo-deny step; `cargo deny check` green locally (intra-workspace path-dependency wildcards allowed with rationale; no speculative license allowances) |
| T-09+ differential/fuzz/daemon/WASM | **done** | differential + fuzz landed in the fifteenth pass (§21.2: the restored acceptance, the `copy_heredoc` fuzz bug it found and fixed, the 60-second `make fuzz` smoke and the CI ten-minute lane — since the Phase 12 M2 card the corpus seeds from the differential cases, the tree-sitter fallback edges and the §7.2 payloads, and the target also drives the LSP session); the daemon is T-12 above (Windows lane verified by the `windows-daemon` CI job since Phase 11); WASM is T-13 below |

| §17.1-Q3 testing helpers (Phase 8) | **done** | `php/lintel/src/Test/ComponentTester.php` (`Lintel\Test\ComponentTester`) — compile-and-render a component source in one call: the source compiles on the fly through the real binary (the §6.10 compile path; `Loader::binary()` resolution, an explicit `--binary`-shaped constructor override for tests), the artifact is required once per unique source (a second render reuses it — PHP cannot redeclare), the component invokes with named arguments (`$props` spread), and the result renders through the §6.8 runtime's `\Lintel\c()`. Assertions ride `PHPUnit\Framework\Assert` (`assertRenders`, `assertRendersContaining`); `render()` alone needs no PHPUnit (the composer `suggest` documents it). Documented in-process constraints, enforced loudly: two sources may not declare one component name, and a component named after a PHP builtin (`Stat` vs `stat()` — case-insensitive name table) can never be declared. The hostile-payload suite (`php/lintel/tests/ComponentTesterTest.php`, 12 tests) proves escaping is under test too: a child payload surfaces escaped, a `javascript:` URL is blocked to `about:invalid#blocked`, a quote breakout escapes |
| §17.1-Q3 LSP rename / references / workspace symbols (Phase 8) | **done** | `crates/lintel-lsp/src/workspace.rs` — `textDocument/rename` builds a `WorkspaceEdit` over the OPEN documents: a component target (cursor on the declaration identifier or a component-shaped tag) edits the declaration identifier and every `<Component />` tag (open *and* close) across the workspace; a prop target (cursor on a `$param` or a written attribute) edits the `$param` (the `$` rides the span) and every `prop={...}`/`prop="..."` attribute; body children `{$name}` are not attribute usages. `textDocument/references` lists the same spans (declaration behind `includeDeclaration`); `workspace/symbol` fuzzy-matches declared components (case-insensitive subsequence) as FUNCTION symbols on the identifier span, URI-ordered. Targets must be declared somewhere in the open workspace (an undeclared component is not renameable — a partial rename breaks the workspace); tag matches are exact-as-written (`Ui\Button` never matches a bare `Button` declaration). At the Phase 8 pass the workspace WAS the open documents; the persistent index landed in the Phase 9 M1 card (the row below), closing that carried Q3 seam; `run`'s three arms collapse into `workspace::respond` to keep the dispatcher under its `too_many_lines` budget. Mock-client scenarios (`crates/lintel-lsp/tests/lsp.rs`): cross-file rename from declaration and invocation, prop rename with the body child intact, refusal shapes (invalid/lowercase name, no-op rename, HTML element, undeclared component), references with/without declaration, prop references across files, fuzzy symbol search incl. empty query |
| Phase 9 — scale & production hardening | **done** | **M0 cwd hygiene:** the bare `--manifest` default anchors `lintel.json` to the workspace being compiled (the common ancestor of the build inputs and `--out` — the same root the dev loader and the Composer plugin read the manifest from), never the process cwd; the blade/twig console suites dropped their chdir workaround and both assert no stray manifest lands at the runner's cwd (`crates/lintel-cli/tests/cli.rs`: the spawned-binary cwd test, the in-process engine test, the multi-input shared-ancestor test). **M1 persistent LSP workspace index (§17.1 Q3):** `crates/lintel-lsp/src/index.rs` — `initialize` walks the workspace root (`workspaceFolders`/`rootUri`, dot-directories and `vendor`/`node_modules` skipped), indexes every `.lintel.php` file's declarations, persists them to `.lintel-cache/lsp-index.json` (best-effort, keyed by mtime+size), and `rename`/`references`/`workspace/symbol`/`definition` consult the index in addition to the open documents: a closed file's component is renamable (edits computed against its validated disk bytes ride the `WorkspaceEdit`), referenceable, symbol-listed, and a `<Component />` tag resolves to its declaration location cross-file. Stale entries evict by mtime+size on access; `didOpen`/`didChange` full sync/`didSave` re-parse the synced text immediately (the buffer is the truth while the file is open). Mock-client scenarios: cross-file definition/rename/references/symbols with the declaration file closed, buffer-sync invalidation, the cache round-trip across sessions, and mtime+size eviction after an on-disk edit. **M2 incremental build cache (§9.3 warm-build target, §17.1-Q2 incremental cache):** `crates/lintel-cli/src/cache.rs` — SHA-256 content+target keys, `<hash>.php` + sidecar (source, target, options hash, rendered warnings) under `<workspace>/.lintel-cache/`, warm builds replay warnings verbatim in both formats, `--no-cache` bypasses lookup and store, `check`/`analyze` never read it, and stdout builds (the §6.10 dev loader's lane) leave no cache beside the sources. `.lintel-cache/stats.json` records compiled/cache-hits/bypassed; the integration tests measure the compile counts through it; the criterion bench (`cargo bench -p lintel-cli --bench build_cache`) pins cold 24-file ≈4.8 ms vs warm ≈1.2 ms (I/O-bound) vs `--no-cache` ≈3.1 ms. **M3 stream-target tester + docs:** `ComponentTester::renderStreaming()` compiles `--target=stream` and consumes the generator through `\Lintel\stream()`; the parity suite proves the consumed stream equals the string render byte for byte (hostile payload included) and the one-target-per-process constraint refuses loudly. The parity acceptance proved the stream artifact's declared `: Html` TypeErrored against its own generator body (`Return value must be of type Html, Generator returned` under `strict_types`) — the freeze lifted for the proven bug and the scanner now rewrites `Html`-spelled declared return types to the non-null `\Generator` on the same line when the stream lowering is active (`Lowering::rewrites_return_types`; unions and body-less declarations verbatim), regression-tested in `tests/stream_target.rs`. Docs: the CHANGELOG's Phase 9 entries, the docs-site `Tooling` page (incremental builds + the LSP workspace index), and the §10 stream contract's return-type note in `docs/stream-alignment.md` |
| Year-3 RFC convergence — the real `lintel migrate` (Phase 8) | **done** | `crates/lintel-cli/src/lib.rs` — `migrate` is no longer the thin build alias: it compiles each source with `Target::Rfc` and REWRITES the source file in place (line-preserved `new \Markup\Element` lowering). `--dry-run` prints a unified diff per file (LCS line diff, 2 context lines, `--- a/…`/`+++ b/…`/`@@` headers) and writes nothing; `--backup` keeps the original bytes beside the rewrite as `<file>.bak`; an already-RFC-parity file reports `already migrated` without a diff. Safety rails: a file that fails to compile is refused untouched with its rendered diagnostics, and a file whose component invocations none of the migrated sources declares is refused too (`lintel: cannot migrate … declared in no migrated source`, JSON-shaped under `--format=json`) — cross-file components resolve through the migrated SET's declared names (case-insensitive), so migrating a directory migrates coherently. `--out`/`--in-place` are now refused under migrate (the rewrite IS the output); `--to=native` stays the only value. Integration contracts in `crates/lintel-cli/tests/migrate.rs` (7 tests: in-place parity + line preservation, dry-run diff bytes, backup bytes, parse-error refusal, unresolved-component refusal + cross-file resolution, already-migrated no-op, flag gating) |
| Phase 11 — advanced LSP parsing, streaming timing & cross-platform completeness | **done** | **M1 tree-sitter LSP lane (§17.1 Q3's "tree-sitter grammar"):** `crates/lintel-lsp/src/tree_sitter.rs` — the upstream `tree-sitter-php` grammar (crates.io 0.25, MIT; the `wasm`/wasmtime feature off; `Zlib` joined the deny.toml allowlist per the documented as-they-appear policy for `foldhash`) parses every indexed source as an independent second lane, and `definition`/`rename`/`references` (plus `workspace/symbol` through the shared accessor) consult the merged declaration table — the tree-sitter view UNIONED with the lintel-core scanner's (dedup by name + identifier offset; the lanes' offsets are byte-equal where both see a declaration, pinned by a unit test). A file whose tree-sitter parse cannot see its declarations falls back to the scanner (`function Badge(***): Html` — the probe-found fallback shapes); the per-file lane rides `.lintel-cache/lsp-index.json` (shape version 2) across sessions. Documented grammar limits, probed before the design: markup regions are ERROR soup (the PHP grammar does not model JSX; no upstream JSX extension exists to reuse — the scanner stays the JSX-structure authority), error recovery can swallow a declaration after a markup region (the union closes the gap; on the healthy fixture ts sees `Badge`, the merged view sees `Badge` AND `Page`), and multibyte *identifiers* are outside both lanes (the scanner's rule is ASCII-shaped) while multibyte *content* leaves both lanes' byte offsets byte-accurate. The acceptance is the monorepo-fixture integration test `crates/lintel-lsp/tests/tree_sitter.rs` (5 tests): cross-file definition into a CLOSED file, rename spanning the closed declaration + the open invocation, references across the session cache, the malformed file staying declared through the fallback (rename + definition still answer), and the cache round-trip across a session boundary. **M2 RR progressive per-boundary timing (§10, closes the "progressive per-boundary timing on RoadRunner is CI-lane work" note):** `bench/lib/ProgressiveTiming.php` renders a three-boundary `<Suspense>` page through the REAL `RoadRunnerFlushTransport` (composed, not subclassed — the class is final; a recording worker drains `respond()` one frame per chunk), models deterministic latency (usleep slices interleaved with `Fiber::suspend()`, resolution order lt-1→lt-2→lt-3), and measures TTFB-first-placeholder (~8 µs), TTFB-first-resolved (~37 ms) and the fully-resolved total (~73 ms) — asserting INSIDE the harness that placeholder N flushes before resolved N-1, resolution stays in order, and the RR wire order equals the flush order (30 chunks byte-identical); a broken flusher fails the lane instead of producing numbers. Wired into `bench/run.php` (the S6 summary section + `bench/results/progressive-timing.json`, artifact-only per the job's commit-nothing guard), CI's `bench` job through `make bench` (Docker-pinned php:8.6 digest AND native fallback; `spiral/roadrunner-http ^4.2` joined bench's dev deps for the `HttpWorkerInterface`, lock refreshed), and a Rust integration test (`crates/lintel-cli/tests/progressive_timing.rs`, `PHP_BIN`-gated) drives the CLI entry `bench/lib/progressive_invariant.php`. Docs: `docs/deployment/roadrunner.md`, `docs/stream-alignment.md` and the transports.php §10 header record the closure. **M3 Windows socket lane (closes T-12's carried DEFERRED-TO-CI/MAINTAINER):** the new `windows-daemon` CI job runs the whole watch suite on `windows-latest` over the daemon's loopback-TCP fallback — serve + recompile-on-change, the build-cache-backed rebuilds, the concurrent-clients lane and the p50/p99-vs-spawn measurement — with serial test threads (each test tears down a scratch directory whose socket the next daemon must not hold open under Windows's strict directory locking); the `#[cfg(windows)]` arms additionally compile-checked locally via `cargo check --target x86_64-pc-windows-gnu`; `docs/daemon-protocol.md` and the watch module docs record the closure. Docs: the CHANGELOG's Phase 11 entries and the docs-site `Tooling` page's tree-sitter section |

Benchmarks live in `crates/lintel-core/benches/compile.rs` (criterion): the
four compiler-throughput scenarios plus `rfc_component_tree` (RFC-parity
lowering), `stream_component_tree` (streaming generator lowering),
`warned_handlers` (warn-and-continue validation path), `signature_checked`
(in-file signature validation path), `suppressed_handlers` (the
`lintel-ignore-next` window capture and per-diagnostic lookup path),
`required_missing` (the LNT0205 required-parameter rejection path) and
`custom_rule` (the injected validation-rule registry: built-in set plus one
consumer policy rule, against the hardcoded `component_tree` cost),
`file_suppressed_handlers` (the file-level `lintel-ignore-file`
short-circuit against the unsuppressed `warned_handlers` cost),
`srcset_attrs` (the `srcset` per-URL attribute routing path) and
`elided_children` (the §7.3 escape-elision path: provably-scalar children
compile without the `\Lintel\c()` wrapper) and, since the eighth pass,
`typed_children` (the typed-parameter direct-escape path: `string`-typed
parameter children fold to `\htmlspecialchars(...)` without the wrapper,
drift-checked against the wrapped emission in the bench body). A
second group, `sourcemap`, measures the offset→line:col lookups behind
every rendered diagnostic. Since the seventh pass, a `json` group measures
the machine-readable emission path (`Diagnostic::to_json` over the
`warned_handlers` warnings — `docs/diagnostics-json.md`), a `visitor`
group measures the read-only `NodeVisitor` traversal
(`docs/node-visitor.md`), and since the ninth pass an `inventory` group
measures the component shape report (`collect_inventory` over the
component tree, drift-checked against the 13 expected invocations and 1
declared signature in the bench body — `docs/component-inventory.md`),
and since the tenth pass a `rules` group measures the registry-listing
path (`builtin_rule_metas` JSON emission plus `registry_metas` over a
synthetic 128-rule classified registry — `docs/rule-registry.md`), and
since the eleventh pass a `cross_file` group measures the merged-context
validation path (plain context checked to fire nothing, merged context
drift-checked against the nine expected `Row` children violations —
`docs/cross-file-signatures.md`). They
measure compiler throughput only; the §9.2 PHP rendering harness is future
work, so no perf claims beyond these numbers are made anywhere.

### 21.2 Drift corrections baked into the executable goldens

The hand-written §6.9 golden does not follow §6.3's own `line_of` semantics
(it places `\Lintel\c($label)` on the opening line of the `<span>` literal
even though the expression sits one source line later, and it shows one
fewer closing paren). The executable goldens in
`crates/lintel-core/tests/golden.rs` are now the source of truth; they keep
the invariants that matter — total line count preserved, every non-JSX token
on its original line — and are verified end-to-end by the test suite.

A seventh-pass drift correction fixed the bench source builders:
`warned_handlers_source`, `suppressed_handlers_source` and
`file_suppressed_handlers_source` wrote `onclick={track('row-i')}` — a
dynamic handler, so `warned_handlers` measured the LNT0301 *rejection*
path while its doc comment, name and this table claimed the LNT0302
*warning* path (rustc accepts the lone `}` in the format string, so the
compile could not catch it, and the benches never unwrapped the result).
The builders now emit static `onclick="track('row-i')"` strings, and the
`json` bench asserts the 20 warnings exist so the path cannot silently
drift back.

The twelfth pass recorded three transcription deviations, each forced by a
failing gate rather than preference:

- **PHPStan level max vs. the §6.8 bodies as written.** The reference
  runtime casts `mixed` values to `string` in four places and takes the
  spread map untyped; level max rejects both. The bodies stay verbatim and
  carry four per-line `@phpstan-ignore cast.string` annotations with the
  contract as the rationale, plus one `@param array<string|int, mixed>`
  shape on `attrs()`.
- **A global `Html` alias.** §6.9's seed and the `lintel init` demo declare
  unqualified `Html` return types, and compiled output is global-namespace
  PHP; the init-demo execution gate failed with a TypeError until the
  runtime aliased `Lintel\Html` to the global `Html` name (§6.8 as written
  has no alias, and its own seed cannot execute without one).
- **`srcset()` had no reference body.** §7.1 and `docs/srcset-alignment.md`
  pin the contract ("split candidates, validate each URL") but not the
  code; the implementation probes the whole candidate (browsers ignore
  ws/ctl inside schemes, so `java\tscript:x` is one obfuscated URL, not a
  URL plus descriptor), splits candidates on commas followed by whitespace
  or end of value (so `data:text/html,x` keeps its URL-internal comma), and
  keeps only a trailing width/density descriptor on a blocked candidate.

The CI `php` job pins PHP 8.4 (`shivammathur/setup-php`), not the doc's
"PHP 8.6 container": 8.6 is the launch target, 8.4 is the maintained
runtime the corpus can actually exercise; the runtime package declares
`php: >=8.1`.

The fourteenth pass (F0/F1 + cards T-13/T-17/T-16/T-09) recorded the
following closures and deviations, each forced by a failing gate rather
than preference:

- **F0 (CI repair):** `ci.yml`'s cargo-deny step name carried an unquoted
  colon, so YAML parsing failed and **zero jobs ran**. The name is quoted,
  actionlint (user-space binary) joined the local gate set (`make
  workflows`, folded into `make ci`), and a pinned `workflow-lint` CI job
  parse-validates every future workflow edit. F0 unblocks the
  rust/php/dx/bench jobs, so T-14's two DEFERRED-TO-CI halves (the
  Docker-pinned bench and the opcode probe) can execute on runners.
- **F1 (stream closures capture enclosing locals):** the thirteenth pass's
  §21.2 bullet — "the stream emit's closures capture nothing" — is closed.
  The emitted closure is now `static function () use (...): \Generator`
  with a by-value `use` list for every enclosing local its body
  references, computed from the assembled body (single-quoted literals
  hold no interpolation and are skipped; double-quoted and heredoc
  interpolations count; comments are skipped; superglobals and `$this` are
  never captured; nested component-children closures ride transitively
  through the outer capture). The regression test executes a streaming
  component with a typed parameter under PHP and asserts the render plus
  an empty stderr; `docs/stream-alignment.md`'s contract table shows the
  captures; the compiler-freeze exception covered this repair only.
- **T-13 (WASM playground):** `crates/lintel-wasm` (the card-named entry
  crate) wraps `compile()` behind `wasm-bindgen`; `playground/` ships
  side-by-side output, a diagnostics panel, copy compiled, an examples
  dropdown and `#src=…&t=…` permalinks, with a service worker making it
  work offline after first load. The acceptance "loads <1 MB gz" is
  asserted by `crates/lintel-wasm/tests/size_budget.rs` (`make
  size-budget`): the bundle gzips to ~50 KB and the whole page payload to
  ~59 KB. Verified served via `php -S` and in a real headless browser,
  offline included.
- **T-17 (docs site v0):** `crates/lintel-docs` (the card-named site
  builder) renders `docs-site/pages/*.md`, the registry-driven error index
  (serving `/e/LNTxxxx` exactly as `Code::docs_url` promises) and the
  committed benchmark results with the methodology embedded verbatim; the
  built tree mirrors the playground so the site is self-contained. The
  §13.7 docs link check runs inside the build (`make docs-site`) and fails
  it on any broken internal link. Deploy to lintel.sh stays T-18.
- **T-16 (VS Code grammar):** `editors/vscode` ships the TextMate grammar
  (`source.lintel.php`: PHP first, JSX at expression starts, backreferenced
  close tags so children and nesting highlight), the language binding for
  `*.lintel.php` and the `lintel-check` problem matcher. The acceptance
  "JSX highlighted inside .lintel.php" is asserted by the tokenization
  test (`npm test`) running the grammar through vscode-textmate +
  vscode-oniguruma — the engine VS Code itself uses — and replayed by CI's
  `editors` job.
- **T-09 (differential + fuzz): partial.** The fuzz half is real:
  `fuzz/compile_fuzz` exercises `compile()` on all three targets with the
  never-panic/never-loop invariants and sampled `php -l` validation,
  `make fuzz` runs the 60-second smoke locally and CI's nightly `fuzz`
  lane runs the ten-minute acceptance. The differential half did **not**
  land: the proptest generator was replaced by a deterministic LCG
  generator mid-session and its remaining contract mismatches against the
  hand-written reference renderer (class lists, spreads, self-closing
  shapes) were not reconciled inside the session budget, so the
  incomplete test was removed rather than left red. It is the first
  backlog item for T-09's closure.
- **S5 re-bench (honesty fix):** the §9.2 harness' stream lane now compiles
  a **dynamic** streaming component (locals through the F1 capture, no
  superglobal detour) and the committed `bench/results/` artifacts were
  regenerated in the same native environment (PHP 8.4.11 static CLI, no
  opcache — the opcode probe stays null here and is re-attempted by the CI
  bench job on the pinned image). The dynamic stream lane's TTFB
  (~3.4–4.2 µs median) replaces the prior static-only ~1.9 µs figure; the
  sync lane still blocks ~15.2 ms.

The fifteenth pass (CI repair + T-09 closure + T-18/T-19/T-20) recorded the
following closures and deviations, each forced by a failing gate or a
differential divergence rather than preference:

- **F2 (CI repair — fuzz lane).** The fourteenth pass's fuzz lane was red on
  `main` (run #44): `copy_heredoc` sliced `src` at a non-char-boundary when
  `<<<` had no label and a quote skip landed inside a multi-byte char —
  `crates/lintel-core/src/scan.rs` (`copy_heredoc`, the empty-label rewind).
  PHP lexes `<<<"` with no identifier as no heredoc at all, so the scanner
  rewinds to just after `<<<` and the bytes pass through verbatim. Fuzzer
  verdict: **compiler bug fixed**. Regression:
  `crates/lintel-core/tests/passthrough.rs::heredoc_start_without_label_passes_through`
  (the minimized crash-5bee847c artifact no longer crashes; a 5-minute fuzz
  run after the fix found nothing further).
- **F2 (CI repair — bench lane).** The `bench` job failed at `make bench`:
  the Dockerfile built with `bench/` as the build context, so the
  `../php/runtime` path repository in `bench/composer.json` did not exist
  inside the image and `composer install` aborted. `bench/Dockerfile` now
  mirrors the repo layout (context = repo root, `WORKDIR /src/bench`,
  `COPY php/runtime /src/php/runtime`), verified by a reproduction of the
  stage-1 `composer install` against that exact layout. Verdict: **CI
  wiring bug fixed** (no test was touched; the pinned `php:8.6-cli` digest
  was re-verified against the Docker Hub registry, HTTP 200 same digest).
- **T-09 differential closure.** The differential acceptance (§13.2) is
  restored at `crates/lintel-core/tests/differential.rs` +
  `crates/lintel-core/tests/differential/reference_renderer.php`: proptest
  random JSX trees → compile → `php -l` (all three targets) → execute on
  the string AND stream targets → compare stdout byte-for-byte with the
  hand-written reference renderer. **220 generated templates green, 0
  deleted or ignored** (plus 23 isolated reconciliation cases — this note's earlier "24" was a counting slip, fixed by the Phase 12 docs sweep). Per-
  divergence verdicts:
  - *class lists* — reference bug fixed: the reference model's class-list
    materializer (`r_clstpl` in `reference_renderer.php`) now walks
    conditional (`'cls' => cond`), nested and empty-string entries per
    §4.4; the compiler's `cls()` emission needed no change.
  - *self-closing shapes* — no side was wrong: voids serialize `<br>`, a
    non-void `<div />` expands `<div></div>` (§4.2), and the generator was
    taught that JSX has no bare-void source form (Babel rejects `<br>`
    with "Expected corresponding JSX closing tag" — the compiler's
    LNT0102 matches).
  - *spreads* — two fixes: (a) **runtime bug fixed** —
    `php/runtime/src/functions.php` `attrs()` now routes an exact `srcset`
    key through `srcset()` (§4.4's spread row: "each entry goes through
    the same attribute rules"); a spread `srcset` previously bypassed
    per-URL validation entirely (regression:
    `spread_srcset_routes_like_the_attribute`); (b) reference fixed to the
    same contract.
  - *stream nesting (new divergence, found by the generated cases)* —
    **runtime bug fixed**: `c()` escaped the chunks of a `Traversable`
    child, so a nested streamed component rendered double-escaped
    (`&lt;section…`). The stream target's chunks are compiler-produced
    markup (literals) or already-escaped data, so `c()` now walks a
    `Traversable` raw while plain arrays stay escaped-data
    (`php/runtime/src/functions.php` `c()`); the reference model already
    encoded the §10 contract.
  - The remaining isolated cases pin static URL folds verbatim (§4.4's
    static row), URL/entity-encoded allowlist behavior, every §4.5 child
    class, component named-arg/bare-prop/spread semantics, raw-text
    bodies, style charset, fragments, comments, and the LNT0202/LNT0301
    compile rejections.
- **T-19 launch honesty (migrate alias).** §16 post #3 names
  `lintel migrate --to=native`; the command now **exists** as a thin alias
  over `build --target=rfc` (`crates/lintel-cli/src/lib.rs` `parse` /
  `parse_args` — card-named freeze exception). `--to=native` and
  `--to native` are accepted, other values and `--target` overrides are
  rejected with a pointer at `docs/rfc-alignment.md`;
  `crates/lintel-cli/tests/cli.rs::migrate_is_a_thin_alias_for_build_target_rfc`
  pins all three spellings, `migrate_rejects_other_to_values_and_target_overrides`
  pins the rejections, and `usage_errors_cover_flags_paths_and_modes`
  was updated for the widened missing-command message (contract text
  change, not a weakened assertion).
- **T-18 release engineering.** v0.1.0 declared once in
  `[workspace.package]` and inherited by the five crates; composer package
  versions deliberately stay tag-derived (an explicit `version` field
  fails `composer validate` with a publish advisory, exit 2 — the tag is
  the bump); `LICENSE-MIT`/`LICENSE-APACHE`/`CHANGELOG.md`/`SECURITY.md`/
  `CODE_OF_CONDUCT.md` authored; `.github/workflows/release.yml` builds
  the §12.1 matrix with SHA256SUMS + build-provenance attestation. The
  local dry run (release build → archive → checksums) and the installer's
  tamper test are green; **tagging and publishing stay
  DEFERRED-TO-MAINTAINER**.
- **T-19/T-20 assets.** OG image `docs/og.png` (1280×640), VHS tape
  `docs/demo.tape` rendered by the new `launch-assets` CI job (VHS/ffmpeg
  are absent in this sandbox — the CI render job is the card's sanctioned
  alternative), `docs/launch/posts.md` (the three §16 variants, `wc -m`
  = 166/159/131 ≤ 280, every claim linked to a reproducible artifact),
  `docs/faq.md`, `install.sh` (checksum-verifying installer, dry run
  green) and `scripts/walkthrough.sh` (timed newcomer walkthrough:
  install → init → edit → `build --in-place` → render over HTTP, 1 s
  locally vs the 600 s budget; macOS/Windows legs DEFERRED-TO-CI as the
  card allows, in the new `install-smoke` matrix job).

The thirteenth pass (T-11/T-14/T-15) recorded five deviations, each forced
by a failing gate rather than preference:

- **§6.10's `ClassLoader::class::getRegisteredLoaders()` line is invalid
  PHP** — `php -l` rejects the double-`::` chain outright. The loader uses
  the correct static call `ClassLoader::getRegisteredLoaders()`; its tests
  register a real `ClassLoader` (constructed with a vendor dir — the only
  way `register()` records a loader in `getRegisteredLoaders()`).
- **§6.10's cache key carries no size.** The doc hashes the path and adds
  `filemtime` only; a test edits a source with the *same* mtime and a
  different size and is served stale forever. The key is now
  `hash('xxh3', $file) . '-' . mtime . '-' . size`.
- **§6.10's `lintel build --stdout` flag does not exist.** The real §6.7
  CLI rejects `--stdout` (exit 2): `build` writes stdout by default. The
  loader invokes `lintel build <file>`.
- **The stream emit's `static function (): \Generator` closures capture
  nothing** (docs/stream-alignment.md as shipped): a streamed component
  body cannot see its own locals — not even its own function's parameters.
  The bench S5 stream lane routes its data through a superglobal; the emit
  fix (emit `use (...)` for referenced locals) is backlog item (§f); the
  compiler core stays frozen this session.
- **`make golden` runs the committed golden + line-preservation tests
  instead of `cargo insta review`** — no cargo-insta dependency exists in
  the tree; the goldens are plain integration tests (§21.1).
- **The seventeenth pass inherited a red pedantic-clippy gate** (four
  warnings left in the sixteenth pass's newest CLI code) and repaired it
  by correcting the code, no `#[allow]` anywhere: `lintel-cli`'s
  `types_flag` helper had been left unused (dead-code) by the `--types`
  arms' inline duplicates — the arms now call it and the duplicated
  contract text is gone; the helper's insertion also mangled the doc
  comments (`parse_args`' intro sat on `types_flag`) — restored; the
  duplicated `--to` value handling collapsed into the shared `migrate_to`
  helper, putting `parse_args` back under the `too_many_lines` budget; and
  `tests/stale_hints.rs` carried a bare `assert!(stale.is_empty())`
  (`clippy::assert_is_empty`) and a needless borrow. Clippy
  `--workspace --all-targets --all-features -- -D warnings -W
  clippy::pedantic` is zero-warning again.
- **The `analyze_children` bench drift-check could never fire** (same
  pass): its hint scanner grepped source lines for `page {{$page}} of` —
  the double-brace spelling as it appears in Rust source — while the
  source builder's `writeln!` format escaping had long since emitted the
  literal `page {$page} of` (single braces), so the scan found zero hints
  and the bench panicked on `assert_eq!(hints.len(), 20)` before measuring
  anything (`cargo test --benches` red: left 0, right 20). The scanner now
  matches the emitted bytes; the analyze bench drift-checks 40 dynamic
  children / 20 elided / rate 0.5 as its doc comment promises.

Two doc-open decisions resolved here: the installer never invents a live
release URL (no releases exist before T-18; `LINTEL_BIN` and
`extra.lintel` {url, sha256} are the supported sources), and the
`lintel.json` manifest path defaults to the process cwd with an explicit
`Loader::boot(..., projectRoot:)` override (§6.10 does not pin it).

### 21.3 Documented v0.1 scanner limitations

`<?` short open tags are not recognized (deprecated upstream); a file
without a `<?php` open tag is treated as a pure HTML template and passed
through untouched; `<\Namespaced\Tag>` written with a leading backslash is
treated as operators (use `<Ui\Button>`). Each is asserted by a test so the
behavior is intentional, not accidental.
