# Type-driven escape elision: the `lintl analyze` path (§7.3-Y2)

> Architecture doc §7.3 plans the Y2 elision: "a `lintl analyze` pass using
> PHPStan's type info (via the PHPStan extension) provides exact types →
> elide escaping where the type is `int|float|bool|numeric-string|literal`.
> Benchmarks must report elision rate." This page records the shipped
> contract: the pass, the rules, what each rule must prove, and the one
> allowlist kind that deliberately does not elide.

## The path

1. **The PHPStan bridge resolves the types** (`php/lintl-phpstan`):
   `bin/analyze <source.lintl.php>` prepares the source through the same
   compiler binary the CLI uses (`Bridge::prepare` — artifact + metadata
   sidecar + signature stub), runs `phpstan analyse` with the
   `LintlTypesCollector` rule over the prepared cache, and writes the
   types report to `<source>.lintl-types.json` (or `--out PATH`). The
   collector classifies every `\Lintl\c()` argument's type through
   PHPStan's analyser — the signature stubs carry the scanner's proven
   facts and the consumer's own types/docblocks narrow the rest.

2. **The compiler consumes the report** (`crates/lintl-core`):
   `lintl analyze <sources> [--types PATH]` reads the report (the default
   sidecar beside each source, or one explicit file for every input),
   resolves each hint onto a child expression by BOTH the absolute source
   line and the exact expression text (the artifact is line-preserved, §5.2,
   so the report's per-line entries and the source agree 1:1; the lowering
   emits the trimmed source text verbatim inside `c(...)`), and lowers
   through [`compile_analyzed`] with the resolved decisions.

3. **The elision ladder is ordered, never duplicated**: the v0.1 folds run
   exactly as before (typed-parameter direct escape, single-quoted and
   integer literal folds, `(int)`/`(float)`/`count()` folds) and the
   analyze decision applies ONLY to children those folds could not prove.
   An empty or absent report compiles byte-identical to `lintl build`
   (asserted by `tests/analyze.rs`).

## The rules and their proofs

Each elision rule is byte-identical in output to what `\Lintl\c()`
renders for the same value — the fold changes cost, never markup:

- **`int` / `float`** — PHP's parameter type system enforces the declared
  `int`/`float` at call time (a non-numeric string throws even in weak
  mode), so the value at render time is digits and cannot carry markup;
  the raw expression interpolates exactly as `(string) $v`, which is what
  `c()` does. Golden: `AnalyzeElisionTest`'s compiled goldens; threat
  case: the attacker sibling stays escaped while the int/float children
  render digits.
- **`numeric-string`** — `is_numeric` strings are `[0-9.eE+-]` plus
  whitespace: none of `<>&"'`, so `htmlspecialchars` is the identity on
  them. PHPStan proves the kind from a `string`-ish value narrowed by an
  `is_numeric` guard or a documented `numeric-string` shape — both paths
  pass through level-max type checks, and the corpus case executes the
  guarded artifact with attacker payloads (markup fails the guard and
  takes the rejected branch).
- **`literal`** — a single constant string the analysis fixed at analysis
  time. Elision inlines the ESCAPED value (`htmlspecialchars` semantics,
  the v0.1 literal fold's function), byte-identical to `c()`'s rendering
  of that value, and attacker-uncontrollable by construction: the value is
  baked into the artifact, not read at runtime. A multi-line value never
  inlines (line preservation, doc 13.4).

### The `bool` fold (M3)

`c()` renders every bool as `''` — both `true` and `false` — while a raw
interpolation renders `'1'`. The shipped fold emits
`(($expr) ? '' : '')`: the ternary **evaluates the expression** (side
effects ride along) and emits the constant `''`, byte-identical to
`c()`'s output for both truth values. Nothing runtime-controlled reaches
the markup — the emitted form has only two constant branches — so even a
mis-proven type renders empty, never its content (stronger than the
int/float rules, whose raw interpolation trusts the type at render time).
A multi-line expression stays wrapped (the one-line ternary would shift
the artifact's lines, doc 13.4). Goldens: `tests/analyze.rs` (the emitted
shape, the multi-line guard, the mis-proven shape) and
`AnalyzeElisionTest::testBoolFoldIsParitySafeSideEffectPreservingAndMisProofSafe`
(execution: parity, side effects, and the lying-report threat case).

## The report contract

One JSON object per source:

```json
{
    "version": 1,
    "source": "/abs/path/Page.lintl.php",
    "children": [
        {"line": 9, "expr": "$count", "type": "int"},
        {"line": 12, "expr": "MODE", "type": "literal", "value": "AT&T"}
    ]
}
```

- `line` — the child's absolute 1-based source line (== the artifact line).
- `expr` — the child's expression text exactly as `c(...)` carries it in
  the artifact (the trimmed source text).
- `type` — one of `int`, `float`, `numeric-string`, `literal` (with
  `value`), or a non-elidable bucket (`bool`, `string`, `mixed`, …). The
  report names every `c()` argument; the compiler's allowlist decides what
  folds.
- Unknown `type` strings fail the analyze compile loudly — a
  producer/consumer contract mismatch must never degrade silently.
- Conflicting entries (same line + text, different kinds) poison the key:
  the child keeps the helper.

## What the compiler guarantees

- **Conservative by default**: a missing report, an unmatched hint (stale
  line/text), an ambiguous entry, or an unproven kind ALL keep
  `\Lintl\c()`. A false-positive fold would smuggle markup into the
  output; false negatives cost one helper call.
- **Attributes never elide** — only child expressions (`docs/escape-elision.md`
  rule 5); `u()`/`a()`/`srcset()`/`cls()`/`sty()` carry context contracts
  (scheme allowlists, boolean semantics), not just escaping.
- **Line preservation holds** (doc 13.4): the raw-expression folds emit
  the expression text itself; the literal fold requires matching newline
  counts before inlining.
- **The `rfc` target is unaffected** — it emits no `\Lintl\` helpers by
  contract (`docs/rfc-alignment.md`), so decisions are inert there.

## The CLI

```
lintl analyze <paths>... [--types PATH] [--target=string|stream|rfc] [--format=text|json]
```

Compiles like `build` (PHP to stdout), reports the §7.3-Y2 elision
counters on stderr: `children` (every dynamic child lowered on the markup
targets), `considered` (how many reached the analyze decision step past
the v0.1 folds) and `elided` (how many folded). Text prints one summary
line per file with the elision rate; `--format=json` emits one object per
file with a `rate` field (`elided/considered`, null when nothing was
reachable). That report is the "benchmarks must report elision rate"
acceptance: the `analyze_children` bench group drift-checks the same
counters (`crates/lintl-core/benches/compile.rs`).

## The stale surface (M2: the analyze UX)

A report entry that matches no child expression of the current source is
STALE — the analysis ran against an older artifact. `lintl analyze` counts
them in its report (`stale` in JSON, `N stale hint(s) ignored` in text),
and `lintl check <paths> --types PATH` renders each one as an advisory —
one `check <file>: stale analyze hint at line N: expr (type type)` line, or
a JSON object with `"stale": true`. Advisories never change the exit code:
elision already ignores the entries, the seam only makes the degradation
visible (`lintl_core::stale_hints`, tested by `tests/stale_hints.rs`).

## Documented limitations

- **Sibling roots are not analyzed together.** Two JSX roots at statement
  level (`return <p>…</p><span>…</span>;`) pass through verbatim (the
  scanner lowers one root per expression start; pre-existing behavior,
  §21.3's neighbor) — the second element never becomes a child and so
  never elides. A fragment root (`<>…</>`) is the supported shape.
- **`numeric-string` trusts the type surface.** The analyze pass consumes
  PHPStan's types; a `numeric-string` documented without a runtime guard
  is the author's claim, kept honest by level-max type checks (a plain
  string flowing into a `numeric-string` slot is a PHPStan error the
  bridge's diagnostics rule surfaces). The corpus case pairs the docblock
  with a body guard so the runtime behavior is proven, not assumed.
- **Multi-line child expressions** match hints by their first line; the
  raw-expression folds preserve their internal newlines, and a literal
  with a newline-count mismatch stays wrapped.
