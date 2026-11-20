# The T-13 playground (task card: WASM + Playground)

The Lintel compiler running in the browser as WebAssembly: write JSX-in-PHP on
the left, watch the compiled plain PHP appear on the right, diagnostics panel
below, examples dropdown up top, shareable permalinks (`#src=…&t=…`) and a
"copy compiled" button.

**Acceptance (T-13, verbatim):** *loads <1 MB gz; works offline after first
load.*

- The size budget is asserted by `crates/lintel-wasm/tests/size_budget.rs`:
  the `wasm32-unknown-unknown` bundle (built `opt-level=z` + fat LTO,
  `strip = true`) must gzip to under 1 MB — as must the whole page payload.
- The page makes zero external requests; `sw.js` precaches every asset, so
  after the first load the page runs with the network off.

## Serve it

```sh
php -S localhost:8080 -t playground/
```

then open <http://localhost:8080/playground/>. Any static file server works
(no build step, no server code) — `php -S` is what CI and the local gate use.

## Rebuild the committed wasm bundle

The generated bindings under `playground/assets/` are committed; rebuild them
after touching the compiler or the crate:

```sh
make playground     # cargo build --profile wasm-release --target wasm32-unknown-unknown
                    # + wasm-bindgen --target web into playground/assets/
```

Requirements: the `wasm32-unknown-unknown` rust target and a `wasm-bindgen-cli`
matching the `wasm-bindgen` version locked in `Cargo.lock` (the size-budget
test asserts that match). When you change any asset, bump the `CACHE`
constant in `sw.js` so already-installed service workers refresh.

## API surface

The crate exposes exactly two functions (`crates/lintel-wasm/src/lib.rs`):

- `compile_source(source, target)` → JSON envelope
  `{ok, php, diagnostics, error}`; each diagnostic carries the machine fields
  of `docs/diagnostics-json.md` plus a rendered form for the panel.
- `lintel_version()` → the compiler version the footer shows.

The target words are the CLI's: `string` (default), `stream`, `rfc`.
