# Stream alignment: the `stream` lowering target

> Architecture doc 5.3 names three lowering targets; `string` and `rfc` were
> shipped first, and §10 describes the streaming model. This page records the
> exact `Target::Stream` lowering contract for `lintel-core`, the same way
> `docs/rfc-alignment.md` records the `rfc` contract, so the behavior is
> intentional and testable (`crates/lintel-core/tests/stream_target.rs`).

## Source of truth

The architecture doc (§5.3 target table, §10 streaming model) specifies:

- `stream` compiles markup so that rendering produces **generators yielding
  string chunks** instead of one concatenated string.
- The same source must stay compilable to the sync (`string`) target; a
  component may therefore return either shape at runtime.
- Line preservation (doc 13.4) holds for every target.

## Exact lowering contract (`Target::Stream`)

| Source | Lowered PHP |
|---|---|
| `<b class="x">{$name}</b>` | `(static function (): \Generator { yield '<b class="x">'; yield \Lintel\c($name); yield '</b>'; })()` |
| `<Card title="Hi"><p>Body</p></Card>` | `(static function (): \Generator { yield \Lintel\c(Card(title: 'Hi', children: (static function (): \Generator { yield '<p>Body</p>'; })())); })()` |
| `<>a{$x}b</>` (root fragment) | `(static function (): \Generator { yield 'a'; yield \Lintel\c($x); yield 'b'; })()` |
| `<a href={$u} class={$c}>x</a>` | `… yield '<a'; yield \Lintel\u('href', $u); yield \Lintel\cls($c); yield '>x</a>'; …` |

Details the doc does not pin down, decided here:

1. **Root shape: an immediately-invoked generator closure.** The scanner
   replaces a JSX *expression*, so the streaming lowering must itself be an
   expression: `(static function (): \Generator { … })()`. The closure body
   runs lazily on first iteration; assigning the expression creates the
   generator object without rendering anything. Component functions that
   stream declare `: \Generator` and return the closure directly.
2. **Chunks mirror the string target's parts.** Literal runs merge into one
   quoted string chunk (one `yield` per literal run, same merge rule as the
   string concat, doc 6.5); dynamic children/attributes ride through the same
   context-aware `\Lintel\c/a/u/cls/sty/srcset/attrs` helpers. Escape-by-default
   (doc 4.4) is unchanged by streaming.
3. **Components stay wrapped in `\Lintel\c()`.** A component call is yielded
   as `\Lintel\c(Card(...))`: the runtime helper already renders `Html`
   values, strings and *iterables* recursively (doc 6.8), so a component may
   return either a `\Generator` (streaming chunks) or an `\Lintel\Html`
   (sync) without the caller changing. This is what makes §10's "the same
   source compiles to sync or stream" hold call-site by call-site.
4. **The `children:` argument is a generator closure.** Instead of
   `children: new \Lintel\Html(...)`, streaming components receive
   `children: (static function (): \Generator { … })()`, which `\Lintel\c()`
   renders by iteration inside the component body. Authors of streaming
   components type the parameter accordingly (e.g. `iterable $children`).
5. **Line preservation holds.** Newlines are interleaved between `yield`
   statements exactly as the string target interleaves them between `.`:
   operands, so the emitted expression contains as many newlines as the
   source span it replaces (`stream_target_preserves_line_count`).
6. **Nested JSX in expressions lowers recursively.** `{$ok && <i />}` lowers
   the inner root with the same injected strategy, so a branch yields a
   generator and `\Lintel\c()` renders it (generators are iterable, and a
   falsy guard short-circuits before the closure runs).

## The §10 flush runtime (php/runtime)

The runtime that consumes these chunks ships in `php/runtime/src/functions.php`:

- **`Lintel\stream(mixed $tree, ?callable $flush = null): string`** walks a
  render tree the way `c()` walks a `Traversable` — chunk strings are
  compiler-produced markup (or data already escaped at build time) and sink
  raw, `Html` is the trusted sink, null/bool render empty, scalars
  stringify, anything else is rejected. Each chunk reaches the `$flush`
  sink as it is produced; without a sink the bytes are captured (chunk
  order is still the flush order).
- **The `<Suspense>` boundary** (`new \Lintel\Suspense(fallback: …, work: …)`
  yielded where the boundary sits): the fallback renders through `c()` and
  flushes immediately inside `<div id="lt-N">…</div>`; the work runs in a
  fiber pumped round-robin once the shell finished, and each resolution
  flushes `<template id="lt-N-r">…</template><script>lintelSwap("lt-N")</script>`
  **out of order** — a fast sibling flushes before a slow earlier boundary.
  The ≈250-byte swap runtime (184 bytes measured) is emitted once per
  render with the first resolved boundary and replaces the placeholder in
  place (`replaceChildren(t.content.cloneNode(true))`). Resolved generators
  walk as markup streams (their chunks flush progressively; nested
  boundaries join the pump); every other resolved value is data and renders
  through `c()`, so **no streaming path bypasses the escapers** (threat
  cases in `php/runtime/tests/ThreatCorpusTest.php`).
- **Sync compat (§10 "a page works everywhere"):** a `Suspense` consumed
  outside the flush runtime — through `c()`, i.e. the sync target — runs
  inline: no fallback, the resolved value renders whole.
- **Flush transports** (`Lintel\FlushTransport`): `stream_via($tree, $transport)`
  wires `begin()` / per-chunk `write()` / `end()`. `CliFlushTransport`
  (echo + flush) and `FpmFlushTransport` (`X-Accel-Buffering: no` +
  buffered-output teardown + flush) ship; FrankenPHP streams through the
  same echo+flush shape, and the live RoadRunner/Swoole/OpenSwoole and
  ReactPHP/AMPHP adapters are CI-lane work.

## What this target does not do yet

- **Compiler-side boundary lowering.** The compiler emits plain generators
  and no swap-script scaffolding: boundaries enter the tree as
  `\Lintel\Suspense` values from hand-authored streaming components, and a
  boundary consumed through a `c()` call (a nested component rendered by
  the string-result `c(…)` chunk shape) runs inline by the sync rule
  above. Compiler emission of boundary scaffolding is future work.
- **Adapters beyond the transport seam** (live FrankenPHP, RoadRunner,
  Swoole/OpenSwoole, ReactPHP/AMPHP) need those runtimes — CI-lane work.
