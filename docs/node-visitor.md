# Node visitor: the read-only traversal seam

> Architecture doc §6.2 defines the AST, and §5.2's compile model is
> scan → validate → emit. The validation pass has always walked the tree
> one way; this page records the exact public traversal contract —
> `ast::NodeVisitor`, `ast::visit_node`, `ast::visit_roots` (re-exported at
> the crate root) — so LSP features, metrics collectors and doc generators
> walk the compiler's tree instead of re-implementing the recursion.
> Testable in `crates/lintel-core/tests/visitor.rs`.

## Source of truth

- `crates/lintel-core/src/ast.rs` — the `NodeVisitor` trait and the two
  walk functions, next to the `Node` types they traverse.
- `crates/lintel-core/src/validate.rs` — the private `walk` the seam
  mirrors; rules see the same node sequence.

## Exact traversal contract

1. **Top-down, depth-first, source order.** `visit_node` calls
   `NodeVisitor::visit` on the node first, then recurses into children in
   source order. `Element` and `Fragment` recurse into their children;
   `Text` and `Expr` are leaves and are single visits.
2. **Exactly once per node over one scan.** `scan()`'s root trees are
   disjoint: an element's children hold their own subtrees, while JSX
   embedded inside a PHP expression (an attribute value like
   `avatar={<img />}`, or any expression-start JSX) is a separate root in
   `Parsed::roots`. `visit_roots(&parsed.roots, …)` therefore visits every
   node exactly once — the same guarantee `validate_with`'s walk relies on
   to never double-report.
3. **Attributes are not separate visits.** `Attr` values live on
   `Element::attrs` and are inspected when the owning element arrives,
   exactly the way a `ValidationRule` reads its element. This keeps the
   visitor a `Node`-shaped walk and the trait a single method.
4. **Read-only and infallible.** The visitor receives `&Node` and
   accumulates state through `&mut self`; the walk cannot fail and has no
   error channel. A visitor that *reports* belongs on the
   `ValidationRule` seam, where diagnostics flow through the severity
   split and the `lintel-ignore-*` choke point (`docs/ignore-directives.md`).
5. **Line/offset fields are honored.** Every node carries the absolute
   byte `offset` fields (doc §6.2's documented extension), so a visitor can
   map positions through `lintel_diag::SourceMap` without re-scanning.

## What the seam does not do yet

- **No mutation or rewrite support.** The compiler's lowering is a
  `Lowering` strategy over an immutable tree (doc 5.3); a mutable-visitor
  API would be a different seam and is not planned for v0.x.
- **No post-order callbacks.** The contract is the validation walk's
  top-down shape; an `after_children` hook would extend the trait and the
  built-in walk together, if a consumer ever needs bottom-up folding.
- **No attribute walk of its own.** Spreads and named attributes stay on
  the element visit; a dedicated attribute traversal is future work if an
  LSP consumer asks for hover targets per attribute.
