# Component inventory: the read-only shape-report seam

> Architecture doc §6.2 defines the AST and §17.1 the signature scanning;
> consumers that describe a file (LSP outlines, metrics, dashboards, the
> CLI's `lintl stats`) need the declared/invoked component lists derived
> from the compiler's own data. This page records the exact contract of
> `inventory::collect_inventory` and its types (re-exported at the crate
> root) the way `docs/node-visitor.md` records the traversal contract.
> Testable in `crates/lintl-core/tests/inventory.rs`, demoed by
> `examples/inventory.rs`, benched by the `inventory` group.

## Source of truth

- `crates/lintl-core/src/inventory.rs` — `collect_inventory`,
  `FileInventory`, `DeclaredComponent`, `Invocation` and the public
  `InventoryBuilder` (the `NodeVisitor` impl the walk uses).
- `crates/lintl-core/src/scan.rs` — `ScanContext::declared_signatures`
  (the declaration table the report reads) and `Signature`, whose fields
  the declared entries carry verbatim.
- `crates/lintl-core/src/ast.rs` — `Attr::name()`, the accessor the prop
  list reads (a spread carries no name and never appears in `props`).

## Exact report contract

1. **Two lists, both deterministic.** `declared` is the scan context's
   signature table in declaration order; every declared function is listed
   (helpers included — the component-shaped subset is the consumer's
   filter), and the name is the function's **last path segment in the
   author's spelling** (PHP function lookups stay case-insensitive; the
   table key now keeps what the author wrote). `invocations` is every
   component-shaped JSX element (capitalized first letter or containing
   `\`) over all roots, **sorted by absolute source offset**.
2. **The offset convention is the diagnostics convention.** An
   `Invocation::offset` is the absolute byte offset of the invocation's
   opening `<` — the same anchor every `Diagnostic` carries — so
   `SourceMap` maps it to 1-based line / character column without
   re-scanning.
3. **Props are the named attributes in source order.** Static, boolean and
   dynamic attributes contribute their names; a spread carries no name, so
   it is reported through the `spread` flag instead (it may pass arbitrary
   named arguments). `children` reports JSX children; `declared_in_file`
   reports whether the signature table knows the component (`false` for
   cross-file components, where prop checks are impossible — the same
   boundary `LNL0203`/`LNL0205` respect).
4. **Suppression plays no part.** The report is not a diagnostic: the
   `lintl-ignore-next` / `lintl-ignore-file` contracts
   (`docs/ignore-directives.md`) silence validation output, and a shape
   report stays complete. The walk never consults `ScanContext`'s
   suppression state.
5. **Read-only and infallible.** `collect_inventory` maps `(&[Node],
   &ScanContext)` → `FileInventory`; it cannot fail and has no error
   channel, like the `NodeVisitor` walk it is built on. Parse errors are
   the caller's concern: a scan with diagnostics carries partial roots, and
   the CLI's `stats` command treats parse diagnostics as a file failure
   before any inventory is taken.
6. **`to_json(src)` is one stable JSON line** — the `Diagnostic::to_json`
   contract, applied to the report: no trailing newline, field order
   `declared` (entries: `name`, `params`, `required`, `variadic`,
   `typed_strings`) then `invocations` (entries: `name`, `offset`, `line`,
   `col`, `props`, `children`, `spread`, `declared_in_file`),
   renderer-parity positions from `SourceMap` (out-of-range offsets clamp,
   so emission never panics) and the shared `lintl_diag::json_escape`
   escaping. An empty report is `{"declared":[],"invocations":[]}`.
7. **The builder is public.** `InventoryBuilder::new(&context)` is a
   `NodeVisitor` impl a consumer can fold into its own walk (record
   invocations while doing something else), finishing with
   `InventoryBuilder::finish` in visit order; `collect_inventory` is the
   sorted, signature-complete wrapper.

## What the seam does not do yet

- **No HTML-element or fragment inventory.** The report lists components
  only; an element-level shape report (tag census, attribute coverage) is
  future work if a consumer asks.
- **No cross-file resolution.** `declared_in_file` is the only link
  between an invocation and a declaration; imports of components from
  other files are not resolved (the compiler itself cannot check them —
  doc §17.1's same-file boundary).
- **No usage/definition matching beyond the name.** The report does not
  pair invocation sites with their declared signature objects; a consumer
  maps `name` → `declared[i].name` case-insensitively, exactly like
  `ScanContext::signature_of`.
- **No re-export/import awareness.** `use function` imports are recorded
  name-only by the scanner (no parameter list exists to record), so they
  never appear in `declared`.
