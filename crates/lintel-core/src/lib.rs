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
//!    dynamic attributes through the context-aware helpers (`a`, `u`, `cls`,
//!    `sty`); only provably literal text folds directly into the output.
//!
//! # Errors
//!
//! [`compile`] returns every [`Diagnostic`] it found (parse diagnostics from
//! the scanner, later validation diagnostics) instead of panicking; each has
//! a stable `LNTxxxx` code, a source offset and an optional help line.

pub mod ast;
mod emit;
pub mod scan;
pub mod text;
pub mod validate;

pub use lintel_diag::{Code, Diagnostic};

/// Output lowering target (doc 5.3). Only the string target exists in v0.1;
/// `rfc` and `stream` arrive with later milestones.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Target {
    /// String-concat AOT (default): functions return one `Html` value.
    #[default]
    String,
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
}

/// Compile `.lintel.php` source into plain PHP.
///
/// This is a pure function: the same input and options always produce the
/// same output, and no PHP binary or filesystem access is involved.
///
/// # Errors
///
/// Returns every [`Diagnostic`] found while scanning (stable `LNTxxxx`
/// codes, byte offsets, optional help lines). Partial output is never
/// returned: any diagnostic fails the whole compilation.
pub fn compile(src: &str, opts: &Options) -> Result<Output, Vec<Diagnostic>> {
    match opts.target {
        Target::String => compile_string(src),
    }
}

fn compile_string(src: &str) -> Result<Output, Vec<Diagnostic>> {
    let parsed = scan::scan(src);
    if !parsed.diagnostics.is_empty() {
        return Err(parsed.diagnostics);
    }
    let problems = validate::validate(&parsed.roots, &parsed.context);
    if !problems.is_empty() {
        return Err(problems);
    }
    Ok(Output { php: parsed.php })
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
        assert_eq!(out, Output { php: String::new() });
    }
}
