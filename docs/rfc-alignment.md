# RFC alignment: the `rfc` lowering target

> Task T-00 (architecture doc §15) requires the exact lowering names for
> `--target=rfc` to be recorded before the target ships. This page is that
> record for `lintel-core` v0.2 work.

## Source of truth

The architecture doc (§0.1, §4.9, §5.3) records the verified properties of
the **Native Markup Expressions** RFC (Liam Hammett, opened 2026-07-14,
under discussion, target PHP 8.7/9.0, php-src PR #22661):

- Lowercase tags are HTML elements; capitalized tags are components.
- Markup lowers at compile time to
  `new \Markup\Element('button', ['class' => 'btn'], ['Sign in'])`.
- Fragments `<>…</>` exist in the RFC syntax.
- Components are called with named arguments.
- Escaping is the runtime's contract (escape-by-default).

## Exact lowering contract (`Target::Rfc`)

| Source | Lowered PHP |
|---|---|
| `<b class="x">hi</b>` | `new \Markup\Element('b', ['class' => 'x'], ['hi'])` |
| `<Card title="Hi" />` | `Card(title: 'Hi')` |
| `<Card title="Hi"><p>Body</p></Card>` | `Card(title: 'Hi', children: [new \Markup\Element('p', [], ['Body'])])` |
| `<div data-id={$id} disabled {...$props} />` | `new \Markup\Element('div', ['data-id' => $id, 'disabled' => true, ...$props], [])` |
| `<>a{$x}b</>` (root fragment) | `['a', $x, 'b']` |
| `<br />` | `new \Markup\Element('br', [], [])` (empty children; voids render themselves) |

Details the doc does not pin down, decided here so the behavior is
intentional and testable (`crates/lintel-core/tests/rfc_target.rs`):

1. **No `\Lintel\` helpers.** The `rfc` target emits zero `\Lintel\*()`
   calls: dynamic children/attributes pass through as raw PHP expressions
   because escaping under the RFC model is the runtime's job
   (escape-by-default). The string target keeps the context-aware helpers.
2. **No value wrapper.** A root element lowers to the `\Markup\Element`
   constructor call itself; a root fragment lowers to a plain PHP array of
   the child values. The RFC's `\Markup\Html` value type is a runtime
   concern outside the compiler's output shape.
3. **Literal folding is preserved.** Simple single-quoted child expressions
   (`{'text'}`) and integer children fold to PHP literals in both targets;
   folding is a compile-time optimization, not runtime behavior.
4. **Fragments flatten.** A nested `<>…</>` splices its children into the
   enclosing children list (same shape as the string target's lowering);
   only a *root* fragment becomes a bare array.
5. **Author-trusted text stays verbatim.** Text children are emitted as PHP
   string literals of the normalized source text, exactly like the string
   target's author-trust rule (doc 4.6: entities pass through).
6. **Line preservation holds.** The RFC expression is newline-padded to the
   source span it replaces; `rfc_target_preserves_line_count` asserts the
   invariant end-to-end.
7. **Spread syntax.** `...$expr` inside the attributes array (PHP 8.1+
   string-keyed array spread) and `...$expr` in component argument lists
   (argument unpacking, doc 4.7).

## Migration path

`Target::Rfc` is the lowering that `lintel migrate --to=native` (doc §0.1)
will emit once the RFC stabilizes. Recompiling a file with
`Options { target: Target::Rfc }` produces RFC-shaped output today, which
keeps the migration rehearsal (doc §4.9) runnable against real code.
