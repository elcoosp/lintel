# srcset alignment: the per-URL `srcset` attribute routing

> Architecture doc §4.4 (attribute table) lists `srcset` among the URL
> attributes, and §7.1 (escaping matrix) pins `srcset()` as the v0.2 helper:
> *"split candidates, validate each URL"*. This page records the exact
> compiler-side contract for `lintl-core`, the same way
> `docs/rfc-alignment.md` and `docs/stream-alignment.md` record the lowering
> contracts, so the behavior is intentional and testable
> (`crates/lintl-core/tests/srcset.rs`).

## Source of truth

- §4.4: `href`, `src`, `action`, `formaction`, `poster`, `cite`, `data`,
  `srcset` are **URL attributes**; srcset values are "parsed per-URL".
- §7.1: `srcset` routes to a dedicated `srcset()` helper that splits the
  candidate list and validates **each** URL against the scheme allowlist —
  a single-value escape like `\Lintl\u()` cannot, because it sees one
  opaque string.

## Exact lowering contract

| Source | Lowered PHP (string/stream targets) |
|---|---|
| `<img srcset={$variants} />` | ` \Lintl\srcset('srcset', $variants)` |
| `<img srcset="a.png 1x, b.png 2x" />` | ` srcset="a.png 1x, b.png 2x"` (folded literal) |
| `<Img srcset={$variants} />` | `Img(srcset: $variants)` (named argument, no helper) |
| spread `{...$attrs}` | `\Lintl\attrs($attrs)` — per-key routing is the runtime helper's contract |

Details the doc does not pin down, decided here so the behavior is
intentional and testable:

1. **Why not `\Lintl\u()`.** Adding `srcset` to the URL-attribute list
   would scheme-check the *whole* attribute string as one URL. A srcset
   value is a comma-separated candidate list (`url [width|density]`, …), so
   each candidate needs its own scheme check; the dedicated helper owns the
   splitting.
2. **Routing is case-insensitive, the emitted name is as written.** The
   comparison lowercases the attribute name (HTML attribute names are
   case-insensitive), while the helper's first argument keeps the author's
   spelling, exactly like `\Lintl\u()`/`\Lintl\a()`.
3. **Static values fold verbatim.** A quoted `srcset="…"` attribute is
   author-trusted static markup and folds into the literal at compile time,
   the same rule as every other static attribute (doc 4.4); no runtime
   helper is emitted for it.
4. **Both markup targets share the routing.** The `stream` target emits the
   same `\Lintl\srcset()` call inside its `yield` chunks because it shares
   the attribute emitter with the string target; escape-by-default (doc 4.4)
   is unchanged.
5. **The `rfc` target stays helper-free.** Under the RFC model the runtime
   owns escaping, so `srcset={expr}` passes through as a raw
   `'srcset' => expr` array entry, like every other attribute
   (`docs/rfc-alignment.md`).
6. **Component attributes are named arguments.** Helper routing is an
   HTML-attribute concern; on a component invocation `srcset` is a plain
   named argument and never re-routed.
7. **Line preservation holds.** The re-routed call is a dynamic part like
   `\Lintl\u()` and participates in the same newline interleaving;
   `srcset_preserves_line_count` asserts the invariant end-to-end.

## What the compiler does not do yet

- **No compile-time candidate parsing.** The compiler emits the helper call
  and never splits or validates the candidate list itself; per-candidate
  scheme checks are the `\Lintl\srcset()` runtime contract, and that
  helper ships with the PHP runtime (T-07), not the compiler.
- **No `srcset()` runtime.** The `php/` workspace does not exist yet; until
  it lands the emitted calls are forward references, like every other
  `\Lintl\*` helper the compiler emits.
