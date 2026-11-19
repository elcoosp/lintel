//! Lintel compiler core: scan → parse → lower → emit for `.lintel.php`
//! sources (architecture doc 5).
//!
//! The crate is pure (no I/O, no PHP invocation) and exposes the single
//! contract shared by the CLI, daemon, LSP and WASM playground:
//!
//! ```
//! use lintel_core::{compile, Options};
//!
//! let src = "<?php\nreturn <b class=\"x\">{$name}</b>;\n";
//! let out = compile(src, &Options::default()).unwrap();
//! assert!(out.php.contains("new \\Lintel\\Html("));
//! ```
//!
//! # Guarantees
//!
//! 1. **Verbatim pass-through** — PHP that contains no JSX is emitted
//!    byte-for-byte identical (AGENTS.md prime directive 2).
//! 2. **Line preservation** — the emitted file has exactly as many lines as
//!    the source, so every PHP token after a JSX root keeps its line number
//!    (Xdebug/PHPStan-friendly, doc 13.4).
//! 3. **Escape-by-default** — dynamic children go through `\Lintel\c()`,
//!    dynamic attributes through the context-aware helpers (`a`, `u`,
//!    `srcset`, `cls`, `sty`); only provably literal text — and provably
//!    scalar child expressions (`(int)`/`(float)` casts and `count()`
//!    calls, doc §7.3) — fold directly into the output.
//!
//! # Errors
//!
//! [`compile`] returns every error-severity [`Diagnostic`] it found (parse
//! diagnostics from the scanner, later validation diagnostics) instead of
//! panicking; each has a stable `LNTxxxx` code, a [`Severity`], a source
//! offset and an optional help line. Warnings (e.g. the static `on*` handler
//! lint) ride along on [`Output::warnings`] without failing the compilation.
//!
//! Validation diagnostics anchored on a line preceded by a
//! `// lintel-ignore-next` (or `# lintel-ignore-next`) line comment are
//! dropped, and a file-level `// lintel-ignore-file` comment drops every
//! validation diagnostic in the file — see `docs/ignore-directives.md` and
//! [`scan::ScanContext::suppressed_at`]/[`scan::ScanContext::file_suppressed`].
//! Parse diagnostics (`LNT01xx`) are never suppressible.

pub mod ast;
pub mod emit;
pub mod scan;
pub mod text;
pub mod validate;

pub use lintel_diag::{Code, Diagnostic, Severity, SourceMap};
pub use validate::{builtin_rules, ValidationRule};

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
    /// component calls; no `\Lintel\` runtime helpers are emitted.
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

/// Compile `.lintel.php` source into plain PHP.
///
/// This is a pure function: the same input and options always produce the
/// same output, and no PHP binary or filesystem access is involved.
///
/// # Errors
///
/// Returns every error-severity [`Diagnostic`] found while scanning or
/// validating (stable `LNTxxxx` codes, byte offsets, optional help lines).
/// Partial output is never returned: any error fails the whole compilation.
/// Warning-severity diagnostics ride along on [`Output::warnings`].
pub fn compile(src: &str, opts: &Options) -> Result<Output, Vec<Diagnostic>> {
    compile_with(src, opts, validate::builtin_rules())
}

/// Compile `.lintel.php` source with an explicit validation-rule registry.
///
/// The seam mirrors [`scan::scan_with`]'s lowering injection (doc 5.3):
/// rules run in slice order over a depth-first walk — pass
/// [`validate::builtin_rules`] to extend the default set with a custom
/// policy, or a smaller slice to disable built-in lints. Parse diagnostics
/// (`LNT01xx`) are scanner output and are never rule-selected; everything a
/// rule pushes flows through the same severity split and
/// `lintel-ignore-next` choke point as the built-in lints (see
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
    let parsed = scan::scan_with(src, emit::lowering_for(opts.target));
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
    Ok(Output {
        php: parsed.php,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::{compile, Options, Output};

    #[test]
    fn compiles_a_component_to_html_new() {
        let src = "<?php\nreturn <b>bold</b>;\n";
        let out = compile(src, &Options::default()).expect("compiles");
        assert_eq!(
            out.php,
            "<?php\nreturn new \\Lintel\\Html('<b>bold</b>');\n"
        );
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
