# Lintel — JSX for PHP, compiled to plain PHP

Lintel is a compiler that lets you write PHP components with JSX-style
markup and compiles them to **plain PHP string concatenation**. The output
is readable PHP: auto-escaped by default, line numbers preserved, no virtual
DOM, no runtime past a handful of small helpers. It runs on PHP 8.1+ today
and lowers to the same markup shape the native JSX-in-PHP RFC for PHP 8.7/9
proposes, so a future `lintel migrate --to=native` is a lowering change, not
a rewrite.

```php
<?php

declare(strict_types=1);

function Card(string $title, string $body): Html
{
    return <div class="card">
        <h2>{$title}</h2>
        <p>{$body}</p>
    </div>;
}

echo Card(title: 'Hello', body: 'Plain PHP out, safe by default.');
```

compiles to:

```php
function Card(string $title, string $body): Html
{
    return new \Lintel\Html('<div class="card"><h2>'
 . \htmlspecialchars($title, \ENT_QUOTES | \ENT_SUBSTITUTE | \ENT_HTML5) . '</h2><p>'
 . \htmlspecialchars($body, \ENT_QUOTES | \ENT_SUBSTITUTE | \ENT_HTML5) . '</p></div>'
);
}
```

Everything the compiler can prove safe at compile time — integer and float
casts, `count()`, parameters declared as `string` — folds straight to the
cheap call. Everything else goes through the runtime helpers, which escape
by default and scheme-check every URL. Try the whole compiler in your
browser in the [playground](/playground/) — it runs as WebAssembly, no
server involved.

## Where to go next

- [Quick start](/quick-start/) — a runnable page in five minutes, from a
  git checkout (signed binaries and Packagist tags land with the release
  engineering card).
- [Language spec](/spec/) — the v1 subset: elements, attributes, children,
  components, the three lowering targets, line preservation.
- [Security model](/security/) — escape-by-default, the helper matrix, the
  rules that reject sinks instead of escaping them.
- [Benchmarks](/benchmarks/) — the reproducible §9.2 harness, the committed
  raw JSON every number comes from, and the publishing rule.
- [Error index](/errors/) — every `LNTxxxx` diagnostic, served exactly at
  the path the compiler prints.
