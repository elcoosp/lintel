# Language spec (v0.1 subset)

The compiler implements the v1 language plan's core: JSX recognized at
expression starts, HTML elements and components, fragments, the attribute
matrix, children rules, and three lowering targets. This page is the
author-facing summary; the executable goldens in `crates/lintel-core/tests/`
are the source of truth.

## Where JSX may appear

JSX is recognized **only in expression-start position**: after `return`,
`echo`, `yield`, `=`, `=>`, `(`, `,`, and the other expression-start
operators. After an *operand* — a variable, literal, `)`, identifier,
number — `<` is the less-than operator. One rule resolves both of these
forever:

```php
$ok = $a < $b;          // less-than: `<` follows an operand
return <div>hi</div>;   // JSX: `<` starts an expression
```

PHP passes through byte-identical when it contains no JSX — the scanner's
pass-through contract is golden-tested over 60 cases (strings, heredocs,
comments, `?>`, attributes).

## Elements

Lowercase first letter means an HTML element; capitalized or
namespace-qualified (`<Ui\Button>`) means a **component**. Void elements
(`br`, `img`, `input`, … — the full list in §4.2) emit without a close tag;
a self-closed non-void `<div />` emits `<div></div>`.

```php
return <section class="hero" id={$id}>
    <img src={$src} alt="" />
    <h1>{$title}</h1>
</section>;
```

## Fragments

`<>…</>` groups children without a wrapper element. Under the string target
the fragment itself disappears; under the RFC target a fragment root lowers
to a plain PHP array of children.

## Attributes

| Form | Meaning |
|---|---|
| `name="str"` | static; folded at compile time, `"` re-escaped to `&quot;` |
| `name={expr}` | dynamic; `null`/`false` omit, `true` emits a bare attribute, everything else escapes |
| `name` (bare) | boolean `true` |
| `{...$attrs}` | spread; each entry runs the same attribute rules |
| `class={...}` / `style={...}` | list/map helpers (`\Lintel\cls` / `\Lintel\sty`) |
| `href` `src` `action` `poster` … | URL attributes → `\Lintel\u()`: scheme allowlist, blocked schemes become `about:invalid#blocked` |
| `srcset={...}` | per-URL candidate parsing → `\Lintel\srcset()` |
| `onclick={...}` | **compile error** (LNT0301) — dynamic handlers are an XSS sink, not an escaping problem; static handler strings compile with a warning (LNT0302) |

## Children

A `{expr}` child renders through the runtime's `c()`: `Html` values pass
through, strings escape, ints/floats cast, `null`/`bool` render empty,
iterables render recursively, `Stringable` escapes, anything else throws.
The compiler folds what it can prove at compile time: single-quoted string
and integer literals fold to literals, `(int)`/`(float)` casts and
`count()` calls skip the wrapper, and a child that is exactly a parameter
the enclosing function declares `string` compiles to a direct
`\htmlspecialchars(...)` call — the runtime check would be pure overhead.

Whitespace follows JSX's rules: line-leading and line-trailing whitespace
trims, blank lines drop, single-line spacing is preserved. The output
contains exactly as many newlines as the source it replaces — **line
numbers are preserved**, so PHPStan, Xdebug and stack traces all point at
your lines.

## Components

Attributes become **named arguments**; children become the named argument
`children`, passed only when present. Hyphenated attribute names
(`data-x`) cannot be PHP named arguments — the compiler rejects them
(LNT0202) and suggests the spread form. Component names that would shadow a
PHP builtin (`<Header>` vs `header()`) are rejected with LNT0201.

```php
function Card(string $title, ?Html $children = null, string $variant = 'plain'): Html {
    return <div class={['card', "card--$variant"]}>
        <h2>{$title}</h2>{$children}
    </div>;
}

echo <Card title="Hi" variant="wide"><p>Body</p></Card>;
```

Components declared in the same file (or merged from other files by a
consumer) get signature checking: unknown props (LNT0203), unexpected
children (LNT0204) and missing required parameters (LNT0205) are compile
time errors, using the PHP-declared signature, PHP named-argument
case-sensitivity included.

Raw-text elements (`<script>`, `<style>`) take verbatim bodies — no `{}`
interpolation, so JS and CSS braces are safe. `{/* comment */}` produces
nothing.

## Lowering targets

| Target | Output | Use it for |
|---|---|---|
| `string` (default) | one `new \Lintel\Html('…' . …)` per markup root | everywhere today |
| `stream` | an immediately-invoked `static function () use (...): \Generator` yielding string chunks | streaming responses; component children arrive as generator closures the component can iterate or `yield from` |
| `rfc` | `new \Markup\Element(...)` calls, no `\Lintel\` helpers | parity with the native RFC's shape |

The same source compiles to all three; a component may return a `\Generator`
or an `\Lintel\Html` and call sites render either, because `\Lintel\c()`
renders both shapes. The exact contracts live in
`docs/stream-alignment.md` and `docs/rfc-alignment.md`.
