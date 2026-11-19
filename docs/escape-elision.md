# Escape elision: provably-scalar children fold without the `c()` helper

> Architecture doc §7.3 ("Escape elision (honest version)") plans a v0.1
> compiler-side subset: *fold literals; direct `htmlspecialchars` when the
> expression is a typed parameter of the enclosing function; `(int)`/`(float)`
> casts and `count()` skip escaping.* The literal fold has shipped since the
> first emitter; this page records the exact contract for the next subset —
> the **provably-scalar expression fold** — the same way
> `docs/rfc-alignment.md`, `docs/stream-alignment.md`,
> `docs/srcset-alignment.md` and `docs/ignore-directives.md` record their
> contracts, so the behavior is intentional and testable
> (`crates/lintel-core/tests/elision.rs`).

## Source of truth

- §7.3: `(int)`/`(float)` casts and `count()` calls *skip escaping* — an
  int/float interpolates as digits (`NAN`/`INF` included) and cannot carry
  markup, so the `\Lintel\c()` call is pure overhead (§9.1: "everything
  folds that can fold; measure instead of assuming").
- Doc 4.4 (escape-by-default) is unchanged for everything else: any child
  whose type is not provable at compile time keeps the helper.

## Exact lowering contract (`string` and `stream` targets)

| Source child | Lowered PHP |
|---|---|
| `{(int)$n}` | `(int)$n` (raw concat operand / `yield (int)$n;` chunk) |
| `{(integer)$n}` / `{(INT)$n}` | emitted as written — PHP lexes the same int cast |
| `{(float)$rate}` / `{(double)$rate}` | `(float)$rate` / `(double)$rate` |
| `{count($rows)}` | `count($rows)` (always returns an int) |
| `{(int)$row->cells()}` | raw — the cast covers a `$var` accessor chain |
| `{(int)($a . $b)}` | raw — the cast covers the whole parenthesized group |
| `{(int)$n . $suffix}` | `\Lintel\c((int)$n . $suffix)` — the operator binds the cast's *result*; the child is a string |
| `{count($rows) . ' rows'}` | wrapped — same rule: `count()` must exhaust the expression |
| `{(string)$s}` | wrapped — a string cast is a string and can carry markup |
| `{strlen($s)}` | wrapped — unknown callee, unknown return type |
| `{$n}` / `{$arr[0]}` / `{$obj->prop}` | wrapped — the v0.1 subset has no type inference |

Details the doc does not pin down, decided here:

1. **A cast must span the whole child expression.** PHP casts bind tighter
   than every binary operator, so `(int)$n . $suffix` parses as
   `((int)$n) . $suffix` — a string concat. The matcher therefore accepts a
   cast only when the keyword is followed by *one operand atom* that
   exhausts the expression: a `$var` accessor chain (`->prop`,
   `->method(...)`, `[...]` links, no intervening whitespace), a balanced
   parenthesized group, a numeric literal, a constant, a call whose closing
   paren ends the expression, a unary sign, or another cast
   (`(int)(float)$x` recurses; `(int)((float)$x)` is the group form).
2. **Cast keywords are matched case-insensitively**, like every PHP
   keyword, including the `(integer)` and `(double)` aliases, and the
   emitted expression keeps the author's spelling.
3. **`count()` is elided, `sizeof` is not.** `count` always returns an int
   whatever the argument (PHP function names are case-insensitive); the
   doc's v0.1 list names `count()` only, so the alias conservatively keeps
   the wrapper.
4. **Unknown callees stay wrapped even under a cast-free int return.**
   `strlen($s)` returns an int, but proving that would need a runtime
   signature table — the honest v0.1 subset elides only the two shapes the
   language itself proves: casts and `count()`.
5. **Only child expressions elide.** Attribute values keep
   `\Lintel\a/u/srcset/cls/sty` because those helpers carry semantics a
   scalar fold would drop (bool/null handling, per-URL scheme checks,
   class/style list handling); spreads stay `\Lintel\attrs()`; components
   stay named arguments.
6. **Both markup targets share the fold.** The string target emits the raw
   expression as a concat operand, the stream target as a `yield` chunk;
   the `rfc` target was already helper-free and passes the expression
   through unchanged.
7. **Line preservation holds.** The elided expression is a dynamic part
   like `\Lintel\c(...)` and participates in the same newline
   interleaving; the `elided_children_preserve_line_numbers` test asserts
   the invariant end-to-end.

## What the compiler does not do yet

- **Typed-parameter elision** ("direct `htmlspecialchars` when the
  expression is a typed parameter of the enclosing function") stays open:
  it needs the scanner to track the enclosing function's signature for
  each JSX root and to record parameter *types*, not just names — the
  remaining §7.3 v0.1 bullet, deliberately not bundled with this fold.
- **PHPStan-backed elision** (`int|float|bool|numeric-string|literal`) is
  the §7.3 Y2 plan (`lintel analyze`); benchmarks must report the elision
  rate when it lands.
- **No runtime change.** The PHP runtime (T-07) is unaffected: the emitted
  calls shrink, the `\Lintel\c()` helper itself stays for every wrapped
  child.
