# Cross-file signatures: the declaration-merge seam

> Architecture doc §17.1 scopes the signature table to one file ("that is
> the only place the parameter list is knowable"), and
> `docs/component-inventory.md` records the consequence: prop checks for
> components declared in other files are impossible, so `LNT0203`/
> `LNT0204`/`LNT0205` fire only in-file. Real projects split components
> across files, though — an editor integration validating the open buffer
> wants the project's declarations, not the file's. This page records the
> exact contract of `scan::ScanContext::merged_with`, the seam that unions
> two files' declaration tables without changing any default behavior.
> Testable in `crates/lintel-core/tests/cross_file.rs`, demoed by
> `examples/cross_file.rs`, benched by the `cross_file` group.

## Source of truth

- `crates/lintel-core/src/scan.rs` — `ScanContext::merged_with`, next to
  the tables it merges (`declared`, `signatures`, `suppressions`,
  `file_suppressed`).
- `crates/lintel-core/src/validate.rs` — `validate_with`, the consumer
  that reads the merged tables (no rule changed: the same
  `ScanContext::signature_of` / `declares_function` lookups simply
  resolve more names).

## Exact contract

1. **The base file wins.** `base.merged_with(&foreign)` unions the
   declared-function set and the signature table with the base (the file
   being validated) winning case-insensitive name collisions — PHP
   function lookups are case-insensitive, so the collision rule is too —
   and keeping the base's declaration order, with the foreign file's new
   entries appended in their own declaration order. The merge is
   documented non-commutative: it resolves lookups the way PHP resolves a
   call, in the file the call appears in.
2. **Suppression state never merges.** The `lintel-ignore-next` windows
   are byte ranges of *their own* source, and the `lintel-ignore-file`
   flag silences one file; neither fact is meaningful in another file's
   coordinate space, so the merged context starts suppression-clean
   (`docs/ignore-directives.md` is untouched — directives stay a per-file
   contract).
3. **Read-only on both inputs.** The merge clones and appends; neither
   context mutates, and the merged value owns its tables.
4. **Opt-in, zero default behavior change.** `compile`, `compile_with`
   and `validate_with` never merge anything on their own — the built-in
   same-file boundary stands until a consumer explicitly validates against
   a merged context. `merged_with` maps two contexts to one; it cannot
   fail and returns no diagnostics.
5. **What it unlocks.** With the merged context, `validate_with` (and any
   consumer rule reading `signature_of`) resolves foreign declarations:
   `LNT0201` shadow detection sees the foreign declaration,
   `LNT0203`/`LNT0205` check foreign prop lists, `LNT0204` checks foreign
   `children` parameters — exactly the in-file checks, over the union.
   The inventory's `declared_in_file` flag still reports the *scan-time*
   truth for one file; a merged table is the consumer's validation aid,
   not a replacement for the per-file report.
6. **Known limits.** The merge is name-based, like PHP's own
   function tables: two files declaring the same-named function with
   different parameter lists cannot both win (the base's list is used,
   PHP would fatal at load time anyway); `use function` imports are
   recorded name-only by the scanner, so they union as declarations
   without parameter lists.

## What the seam does not do yet

- **No project loader.** The consumer scans each file and folds the
  contexts; there is no `lintel.json` manifest or directory loader
  (architecture §17 mitigation, T-31) yet.
- **No CLI surface.** `build`/`check`/`stats` stay per-file on purpose:
  the merge is a library seam for IDE-style consumers first, and a CLI
  project mode needs caching and file-watch semantics that do not exist.
- **No cross-file diagnostic attribution.** A diagnostic still anchors in
  the file being validated; the foreign file's own mistakes are its own
  scan's business.
