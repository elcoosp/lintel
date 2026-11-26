# Ignore directives: `lintl-ignore-next` / `lintl-ignore-file`

> Architecture doc §17 lists the `// lintl-ignore-next` escape among the
> mitigations for scanner/lint false positives. This page records the exact
> directive contracts for `lintl-core`, the same way `docs/rfc-alignment.md`
> and `docs/stream-alignment.md` record the lowering contracts, so the
> behavior is intentional and testable
> (`crates/lintl-core/tests/suppressions.rs`).

## Source of truth

The architecture doc (§17 Risks & mitigations, "Scanner misparses exotic
PHP" row) names `// lintl-ignore-next` as the escape hatch. Lintl v0.1/0.2
did not implement it; every diagnostic was unconditional. This page pins the
v0.2 contract, together with the file-level `lintl-ignore-file` sibling
that silences a whole file (generated pages, audited legacy markup) while
keeping parse errors fatal.

## Exact directive contract

| Source | Effect |
|---|---|
| `// lintl-ignore-next` (line above the offending line) | validation diagnostics anchored on the following line are dropped |
| `# lintl-ignore-next` | same (`#` is a PHP line comment) |
| `// lintl-ignore-next: reason` | same; the reason text is ignored by the compiler |
| `// lintl-ignore-file` (anywhere in the file) | **every** validation diagnostic in the file is dropped, errors and warnings alike |
| `# lintl-ignore-file` / `// lintl-ignore-file: reason` | same; the reason text is ignored by the compiler |

Decisions the doc does not pin down, made here so the behavior is
intentional:

1. **The `ignore-next` window is one physical source line.** The scanner
   records the byte range `[start, end)` of the line immediately after the
   comment's own line into `ScanContext` (a `scan::Suppression`). A
   diagnostic is suppressed when its offset falls inside a window (start
   inclusive, end exclusive), which covers attribute diagnostics anchored
   on the same line as the component tag.
2. **The directive must be a PHP line comment.** Block comments
   (`/* lintl-ignore-next */`) and occurrences inside strings or heredocs
   are data, not directives — the scanner only inspects comment tokens it
   copies in `copy_line_comment`.
3. **The directive word is exact.** `lintl-ignore-next` / `lintl-ignore-file`
   must be followed by end-of-comment, whitespace, or `:` (the reason
   separator); `lintl-ignore-nextline` and `lintl-ignore-filex` do not
   match.
4. **Only validation diagnostics are suppressible.** The filters run in
   `validate`, the single choke point for `LNL02xx`–`LNL04xx` reports.
   Parse diagnostics (`LNL01xx`) abort the compile before validation and can
   never be silenced by a directive, so a suppression can never hide a
   misparse behind a clean exit — file-level included.
5. **Two `ignore-next` directives in a row are harmless.** Each registers a
   window over the line after it; with consecutive directive lines the
   effective window is the first code line after the last directive.
6. **A trailing `ignore-next` with no following line is inert.** If the
   comment's physical line is the last line of the file (no trailing
   newline, nothing after), no window is registered.
7. **`?>`-terminated comments still work.** A line comment that PHP ends at
   `?>` mid-line still registers the window over the following physical
   line.
8. **The file-level flag is position independent.** `lintl-ignore-file`
   suppresses the whole file from wherever the comment sits — top of file
   by convention, but a directive on the last line works too (where an
   `ignore-next` window would be inert). The scanner latches a flag on
   `ScanContext` instead of recording a window.
9. **The file-level flag short-circuits the rule walk.** `validate_with`
   returns before running any rule when the flag is set, so a suppressed
   file pays no lint cost; there is nothing to filter because nothing ran.
   Custom registries inherited through `compile_with` are short-circuited
   the same way.
10. **There is no opt-back-out.** One `lintl-ignore-file` latches for the
    whole file; repeated directives are idempotent and no `lintl-ignore-restore`
    (or per-line re-enable) form exists. Files that need most — but not
    all — diagnostics kept should use per-line `ignore-next` windows
    instead.

## Public API

- `lintl_core::scan::Suppression` — `{ start: usize, end: usize }`, the
  recorded `ignore-next` window.
- `lintl_core::scan::ScanContext::add_suppression(start, end)` — record a
  window (empty ranges are ignored), mirroring `declare`/`declare_signature`.
- `lintl_core::scan::ScanContext::suppressed_at(offset)` — the lookup the
  validation pass filters with.
- `lintl_core::scan::ScanContext::suppressions()` — the recorded windows,
  in source order.
- `lintl_core::scan::ScanContext::suppress_file()` — latch the file-level
  `lintl-ignore-file` flag (idempotent).
- `lintl_core::scan::ScanContext::file_suppressed()` — whether the flag is
  latched; `validate_with` short-circuits on it before the rule walk.

## Limitations

- The `ignore-next` window is one line: a multi-line JSX root whose
  offending attribute sits on a *later* line than the opening tag needs its
  own directive above that line.
- There is no `lintl-ignore` (same line) form yet; add it only with a
  failing test and an update to this page.
- There is no opt-back-out of a file-level directive (decision 10); a
  directive suppresses the entire file's validation diagnostics.
- The directives are recognized in PHP line comments only, not in
  inline-HTML comments (`<!-- … -->`).
