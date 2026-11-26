//! Lintl compiler core: scan → parse → lower → emit for `.lintl.php`
//! sources (architecture doc 5).
//!
//! The crate is pure (no I/O, no PHP invocation) and exposes the single
//! contract shared by the CLI, daemon, LSP and WASM playground:
//!
//! ```
//! use lintl_core::{compile, Options};
//!
//! let src = "<?php\nreturn <b class=\"x\">{$name}</b>;\n";
//! let out = compile(src, &Options::default()).unwrap();
//! assert!(out.php.contains("new \\Lintl\\Html("));
//! ```
//!
//! # Guarantees
//!
//! 1. **Verbatim pass-through** — PHP that contains no JSX is emitted
//!    byte-for-byte identical (AGENTS.md prime directive 2).
//! 2. **Line preservation** — the emitted file has exactly as many lines as
//!    the source, so every PHP token after a JSX root keeps its line number
//!    (Xdebug/PHPStan-friendly, doc 13.4).
//! 3. **Escape-by-default** — dynamic children go through `\Lintl\c()`,
//!    dynamic attributes through the context-aware helpers (`a`, `u`,
//!    `srcset`, `cls`, `sty`); only provably literal text — provably
//!    scalar child expressions (`(int)`/`(float)` casts and `count()`
//!    calls, doc §7.3) — and children that are exactly a parameter the
//!    enclosing function declares as a non-nullable `string` (folded to a
//!    direct `\htmlspecialchars(...)` call, `docs/escape-elision.md`) skip
//!    the generic helper.
//!
//! # Errors
//!
//! [`compile`] returns every error-severity [`Diagnostic`] it found (parse
//! diagnostics from the scanner, later validation diagnostics) instead of
//! panicking; each has a stable `LNLxxxx` code, a [`Severity`], a source
//! offset and an optional help line. Warnings (e.g. the static `on*` handler
//! lint) ride along on [`Output::warnings`] without failing the compilation.
//!
//! Validation diagnostics anchored on a line preceded by a
//! `// lintl-ignore-next` (or `# lintl-ignore-next`) line comment are
//! dropped, and a file-level `// lintl-ignore-file` comment drops every
//! validation diagnostic in the file — see `docs/ignore-directives.md` and
//! [`scan::ScanContext::suppressed_at`]/[`scan::ScanContext::file_suppressed`].
//! Parse diagnostics (`LNL01xx`) are never suppressible.
//!
//! # Reading the tree
//!
//! Consumers that walk the AST without validating — LSP features, metrics,
//! doc generators — use the [`NodeVisitor`] seam ([`visit_node`]/[`visit_roots`]):
//! the exact top-down, depth-first, source-ordered traversal the validation
//! pass runs, as a read-only visitor contract (`docs/node-visitor.md`).
//! Consumers that describe a file instead of walking it — outlines, dashboards,
//! the CLI's `stats` command — use the [`inventory`] seam
//! ([`collect_inventory`]): every declared component with its signature and
//! every invocation site with its position and props, in one read-only
//! report (`docs/component-inventory.md`). Consumers that list what a
//! validation registry enforces — docs-site generators, editor
//! integrations, CI reporters — use the rule-registry seam
//! ([`RuleMeta`] + [`builtin_rule_metas`]/[`registry_metas`]): every
//! rule's stable id, emitted `LNLxxxx` codes and one-line summary
//! (`docs/rule-registry.md`). Consumers validating one file against
//! components declared in other files fold the tables together with
//! [`scan::ScanContext::merged_with`] — the cross-file signature seam the
//! same-file boundary rules respected until then
//! (`docs/cross-file-signatures.md`).

pub mod analyze;
pub mod ast;
pub mod emit;
pub mod inventory;
pub mod scan;
pub mod text;
pub mod validate;

pub use analyze::{stale_hints, ChildHint, ChildKind, StaleHint, TypeHints};
pub use ast::{visit_node, visit_roots, NodeVisitor};
pub use emit::AnalyzeLowering;
pub use inventory::{collect_inventory, DeclaredComponent, FileInventory, Invocation};
pub use lintl_diag::{json_escape, Code, Diagnostic, Severity, SourceMap};
pub use validate::{builtin_rule_metas, builtin_rules, registry_metas, RuleMeta, ValidationRule};

/// Output lowering target (doc 5.3).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Target {
    /// String-concat AOT (default): functions return one `Html` value.
    #[default]
    String,
    /// Streaming lowering (doc 5.3 `stream`, §10): the JSX root becomes an
    /// immediately-invoked `static function (): \Generator` closure that
    /// yields the markup one chunk at a time. The exact lowering contract
    /// is recorded in `docs/stream-alignment.md`.
    Stream,
    /// RFC-parity lowering (doc 5.3, §0.1): markup becomes
    /// `new \Markup\Element(...)` constructor calls / named-argument
    /// component calls; no `\Lintl\` runtime helpers are emitted.
    /// See `docs/rfc-alignment.md` for the exact lowering contract.
    Rfc,
}

/// Compiler options.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Options {
    /// Lowering target; defaults to [`Target::String`].
    pub target: Target,
}

/// The compiled output for one source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    /// Plain, vendor-free PHP ready for `OPcache`.
    pub php: String,
    /// Warning-severity diagnostics (doc 4.4): the file compiled, but the
    /// validation pass has advice. Errors never appear here — they fail the
    /// compilation instead.
    pub warnings: Vec<Diagnostic>,
}

/// Compile `.lintl.php` source into plain PHP.
///
/// This is a pure function: the same input and options always produce the
/// same output, and no PHP binary or filesystem access is involved.
///
/// # Errors
///
/// Returns every error-severity [`Diagnostic`] found while scanning or
/// validating (stable `LNLxxxx` codes, byte offsets, optional help lines).
/// Partial output is never returned: any error fails the whole compilation.
/// Warning-severity diagnostics ride along on [`Output::warnings`].
pub fn compile(src: &str, opts: &Options) -> Result<Output, Vec<Diagnostic>> {
    compile_with(src, opts, validate::builtin_rules())
}

/// Compile `.lintl.php` source with an explicit validation-rule registry.
///
/// The seam mirrors [`scan::scan_with`]'s lowering injection (doc 5.3):
/// rules run in slice order over a depth-first walk — pass
/// [`validate::builtin_rules`] to extend the default set with a custom
/// policy, or a smaller slice to disable built-in lints. Parse diagnostics
/// (`LNL01xx`) are scanner output and are never rule-selected; everything a
/// rule pushes flows through the same severity split and
/// `lintl-ignore-next` choke point as the built-in lints (see
/// [`validate::ValidationRule`]).
///
/// # Errors
///
/// Same contract as [`compile`]: every error-severity [`Diagnostic`] found
/// while scanning or validating fails the whole compilation, warnings ride
/// along on [`Output::warnings`].
pub fn compile_with(
    src: &str,
    opts: &Options,
    rules: &[&dyn ValidationRule],
) -> Result<Output, Vec<Diagnostic>> {
    Ok(compile_analyzed(src, opts, &TypeHints::default(), rules)?.output)
}

/// The §7.3-Y2 analyze compile (`docs/type-elision.md`): compile `src` with
/// the `PHPStan` bridge's proven child-expression types (`hints`, resolved
/// per absolute source line + expression text) driving escape elision.
///
/// Elision is conservative end to end: only children the analyze pass
/// PROVED `int|float|numeric-string|literal` fold, and only past the v0.1
/// ladder (typed parameters, casts, `count()`) which keeps its exact
/// behavior; every unproven/ambiguous/mismatched child keeps
/// `\Lintl\c()`. With an empty hint table the output equals [`compile`]
/// byte for byte. The [`AnalyzedOutput`] counters report the elision rate
/// (§7.3-Y2: "Benchmarks must report elision rate").
///
/// # Errors
///
/// Same contract as [`compile`]: every error-severity [`Diagnostic`] found
/// while scanning or validating fails the whole compilation, warnings ride
/// along on [`Output::warnings`].
pub fn compile_analyzed(
    src: &str,
    opts: &Options,
    hints: &TypeHints,
    rules: &[&dyn ValidationRule],
) -> Result<AnalyzedOutput, Vec<Diagnostic>> {
    let lowering = emit::AnalyzeLowering::new(opts.target);
    let parsed = scan::scan_with_typed(src, &lowering, hints);
    if !parsed.diagnostics.is_empty() {
        return Err(parsed.diagnostics);
    }
    let (errors, warnings): (Vec<_>, Vec<_>) =
        validate::validate_with(&parsed.roots, &parsed.context, rules)
            .into_iter()
            .partition(|problem| problem.severity == Severity::Error);
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(AnalyzedOutput {
        output: Output {
            php: parsed.php,
            warnings,
        },
        dynamic_children: lowering.dynamic_children(),
        considered: lowering.considered(),
        elided: lowering.elided(),
    })
}

/// The [`compile_analyzed`] result: the compiled output plus the
/// elision-rate counters the analyze pass must report (§7.3-Y2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyzedOutput {
    /// The compiled PHP and the validation warnings.
    pub output: Output,
    /// Every dynamic child lowered on the markup targets (the rate's
    /// denominator).
    pub dynamic_children: u32,
    /// Children that reached the analyze decision step (past the v0.1
    /// folds).
    pub considered: u32,
    /// Children elided by the analyze pass (a subset of `considered`).
    pub elided: u32,
}

impl AnalyzedOutput {
    /// The elision rate: the fraction of decision-reachable children the
    /// analyze pass elided, or `None` when none reached the step.
    #[must_use]
    pub fn elision_rate(&self) -> Option<f64> {
        (self.considered > 0).then(|| f64::from(self.elided) / f64::from(self.considered))
    }
}

#[cfg(test)]
mod tests {
    use super::{compile, Options, Output};

    #[test]
    fn compiles_a_component_to_html_new() {
        let src = "<?php\nreturn <b>bold</b>;\n";
        let out = compile(src, &Options::default()).expect("compiles");
        assert_eq!(out.php, "<?php\nreturn new \\Lintl\\Html('<b>bold</b>');\n");
    }

    #[test]
    fn default_options_target_the_string_emitter() {
        let opts = Options::default();
        assert_eq!(opts.target, super::Target::String);
    }

    #[test]
    fn empty_source_compiles_to_empty_output() {
        let out = compile("", &Options::default()).expect("compiles");
        assert_eq!(
            out,
            Output {
                php: String::new(),
                warnings: vec![],
            }
        );
    }
}
