# Escape elision: provably-safe children fold without the `c()` helper

> Architecture doc §7.3 ("Escape elision (honest version)") plans a v0.1
> compiler-side subset: *fold literals; direct `htmlspecialchars` when the
> expression is a typed parameter of the enclosing function; `(int)`/`(float)`
> casts and `count()` skip escaping.* The literal fold has shipped since the
> first emitter; this page records the exact contract for the other two
> subsets — the **provably-scalar expression fold** and the
> **typed-parameter direct escape** — the same way
> `docs/rfc-alignment.md`, `docs/stream-alignment.md`,
> `docs/srcset-alignment.md` and `docs/ignore-directives.md` record their
> contracts, so the behavior is intentional and testable
> (`crates/lintl-core/tests/elision.rs`,
> `crates/lintl-core/tests/typed_elision.rs`).

## Source of truth

- §7.3: `(int)`/`(float)` casts and `count()` calls *skip escaping* — an
  int/float interpolates as digits (`NAN`/`INF` included) and cannot carry
  markup, so the `\Lintl\c()` call is pure overhead (§9.1: "everything
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
| `{(int)$n . $suffix}` | `\Lintl\c((int)$n . $suffix)` — the operator binds the cast's *result*; the child is a string |
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
   `\Lintl\a/u/srcset/cls/sty` because those helpers carry semantics a
   scalar fold would drop (bool/null handling, per-URL scheme checks,
   class/style list handling); spreads stay `\Lintl\attrs()`; components
   stay named arguments.
6. **Both markup targets share the fold.** The string target emits the raw
   expression as a concat operand, the stream target as a `yield` chunk;
   the `rfc` target was already helper-free and passes the expression
   through unchanged.
7. **Line preservation holds.** The elided expression is a dynamic part
   like `\Lintl\c(...)` and participates in the same newline
   interleaving; the `elided_children_preserve_line_numbers` test asserts
   the invariant end-to-end.

## The typed-parameter direct escape (`string` and `stream` targets)

The doc's remaining v0.1 bullet — *direct `htmlspecialchars` when the
expression is a typed parameter of the enclosing function* — folds on the
same rule: the declaration already proves the value is a string, so the
generic `\Lintl\c()` wrapper's runtime check is pure overhead.

| Source child (inside `function Card(...)`) | Lowered PHP |
|---|---|
| `string $title` declared, child `{$title}` | `\htmlspecialchars($title, \ENT_QUOTES \| \ENT_SUBSTITUTE \| \ENT_HTML5)` |
| `private string $title` (constructor promotion) | same — promotion modifiers are stripped from the type text |
| `STRING $title`, `string &$title`, `string $title = 'x'` | same — type names are case-insensitive, `&` is stripped, defaults do not change the type |
| `{$note}` with no `string $note` declared | wrapped — an undeclared name proves nothing |
| `$title` typed `int`/`bool`/`array`/`?string`/`string\|int`/`\App\Name` | wrapped — only exactly-`string` declarations qualify |
| `string ...$rest`, child `{$rest}` | wrapped — the bare name is an array |

Details the doc does not pin down, decided here:

8. **The scope must still prove the name.** The scanner tracks one
   `scan::FunctionScope` per declared function (and per braced closure,
   whose parameters are parsed for the fold but registered nowhere) while
   it scans; a JSX root lowers against the innermost scope, and a name is
   **unproven** the moment the body can rebind it: an assignment shaped
   token after the name (`=`, compound `+=`/`.=`/…, `??=`, shift
   assignment, `[`), a `global`/`static`/`unset`/`list`/`as`
   (`foreach … as`) construct in flight, or destructuring
   (`[$title] = $pair;`). Statements that merely *read* the name
   (`$title . 'x'`, `$map[$title]`, array literals ending in `];`) do not
   unprove it.
9. **Dynamic binding clears the whole scope.** `extract`, `$$name`
   variable-variables, `include`/`require` (they share the function's
   variable scope) and `eval` can rebind arbitrary names, so the scope's
   proven set is dropped instead of trying to enumerate what moved.
10. **Arrow functions clear the enclosing scope.** `fn(...) =>` bodies are
    braceless expressions, so the arrow's own parameters cannot be scoped;
    their names may shadow anything, and the enclosing scope's proven set
    is cleared conservatively. Braced closures are scoped properly: a
    closure's `string`-typed parameters elide inside it, its untyped
    parameters do not, and the enclosing function's elisions are untouched.
11. **Only child expressions fold, on both markup targets.** Attribute
    values keep `\Lintl\a/u/srcset/cls/sty` (rule 5), component arguments
    stay named arguments, the `rfc` target stays helper-free, and the
    emitted call participates in the same line-preservation interleaving.
12. **The declaration, not the call site, is the proof.** A
    `string $title` parameter is a string at every use in the body (PHP
    rejects `null` for a non-nullable parameter); what the compiler cannot
    know — shadowing, rebinding, dynamic binding — is exactly what the
    scope rules above unprove. A false negative costs one `\Lintl\c()`
    call at render time; a false elision would fold a rebound value.

## What the compiler does not do yet

- **PHPStan-backed elision** (`int|float|bool|numeric-string|literal`) is
  now the shipped `lintl analyze` path — `docs/type-elision.md` records
  the contract, the per-rule proofs and the elision-rate report; `bool`
  deliberately keeps the helper (render parity, recorded there). The
  `fn`-arrow bodies' own typed parameters still wait (rule 10).
- **No runtime change.** The PHP runtime (T-07) is unaffected: the emitted
  calls shrink, the `\Lintl\c()` helper itself stays for every wrapped
  child.
