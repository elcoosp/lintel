# Ignore directives: `lintel-ignore-next`

> Architecture doc §17 lists the `// lintel-ignore-next` escape among the
> mitigations for scanner/lint false positives. This page records the exact
> directive contract for `lintel-core`, the same way `docs/rfc-alignment.md`
> and `docs/stream-alignment.md` record the lowering contracts, so the
> behavior is intentional and testable
> (`crates/lintel-core/tests/suppressions.rs`).

## Source of truth

The architecture doc (§17 Risks & mitigations, "Scanner misparses exotic
PHP" row) names `// lintel-ignore-next` as the escape hatch. Lintel v0.1/0.2
did not implement it; every diagnostic was unconditional. This page pins the
v0.2 contract.

## Exact directive contract

| Source | Effect |
|---|---|
| `// lintel-ignore-next` (line above the offending line) | validation diagnostics anchored on the following line are dropped |
| `# lintel-ignore-next` | same (`#` is a PHP line comment) |
| `// lintel-ignore-next: reason` | same; the reason text is ignored by the compiler |

Decisions the doc does not pin down, made here so the behavior is
intentional:

1. **The window is one physical source line.** The scanner records the byte
   range `[start, end)` of the line immediately after the comment's own line
   into `ScanContext` (a `scan::Suppression`). A diagnostic is suppressed
   when its offset falls inside a window (start inclusive, end exclusive),
   which covers attribute diagnostics anchored on the same line as the
   component tag.
2. **The directive must be a PHP line comment.** Block comments
   (`/* lintel-ignore-next */`) and occurrences inside strings or heredocs
   are data, not directives — the scanner only inspects comment tokens it
   copies in `copy_line_comment`.
3. **The directive word is exact.** `lintel-ignore-next` must be followed by
   end-of-comment, whitespace, or `:` (the reason separator);
   `lintel-ignore-nextline` does not match.
4. **Only validation diagnostics are suppressible.** The filter runs in
   `validate`, the single choke point for `LNT02xx`–`LNT04xx` reports.
   Parse diagnostics (`LNT01xx`) abort the compile before validation and can
   never be silenced by a directive, so a suppression can never hide a
   misparse behind a clean exit.
5. **Two directives in a row are harmless.** Each registers a window over
   the line after it; with consecutive directive lines the effective window
   is the first code line after the last directive.
6. **A trailing directive with no following line is inert.** If the comment's
   physical line is the last line of the file (no trailing newline, nothing
   after), no window is registered.
7. **`?>`-terminated comments still work.** A line comment that PHP ends at
   `?>` mid-line still registers the window over the following physical
   line.

## Public API

- `lintel_core::scan::Suppression` — `{ start: usize, end: usize }`, the
  recorded window.
- `lintel_core::scan::ScanContext::add_suppression(start, end)` — record a
  window (empty ranges are ignored), mirroring `declare`/`declare_signature`.
- `lintel_core::scan::ScanContext::suppressed_at(offset)` — the lookup the
  validation pass filters with.
- `lintel_core::scan::ScanContext::suppressions()` — the recorded windows,
  in source order.

## Limitations

- The window is one line: a multi-line JSX root whose offending attribute
  sits on a *later* line than the opening tag needs its own directive above
  that line.
- There is no `lintel-ignore` (same line) or file-level form yet; add them
  only with a failing test and an update to this page.
- The directive is recognized in PHP line comments only, not in inline-HTML
  comments (`<!-- … -->`).
