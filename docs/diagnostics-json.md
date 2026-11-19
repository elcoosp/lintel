# JSON diagnostics: the machine-readable emission contract

> Architecture doc §6.7 shapes the CLI, and §13 pins the quality system:
> diagnostics must be parseable by machines as well as humans. The
> dependency-free renderer (`Diagnostic::render`) is the human contract;
> this page records the exact machine contract — `Diagnostic::to_json` and
> the CLI `--format=json` flag — so a CI job, editor pipe or LSP-style
> consumer never scrapes rendered snippets. Testable in
> `crates/lintel-diag` (inline unit tests) and
> `crates/lintel-cli/tests/cli.rs`.

## Source of truth

- `crates/lintel-diag/src/lib.rs` — `Diagnostic::to_json`, the
  single-object emitter; `SourceMap` supplies the position fields with
  the exact semantics `Diagnostic::render` has always used.
- `crates/lintel-cli/src/lib.rs` — `--format=text|json` wiring on `build`
  and `check`; the sinks, exit codes and walk order are unchanged.

## Exact emission contract

`to_json(src)` returns **one line of JSON** (no trailing newline), and the
CLI writes one object per diagnostic followed by `\n` (JSON Lines):

```
{"code":"LNT0301","severity":"error","message":"dynamic event-handler attribute `onclick` is not allowed","offset":21,"line":2,"col":16,"help":"use `data-action={...}` …","docs":"https://lintel.sh/e/LNT0301"}
```

Details the docs above do not pin down, decided here so the behavior is
intentional and testable:

1. **Stable field order.** `code`, `severity`, `message`, `offset`, `line`,
   `col`, `help`, `docs` — the order is part of the contract so consumers
   can diff or snapshot; new fields may be appended, existing ones never
   renamed or removed.
2. **Renderer-parity positions.** `line` (1-based, counted on `\n`) and
   `col` (1-based, counted in characters) come from the public
   `SourceMap`; out-of-range or mid-character offsets clamp first, so
   emission can never panic. `offset` stays the raw byte offset.
3. **Severity spells the renderer's words.** `severity` is `error` or
   `warning` — the same strings `Severity::prefix` renders — so a JSON
   consumer and the human renderer agree on classification.
4. **`help` is `null` when absent.** There is no empty-string sentinel; an
   object without advice has `"help":null`.
5. **Escaping is JSON-minimal.** Strings escape `"` → `\"`, `\` → `\\`,
   and control characters → `\n`, `\r`, `\t` or `\u00xx`; everything else
   passes through as UTF-8. No crate dependency is added: the emitter is
   hand-rolled like the renderer, keeping `lintel-diag` dependency-free.
6. **Exit codes do not change.** `--format=json` changes only the
   rendering on the error sink: `check` still exits 1 on error-severity
   diagnostics, `build` still writes compiled PHP to stdout / `--out` /
   `--in-place`, warnings stay advisory, and usage errors (`--format=xml`,
   a missing value) still return the exit-2 `CliError` path without
   touching the sinks.
7. **Both commands share the flag.** `build --format=json` renders
   warnings *and* rejections as JSON on stderr; `check` renders both
   severities, matching text format's behavior.

## What the seam does not do yet

- **No file names in the objects.** The CLI walks files in path order and
  renders each file's diagnostics against its source; a JSON consumer that
  needs the file name correlates by position or waits for a batch envelope
  (a candidate next step, not a shipped one).
- **No pretty printing or `serde`.** The output is one-line JSON Lines;
  the dependency-free stance of `lintel-diag` (crate docs) is unchanged.
- **No parse-error difference.** Parse diagnostics (`LNT01xx`) emit like
  every other diagnostic; they remain non-suppressible
  (`docs/ignore-directives.md`).
