//! Stable-coded diagnostics for the Lintel JSX-for-PHP compiler.
//!
//! Every diagnostic carries a stable `LNTxxxx` code (see the error index at
//! the configured docs base, default `https://elcoosp.github.io/lintel/e/LNTxxxx`
//! — set `LINTEL_DOCS_BASE` to repoint; M3 release-readiness, since `lintel.sh`
//! is not live at launch), a [`Severity`], a human message, a byte
//! offset into the source and an optional `help:` line. The crate has no
//! dependencies: the compiler core (and later the WASM playground) can render
//! diagnostics without pulling in `miette`. The CLI crate may still choose to
//! re-render diagnostics with `miette` for fancy output; [`Diagnostic::render`]
//! is the dependency-free fallback that guarantees a stable, testable shape.
//!
//! # Layout
//!
//! - [`Code`] enumerates every stable error class.
//! - [`Severity`] distinguishes hard failures from advisory warnings.
//! - [`Diagnostic`] is the data type shared by CLI, daemon, LSP and WASM.
//! - [`Diagnostic::render`] draws a miette-style snippet with a caret.
//! - [`Diagnostic::to_json`] emits the same data as one-line JSON (JSON
//!   Lines) for machine-readable consumers.
//! - [`SourceMap`] converts the shared byte offsets into editor positions.

use std::fmt;
use std::fmt::Write as _;

/// Every stable diagnostic class emitted by the Lintel compiler.
///
/// Parse errors use the `LNT01xx` range, component/attribute validation the
/// `LNT02xx` range, security checks `LNT03xx`, and content-model checks
/// `LNT04xx`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Code {
    /// A tag or element was never closed (`LNT0101`).
    UnterminatedTag,
    /// A closing tag does not match the innermost open tag (`LNT0102`).
    MismatchedClose,
    /// An attribute could not be parsed (`LNT0103`).
    BadAttr,
    /// A spread was not written as `{...expr}` (`LNT0104`).
    BadSpread,
    /// A specific character was expected (`LNT0105`).
    Expected,
    /// A component name shadows a PHP builtin function (`LNT0201`).
    BuiltinShadow,
    /// A component attribute cannot become a PHP named argument (`LNT0202`).
    HyphenArg,
    /// A component prop does not match a declared parameter of the in-file
    /// component signature (`LNT0203`); PHP would reject the call at runtime
    /// with `Unknown named parameter`.
    UnknownProp,
    /// JSX children were passed to a component whose declared signature has
    /// no `children` parameter (`LNT0204`).
    UnexpectedChildren,
    /// A component invocation omits a parameter the in-file signature
    /// declares without a default value (`LNT0205`); PHP would reject the
    /// call at runtime with `Argument ... not given`.
    MissingRequiredProp,
    /// A dynamic event-handler attribute was rejected (`LNT0301`).
    DynamicHandler,
    /// A static event-handler string is allowed but discouraged (`LNT0302`).
    StaticHandler,
    /// A void element was given children (`LNT0401`).
    VoidChildren,
}

impl Code {
    /// The stable `LNTxxxx` identifier for this code.
    #[must_use]
    pub const fn lnt(self) -> &'static str {
        match self {
            Self::UnterminatedTag => "LNT0101",
            Self::MismatchedClose => "LNT0102",
            Self::BadAttr => "LNT0103",
            Self::BadSpread => "LNT0104",
            Self::Expected => "LNT0105",
            Self::BuiltinShadow => "LNT0201",
            Self::HyphenArg => "LNT0202",
            Self::UnknownProp => "LNT0203",
            Self::UnexpectedChildren => "LNT0204",
            Self::MissingRequiredProp => "LNT0205",
            Self::DynamicHandler => "LNT0301",
            Self::StaticHandler => "LNT0302",
            Self::VoidChildren => "LNT0401",
        }
    }

    /// One-line human summary, shown after the code when rendering.
    #[must_use]
    pub const fn summary(self) -> &'static str {
        match self {
            Self::UnterminatedTag => "unterminated tag",
            Self::MismatchedClose => "mismatched closing tag",
            Self::BadAttr => "invalid attribute",
            Self::BadSpread => "invalid spread expression",
            Self::Expected => "unexpected input",
            Self::BuiltinShadow => "component shadows a PHP builtin function",
            Self::HyphenArg => "attribute cannot become a named argument",
            Self::UnknownProp => "component has no such parameter",
            Self::UnexpectedChildren => "component does not accept children",
            Self::MissingRequiredProp => "required component parameter missing",
            Self::DynamicHandler => "dynamic event-handler attributes are not allowed",
            Self::StaticHandler => "static event-handler strings are discouraged",
            Self::VoidChildren => "void elements cannot have children",
        }
    }

    /// Link to the documentation page for this code.
    ///
    /// The docs base is configurable via the `LINTEL_DOCS_BASE` environment
    /// variable (M3 release-readiness): the default is the GitHub Pages URL
    /// where the docs-site + error index deploy, because `lintel.sh` is not
    /// live at launch. A maintainer who brings `lintel.sh` up (or any other
    /// domain) sets `LINTEL_DOCS_BASE=https://lintel.sh` and every emitted
    /// diagnostic URL repoints — no code change, no dead links.
    #[must_use]
    pub fn docs_url(self) -> String {
        let base = std::env::var("LINTEL_DOCS_BASE")
            .unwrap_or_else(|_| "https://elcoosp.github.io/lintel".to_string());
        // The base never carries a trailing slash (the format adds the path).
        let base = base.trim_end_matches('/');
        format!("{base}/e/{}", self.lnt())
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.lnt())
    }
}

/// How hard a diagnostic fails the compilation.
///
/// [`Severity::Error`] diagnostics fail `lintel_core::compile` (see the
/// `lintel-core` crate); warnings are returned on the compiled output
/// without failing it (doc 4.4: static `on*` strings are "allowed with a
/// lint warning").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Severity {
    /// A hard failure: the compilation returns `Err` (default).
    #[default]
    Error,
    /// An advisory diagnostic: the compilation succeeds and carries it.
    Warning,
}

impl Severity {
    /// The prefix shown before the code when rendering (`error`/`warning`).
    #[must_use]
    pub const fn prefix(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
        }
    }
}

/// A single compiler diagnostic with a stable [`Code`].
///
/// `offset` is a byte offset into the scanned source; it always points at an
/// ASCII-delimited boundary produced by the parser, so `Diagnostic::render`
/// can compute line/column information without panicking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Stable error class.
    pub code: Code,
    /// How hard this diagnostic fails the compilation.
    pub severity: Severity,
    /// Human-readable description of the problem.
    pub message: String,
    /// Byte offset into the source the diagnostic points at.
    pub offset: usize,
    /// Optional actionable `help:` line.
    pub help: Option<String>,
}

impl Diagnostic {
    /// Create an error-severity diagnostic without a `help:` line.
    pub fn new(code: Code, message: impl Into<String>, offset: usize) -> Self {
        Self {
            code,
            severity: Severity::Error,
            message: message.into(),
            offset,
            help: None,
        }
    }

    /// Attach an actionable `help:` line.
    #[must_use]
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Change the severity (warnings are returned on the output, not raised).
    #[must_use]
    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    /// `` `error|warning[LNTxxxx]: message` ``, as shown on the first render line.
    #[must_use]
    pub fn headline(&self) -> String {
        format!(
            "{}[{}]: {}",
            self.severity.prefix(),
            self.code.lnt(),
            self.message
        )
    }

    /// Render a miette-style snippet with a caret under the offending byte.
    ///
    /// The renderer is deliberately dependency-free: it prints the headline,
    /// a `input:line:col` header, the offending line with a `^` caret, and
    /// optional `help:`/`docs:` lines. Offsets beyond the end of `src` are
    /// clamped so rendering can never panic.
    #[must_use]
    pub fn render(&self, src: &str) -> String {
        let map = SourceMap::new(src);
        let (line_no, col) = map.line_col(self.offset);
        let line_text = map.line_text(self.offset);

        let mut out = String::new();
        out.push_str(&self.headline());
        out.push('\n');
        let _ = writeln!(out, "  \u{250c}\u{2500} input:{line_no}:{col}");
        out.push_str("  \u{2502}\n");
        let gutter = line_no.to_string();
        let pad = " ".repeat(gutter.len());
        let _ = writeln!(out, "{gutter} \u{2502} {line_text}");
        let _ = writeln!(out, "{pad} \u{b7} {}^", " ".repeat(col.saturating_sub(1)));
        if let Some(help) = &self.help {
            let _ = writeln!(out, "  = help: {help}");
        }
        let _ = writeln!(out, "  = docs: {}", self.code.docs_url());
        out
    }

    /// Render the diagnostic as a single-line JSON object (JSON Lines).
    ///
    /// The machine-readable counterpart to [`Diagnostic::render`], for CI
    /// logs, editor pipes and LSP-style consumers: the same [`SourceMap`]
    /// position semantics as the renderer (1-based line, 1-based character
    /// column, out-of-range offsets clamped) with a **stable field order** —
    /// `code`, `severity`, `message`, `offset`, `line`, `col`, `help`,
    /// `docs`. `severity` repeats the renderer's words (`error`/`warning`,
    /// see [`Severity::prefix`]); `help` is JSON `null` when absent; `docs`
    /// is [`Code::docs_url`]. The string contains no newline and no trailing
    /// newline, so a caller emits one object per line by appending `\n`.
    ///
    /// String values are JSON-escaped: `"` → `\"`, `\` → `\\`, control
    /// characters → `\n`, `\r`, `\t` or `\u00xx`; everything else passes
    /// through as UTF-8. Like [`Diagnostic::render`], the method never
    /// panics: any offset is clamped before the position fields are
    /// computed.
    #[must_use]
    pub fn to_json(&self, src: &str) -> String {
        let map = SourceMap::new(src);
        let (line, col) = map.line_col(self.offset);
        let mut out = String::with_capacity(192);
        out.push_str("{\"code\":\"");
        out.push_str(self.code.lnt());
        out.push_str("\",\"severity\":\"");
        out.push_str(self.severity.prefix());
        out.push_str("\",\"message\":\"");
        json_escape(&self.message, &mut out);
        out.push_str("\",\"offset\":");
        let _ = write!(out, "{}", self.offset);
        out.push_str(",\"line\":");
        let _ = write!(out, "{line}");
        out.push_str(",\"col\":");
        let _ = write!(out, "{col}");
        out.push_str(",\"help\":");
        match &self.help {
            Some(help) => {
                out.push('"');
                json_escape(help, &mut out);
                out.push('"');
            }
            None => out.push_str("null"),
        }
        let _ = write!(out, ",\"docs\":\"{}\"}}", self.code.docs_url());
        out
    }
}

/// JSON-escape `value` into `out`, the shared escaping rule of every
/// machine-readable emitter in the workspace (contract of
/// [`Diagnostic::to_json`]): quotes, backslashes and control characters
/// only — everything else is emitted verbatim so multi-byte UTF-8 survives
/// untouched. Public so sibling crates that emit their own JSON Lines
/// (e.g. the core's `FileInventory::to_json`) reuse the exact escaping the
/// diagnostics use instead of re-implementing it.
pub fn json_escape(value: &str, out: &mut String) {
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at offset {}", self.headline(), self.offset)?;
        if let Some(help) = &self.help {
            write!(f, " (help: {help})")?;
        }
        Ok(())
    }
}

impl std::error::Error for Diagnostic {}

/// Byte-offset → editor-position mapping for the offsets every
/// [`Diagnostic`] carries.
///
/// The compiler's diagnostics anchor on byte offsets; editors, LSPs and
/// CLIs speak in 1-based line/column pairs. [`Diagnostic::render`] computes
/// that mapping internally; this struct is the same conversion as a public
/// seam so consumers do not have to re-implement (or drift from) it.
///
/// Semantics, identical to the renderer's long-standing private helpers:
///
/// - Offsets are **clamped** into the source and **floored to a char
///   boundary** — an offset inside a multi-byte character resolves to the
///   character's first byte, never a panic.
/// - Lines are 1-based and counted on `\n` (a CRLF source has the same line
///   numbers as the LF-only equivalent).
/// - Columns are 1-based and counted in **characters**, not bytes, matching
///   the renderer's caret alignment.
pub struct SourceMap<'a> {
    src: &'a str,
}

impl<'a> SourceMap<'a> {
    /// Map offsets against `src`.
    #[must_use]
    pub fn new(src: &'a str) -> Self {
        Self { src }
    }

    /// The mapped source text.
    #[must_use]
    pub fn source(&self) -> &'a str {
        self.src
    }

    /// Clamp `offset` into the source, flooring it to a char boundary.
    #[must_use]
    pub fn clamp(&self, offset: usize) -> usize {
        let mut off = offset.min(self.src.len());
        while off > 0 && !self.src.is_char_boundary(off) {
            off -= 1;
        }
        off
    }

    /// The 1-based line and 1-based character column of `offset`.
    ///
    /// Out-of-range or mid-character offsets clamp first, so the last
    /// position of the source resolves instead of panicking.
    #[must_use]
    pub fn line_col(&self, offset: usize) -> (usize, usize) {
        let offset = self.clamp(offset);
        let prefix = &self.src[..offset];
        let line = prefix.matches('\n').count() + 1;
        let line_start = prefix.rfind('\n').map_or(0, |idx| idx + 1);
        let col = self.src[line_start..offset].chars().count() + 1;
        (line, col)
    }

    /// The full text of the line containing `offset` (without the newline).
    #[must_use]
    pub fn line_text(&self, offset: usize) -> &'a str {
        let offset = self.clamp(offset);
        let line_start = self.src[..offset].rfind('\n').map_or(0, |idx| idx + 1);
        let line_end = self.src[line_start..]
            .find('\n')
            .map_or(self.src.len(), |idx| line_start + idx);
        &self.src[line_start..line_end]
    }
}

#[cfg(test)]
mod tests {
    use super::{Code, Diagnostic, Severity, SourceMap};

    #[test]
    fn lnt_codes_are_stable() {
        assert_eq!(Code::UnterminatedTag.lnt(), "LNT0101");
        assert_eq!(Code::BuiltinShadow.lnt(), "LNT0201");
        assert_eq!(Code::HyphenArg.lnt(), "LNT0202");
        assert_eq!(Code::UnknownProp.lnt(), "LNT0203");
        assert_eq!(Code::UnexpectedChildren.lnt(), "LNT0204");
        assert_eq!(Code::MissingRequiredProp.lnt(), "LNT0205");
        assert_eq!(Code::DynamicHandler.lnt(), "LNT0301");
        assert_eq!(Code::StaticHandler.lnt(), "LNT0302");
        assert_eq!(Code::VoidChildren.lnt(), "LNT0401");
    }

    #[test]
    fn diagnostics_are_std_errors() {
        // The `std::error::Error` impl lets CLI/LSP consumers box a
        // diagnostic into `anyhow`/`Box<dyn Error>` without a wrapper.
        let problem: Box<dyn std::error::Error> = Box::new(Diagnostic::new(
            Code::MissingRequiredProp,
            "missing `$label`",
            3,
        ));
        assert_eq!(
            problem.to_string(),
            "error[LNT0205]: missing `$label` at offset 3"
        );
    }

    #[test]
    fn required_prop_summary_is_stable() {
        assert_eq!(
            Code::MissingRequiredProp.summary(),
            "required component parameter missing"
        );
    }

    #[test]
    fn warnings_render_with_the_warning_prefix() {
        let problem = Diagnostic::new(Code::StaticHandler, "static handler", 0)
            .with_severity(Severity::Warning);
        let rendered = problem.render("x");
        assert!(
            rendered.starts_with("warning[LNT0302]: static handler\n"),
            "{rendered}"
        );
        assert_eq!(
            problem.to_string(),
            "warning[LNT0302]: static handler at offset 0"
        );
    }

    #[test]
    fn docs_url_embeds_the_code() {
        assert_eq!(
            Code::DynamicHandler.docs_url(),
            "https://elcoosp.github.io/lintel/e/LNT0301"
        );
    }

    #[test]
    fn display_includes_code_and_offset() {
        let problem = Diagnostic::new(Code::Expected, "expected `>`", 7);
        assert_eq!(
            problem.to_string(),
            "error[LNT0105]: expected `>` at offset 7"
        );
    }

    #[test]
    fn render_points_at_the_offending_byte() {
        let src = "<?php\n<button onclick={$handler}>";
        let problem =
            Diagnostic::new(Code::DynamicHandler, "boom", 16).with_help("use `data-action={...}`");
        let rendered = problem.render(src);
        assert!(rendered.starts_with("error[LNT0301]: boom\n"), "{rendered}");
        assert!(rendered.contains("input:2:11"), "{rendered}");
        assert!(
            rendered.contains("<button onclick={$handler}>"),
            "{rendered}"
        );
        assert!(rendered.contains('^'), "{rendered}");
        assert!(
            rendered.contains("= help: use `data-action={...}`"),
            "{rendered}"
        );
        assert!(
            rendered.contains("= docs: https://elcoosp.github.io/lintel/e/LNT0301"),
            "{rendered}"
        );
    }

    #[test]
    fn render_clamps_out_of_range_offsets() {
        let src = "abc";
        let problem = Diagnostic::new(Code::UnterminatedTag, "unclosed", 99);
        let rendered = problem.render(src);
        assert!(rendered.contains("input:1:4"), "{rendered}");
    }

    #[test]
    fn render_omits_absent_help_line() {
        let problem = Diagnostic::new(Code::BadAttr, "bad", 0);
        let rendered = problem.render("x");
        assert!(!rendered.contains("= help:"), "{rendered}");
    }

    #[test]
    fn sourcemap_lines_and_columns_are_one_based_chars() {
        let map = SourceMap::new("héllo\nworld");
        // Byte 0 → line 1 col 1; `é` occupies bytes 1..3, so the `l` after
        // it is the third character of the line (byte 3, column 3).
        assert_eq!(map.line_col(0), (1, 1));
        assert_eq!(map.line_col(3), (1, 3));
        assert_eq!(map.line_col(4), (1, 4));
        // Byte 6 is the `\n` itself — still line 1; line 2 starts at 7.
        assert_eq!(map.line_col(6), (1, 6));
        assert_eq!(map.line_col(7), (2, 1));
        assert_eq!(map.clamp(2), 1, "mid-character offset floors");
        assert_eq!(map.line_text(6), "héllo");
        assert_eq!(map.line_text(7), "world");
        assert_eq!(map.line_text(0), "héllo");
    }

    #[test]
    fn sourcemap_clamps_out_of_range_offsets_like_the_renderer() {
        let map = SourceMap::new("abc");
        assert_eq!(map.line_col(99), (1, 4));
        assert_eq!(map.line_text(99), "abc");
        assert_eq!(map.clamp(99), 3);
        let empty = SourceMap::new("");
        assert_eq!(empty.line_col(0), (1, 1));
        assert_eq!(empty.line_text(0), "");
        assert_eq!(empty.clamp(7), 0);
    }

    #[test]
    fn render_agrees_with_the_public_sourcemap() {
        let src = "<?php\n<button onclick={$handler}>";
        let problem = Diagnostic::new(Code::DynamicHandler, "boom", 16);
        let map = SourceMap::new(src);
        let (line, col) = map.line_col(problem.offset);
        assert!(problem.render(src).contains(&format!("input:{line}:{col}")));
    }

    #[test]
    fn json_carries_code_severity_message_position_and_docs() {
        let src = "<?php\n<button onclick={$handler}>";
        let problem =
            Diagnostic::new(Code::DynamicHandler, "boom", 16).with_help("use `data-action={...}`");
        assert_eq!(
            problem.to_json(src),
            "{\"code\":\"LNT0301\",\"severity\":\"error\",\"message\":\"boom\",\"offset\":16,\"line\":2,\"col\":11,\"help\":\"use `data-action={...}`\",\"docs\":\"https://elcoosp.github.io/lintel/e/LNT0301\"}"
        );
    }

    #[test]
    fn json_uses_the_warning_severity_and_null_help() {
        let problem = Diagnostic::new(Code::StaticHandler, "static handler", 0)
            .with_severity(Severity::Warning);
        let json = problem.to_json("x");
        assert!(
            json.starts_with("{\"code\":\"LNT0302\",\"severity\":\"warning\""),
            "{json}"
        );
        assert!(json.contains("\"help\":null"), "{json}");
        assert!(
            json.ends_with("\"docs\":\"https://elcoosp.github.io/lintel/e/LNT0302\"}"),
            "{json}"
        );
        assert!(
            !json.contains('\n'),
            "one line, no trailing newline: {json}"
        );
    }

    #[test]
    fn json_escapes_quotes_backslashes_and_control_characters() {
        let problem = Diagnostic::new(Code::Expected, "expected `>`, got \"x\"\n\t\\y\u{7}", 7);
        let json = problem.to_json("");
        assert!(
            json.contains("\"message\":\"expected `>`, got \\\"x\\\"\\n\\t\\\\y\\u0007\""),
            "{json}"
        );
    }

    #[test]
    fn json_clamps_offsets_like_the_renderer() {
        let problem = Diagnostic::new(Code::UnterminatedTag, "unclosed", 99);
        assert_eq!(
            problem.to_json("abc"),
            "{\"code\":\"LNT0101\",\"severity\":\"error\",\"message\":\"unclosed\",\"offset\":99,\"line\":1,\"col\":4,\"help\":null,\"docs\":\"https://elcoosp.github.io/lintel/e/LNT0101\"}"
        );
    }
}
