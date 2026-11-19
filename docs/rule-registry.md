# Rule registry: the self-describing validation-rule seam

> Architecture doc §4.4/§18 pin the diagnostic contracts (stable `LNTxxxx`
> codes, a `help:` line on every diagnostic), and §21.1 lists the
> `docs/errors/LNT*.md` index pages as not yet written. Consumers —
> docs-site generators, editor integrations, CI reporters — need that
> knowledge in machine-readable form *today*, without scraping prose or
> rendered snippets. This page records the exact contract of the
> rule-registry seam: `validate::RuleMeta`, the
> `ValidationRule::meta` trait method, `validate::builtin_rule_metas` /
> `validate::registry_metas` (re-exported at the crate root) and the CLI's
> `lintel rules` subcommand. Testable in
> `crates/lintel-core/tests/rule_registry.rs` and
> `crates/lintel-cli/tests/cli.rs`, demoed by
> `examples/rule_registry.rs` + `crates/lintel-cli/examples/rules_library.rs`,
> benched by the `rules` group.

## Source of truth

- `crates/lintel-core/src/validate.rs` — `RuleMeta` (id, codes, summary),
  the `meta()` default on the `ValidationRule` trait, the eight built-in
  classifications and the two listing functions.
- `crates/lintel-diag/src/lib.rs` — `Code::lnt`, the stable `LNTxxxx`
  strings the `codes` field reports, and `json_escape`, the shared
  escaping `RuleMeta::to_json` uses.
- `crates/lintel-cli/src/lib.rs` — the `rules` subcommand and its
  `--format=text|json` rendering.

## Exact contract

1. **A rule's identity is a `RuleMeta`.** Three stable fields: `id` (a
   unique kebab-case machine key, e.g. `"builtin-shadow"` — the key a docs
   page or editor integration addresses the rule by; built-in ids never
   change once shipped), `codes` (the `LNTxxxx` classes the rule can emit,
   in emission order — the order a single node's diagnostics arrive in),
   and `summary` (one line; the diagnostics' `help:` lines carry the
   specifics).
2. **Classification is opt-in with a safe default.** `ValidationRule`
   gains a provided method `fn meta(&self) -> Option<RuleMeta>` whose
   default returns `None`: every existing consumer rule keeps compiling
   and simply reports no registry entry. A rule opts in by returning its
   `RuleMeta`; `registry_metas(rules)` lists any injected registry's
   identities in registry order (`None` for an unclassified member), and
   `builtin_rule_metas()` lists the built-in set in registry (diagnostic)
   order.
3. **The built-in registry is fully classified — and pinned.** All eight
   built-in rules carry meta, and the integration tests assert the
   registry's `codes` union equals *exactly* the eight rule-emittable
   classes (`LNT0201`–`LNT0205`, `LNT0301`–`LNT0302`, `LNT0401`): a new
   `Code` variant cannot ship without a rule classification or an
   intentional parse-class exemption. Parse classes (`LNT01xx`) never
   appear in a registry: they are scanner output, never rule-selected.
4. **`to_json()` is one stable JSON line** — the `Diagnostic::to_json`
   contract applied to a registry entry: no trailing newline, field order
   `id`, `codes`, `summary`, the shared `lintel_diag::json_escape`
   escaping, `codes` a JSON array of `LNTxxxx` strings (`[]` for a
   policy-only rule). `lintel rules --format=json` writes one object per
   rule followed by `\n` (JSON Lines).
5. **`lintel rules` is compile-time knowledge.** The subcommand takes no
   paths (a path is a usage error: the registry does not depend on any
   source), accepts only the shared `--format=text|json` flag (plus
   `--help`/`--version`), always exits 0, and renders the built-in
   registry: text is one `<id> <comma-joined codes> <summary>` line per
   rule in registry order; JSON is one `RuleMeta::to_json` object per
   line. A broken/injected stdout sink fails the run like `--help` does.
6. **Read-only and infallible.** The seam maps rules → identities and
   cannot fail; it reports no diagnostics and never consults suppression
   state. A rule's *behavior* still lives in `check` — the listing
   describes policy, it does not run it (feed the same registry to
   `compile_with`/`validate_with` for that).

## What the seam does not do yet

- **No severity field.** Rule-level severity is policy expressed inside
  `check` (the static-handler rule emits a warning); a registry entry does
  not promise one until a second severity-expressing rule shape exists.
- **No per-code docs pages.** The machine-readable registry is the
  compiler-side prerequisite for the `docs/errors/LNT*.md` index pages
  (architecture §21.1); the pages themselves are still future work.
- **No rule configuration surface.** A registry entry carries identity
  only; tuning rules (thresholds, allow-lists) has no seam yet.
- **No discovery of consumer rules at runtime.** `registry_metas` sees the
  registry it is handed; there is no global rule registry beyond the
  built-ins.
