//! PHP-aware scanner (architecture doc 6.3).
//!
//! PHP passes through verbatim; JSX is recognized **only in expression-start
//! position** (doc 4.1): after keywords such as `return`/`echo`, after `(`,
//! `,`, `=>`, operators, or at the start of an argument. After an *operand*,
//! `<` is the less-than operator. This single rule resolves `$a < $b` versus
//! `return <div/>`.
//!
//! The scanner handles strings, comments, `#[attributes]`, heredocs/nowdocs,
//! inline-HTML interleave and `?>` transitions without rewriting any of them:
//! non-JSX bytes are copied 1:1, which the golden tests assert byte-for-byte.
//!
//! Known, deliberate v0.1 limitations (documented, safe): `<?` short open
//! tags are not recognized (deprecated upstream) and `<\Namespaced>` tags
//! written with a leading backslash are treated as operators.

use crate::ast::{Attr, Element, Node};
use crate::emit::{Lowering, StringLowering};
use crate::text::normalize_jsx_text;
use lintel_diag::{Code, Diagnostic};

/// The PHP open tags the scanner recognizes; `<?` short open tags are
/// deliberately not (deprecated upstream, doc §21.3).
const OPEN_TAG_PHP: &str = "<?php";
const OPEN_TAG_ECHO: &str = "<?=";

/// Keywords after which `<` starts an expression (doc 4.1).
const START_KEYWORDS: &[&str] = &[
    "return", "echo", "print", "yield", "and", "or", "xor", "case", "else", "throw", "match", "fn",
];

/// Whether the previous token can end an expression (operand) or starts one.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Prev {
    Start,
    Operand,
}

/// Result of trying to copy a compound token (string/comment/heredoc).
enum Compound {
    /// The byte is not a compound token; the caller keeps scanning it.
    None,
    /// A compound token was copied verbatim.
    Copied {
        /// Optional next previous-token state (`None` = leave unchanged).
        next_prev: Option<Prev>,
    },
}

/// What terminates a PHP scan: end of file, or the closing `}` of a JSX
/// expression brace.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Stop {
    Eof,
    Brace,
}

/// Everything the scanner produced for one source file.
#[derive(Debug)]
pub struct Parsed {
    /// Transpiled PHP: the verbatim input with every JSX root lowered.
    pub php: String,
    /// Every JSX root parsed at any nesting depth, in source order.
    pub roots: Vec<Node>,
    /// Functions declared (`function f()`) or imported (`use function f;`)
    /// in the file, used by the validation pass for shadow detection.
    pub context: ScanContext,
    /// Parse diagnostics; empty iff the source parsed cleanly.
    pub diagnostics: Vec<Diagnostic>,
}

/// Functions declared or imported in the scanned file (lowercase, last path
/// segment). The validation pass consults it to decide whether a component
/// call like `<Header>` shadows the PHP builtin `header()` (doc 4.7).
#[derive(Debug, Default, Clone)]
pub struct ScanContext {
    declared: Vec<String>,
    signatures: Vec<(String, Signature)>,
}

/// The declared parameter list of one PHP function (doc §17.1 signature
/// scanning). Recorded verbatim from `function Name(...)` declarations; the
/// validation pass checks component invocations against it.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Signature {
    /// Parameter names as written (`string $title` → `"title"`), in order.
    /// Types never contain `$`, so the first `$identifier` of each
    /// comma-separated segment is the name; default-value expressions come
    /// after it and are not recorded.
    pub params: Vec<String>,
    /// Whether the list ends in a variadic `...$rest` parameter, which
    /// accepts arbitrary named arguments and disables prop checking.
    pub variadic: bool,
}

impl ScanContext {
    /// Record a declared or imported function name; the last namespace
    /// segment is stored lowercased (PHP function lookups are
    /// case-insensitive).
    pub fn declare(&mut self, name: &str) {
        let candidate = name.rsplit('\\').next().unwrap_or(name).to_lowercase();
        if !self.declared.contains(&candidate) {
            self.declared.push(candidate);
        }
    }

    /// Record a declared function's parameter list (doc §17.1). Also feeds
    /// [`ScanContext::declare`] so builtin-shadow resolution sees the
    /// declaration. Only `function Name(...)` declarations carry a visible
    /// parameter list; `use function` imports are recorded name-only.
    pub fn declare_signature(&mut self, name: &str, signature: Signature) {
        self.declare(name);
        let candidate = name.rsplit('\\').next().unwrap_or(name).to_lowercase();
        if !self
            .signatures
            .iter()
            .any(|(existing, _)| *existing == candidate)
        {
            self.signatures.push((candidate, signature));
        }
    }

    /// The recorded parameter list of a function declared in this file,
    /// looked up case-insensitively by its last path segment (PHP function
    /// calls are case-insensitive).
    #[must_use]
    pub fn signature_of(&self, name: &str) -> Option<&Signature> {
        let candidate = name.rsplit('\\').next().unwrap_or(name).to_lowercase();
        self.signatures
            .iter()
            .find(|(existing, _)| *existing == candidate)
            .map(|(_, signature)| signature)
    }

    /// Whether a function with this name is declared/imported. PHP function
    /// lookups are case-insensitive, so the comparison is too.
    #[must_use]
    pub fn declares_function(&self, name: &str) -> bool {
        let needle = name.to_lowercase();
        self.declared.contains(&needle)
    }
}

/// Scan a full `.lintel.php` source into its transpiled PHP and AST roots,
/// lowering JSX with the default string target.
#[must_use]
pub fn scan(src: &str) -> Parsed {
    scan_with(src, &StringLowering)
}

/// Scan a full `.lintel.php` source, lowering every JSX root with the given
/// [`Lowering`] strategy (doc 5.3). The strategy is injected rather than
/// chosen inside the scanner so new targets need no scanner changes.
#[must_use]
pub fn scan_with(src: &str, lowering: &dyn Lowering) -> Parsed {
    let mut scanner = Scanner::new(src, lowering);
    scanner.transpile_file();
    Parsed {
        php: scanner.out,
        roots: scanner.roots,
        context: scanner.context,
        diagnostics: scanner.diagnostics,
    }
}

/// Function-declaration tracking (doc §17.1): `function` seen → declared
/// name seen → parameter list armed for capture. One state machine instead
/// of loose flags: only a declared name (not a `use function` import) arms a
/// signature capture, because only declarations carry a visible parameter
/// list.
#[derive(Default)]
enum FunctionPending {
    /// No declaration in flight.
    #[default]
    None,
    /// Saw `function`; the next word is the declared name (or `(` closes an
    /// anonymous function).
    Name,
    /// The declared name is recorded; the next `(` starts its parameter list.
    Signature(String),
}

struct Scanner<'s> {
    src: &'s str,
    b: &'s [u8],
    pos: usize,
    out: String,
    roots: Vec<Node>,
    context: ScanContext,
    /// Lowering strategy applied to every JSX root as it is parsed (doc 5.3).
    lowering: &'s dyn Lowering,
    /// Function-declaration state machine (see [`FunctionPending`]).
    pending_function: FunctionPending,
    /// Saw `use` and expect the import list next.
    pending_use: bool,
    /// Inside `use function …;` — every identifier is an imported name.
    import_mode: bool,
    diagnostics: Vec<Diagnostic>,
}

impl<'s> Scanner<'s> {
    fn new(src: &'s str, lowering: &'s dyn Lowering) -> Self {
        Self {
            src,
            b: src.as_bytes(),
            pos: 0,
            // `transpile_file` builds the output in a local buffer and moves
            // it into `self.out` at the end, so no capacity is reserved here.
            out: String::new(),
            roots: vec![],
            context: ScanContext::default(),
            lowering,
            pending_function: FunctionPending::default(),
            pending_use: false,
            import_mode: false,
            diagnostics: vec![],
        }
    }

    /// Entry point: whole file → transpiled PHP.
    fn transpile_file(&mut self) {
        let mut out = String::with_capacity(self.src.len() + self.src.len() / 4);
        while self.pos < self.b.len() {
            self.copy_inline_html(&mut out);
            if self.pos >= self.b.len() {
                break;
            }
            if let Err(problem) = self.php(&mut out, Stop::Eof) {
                self.diagnostics.push(problem);
                break;
            }
        }
        self.out = out;
    }

    /// Copy until `<?php` / `<?=` (inclusive of the open tag).
    fn copy_inline_html(&mut self, out: &mut String) {
        let start = self.pos;
        let rest = &self.src[self.pos..];
        let target = match (find_open_tag(rest, OPEN_TAG_PHP), rest.find(OPEN_TAG_ECHO)) {
            (Some(p), Some(e)) => Some(if p <= e {
                (p, OPEN_TAG_PHP.len())
            } else {
                (e, OPEN_TAG_ECHO.len())
            }),
            (Some(p), None) => Some((p, OPEN_TAG_PHP.len())),
            (None, Some(e)) => Some((e, OPEN_TAG_ECHO.len())),
            (None, None) => None,
        };
        match target {
            Some((rel, len)) => self.pos += rel + len,
            None => self.pos = self.b.len(),
        }
        out.push_str(&self.src[start..self.pos]);
    }

    /// Scan PHP, copying verbatim, transforming JSX at expression starts.
    fn php(&mut self, out: &mut String, stop: Stop) -> Result<(), Diagnostic> {
        let mut depth: usize = 0;
        let mut prev = Prev::Start;
        while self.pos < self.b.len() {
            let c = self.b[self.pos];
            let n1 = self.b.get(self.pos + 1).copied();
            match self.copy_compound(out, c, n1) {
                Compound::Copied { next_prev } => {
                    if let Some(next) = next_prev {
                        prev = next;
                    }
                    continue;
                }
                Compound::None => {}
            }
            match c {
                b'<' if prev == Prev::Start && self.looks_like_tag() => {
                    let start = self.pos;
                    let node = self.parse_element_or_fragment(start)?;
                    let total = self.newline_count(start);
                    out.push_str(&self.lowering.emit_root(&node, total));
                    self.roots.push(node);
                    prev = Prev::Operand;
                }
                b'?' if n1 == Some(b'>') && stop == Stop::Eof => {
                    let s = self.pos;
                    self.pos += 2;
                    self.flush(out, s);
                    return Ok(()); // back to inline HTML
                }
                b'{' => {
                    depth += 1;
                    let s = self.pos;
                    self.pos += 1;
                    self.flush(out, s);
                    prev = Prev::Start;
                }
                b'}' => {
                    if depth == 0 && stop == Stop::Brace {
                        return Ok(()); // caller consumes the closing '}'
                    }
                    depth = depth.saturating_sub(1);
                    let s = self.pos;
                    self.pos += 1;
                    self.flush(out, s);
                    prev = Prev::Operand;
                }
                b'$' | b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'\\' | 0x80..=0xff => {
                    let s = self.pos;
                    let word = self.take_word();
                    self.flush(out, s);
                    self.track_functions(word);
                    prev = Self::prev_after_word(word);
                }
                b'0'..=b'9' => {
                    let s = self.pos;
                    while self.pos < self.b.len()
                        && (self.b[self.pos].is_ascii_alphanumeric()
                            || matches!(self.b[self.pos], b'.' | b'_'))
                    {
                        self.pos += 1;
                    }
                    self.flush(out, s);
                    prev = Prev::Operand;
                }
                b' ' | b'\t' | b'\r' | b'\n' => {
                    let s = self.pos;
                    while self.pos < self.b.len() && self.b[self.pos].is_ascii_whitespace() {
                        self.pos += 1;
                    }
                    self.flush(out, s);
                }
                b')' | b']' => {
                    let s = self.pos;
                    self.pos += 1;
                    self.flush(out, s);
                    prev = Prev::Operand;
                }
                _ => {
                    let s = self.pos;
                    self.advance_char();
                    self.flush(out, s);
                    if c == b'(' {
                        let pending = std::mem::take(&mut self.pending_function);
                        match pending {
                            FunctionPending::Signature(name) => self.capture_signature(out, &name),
                            // anonymous `function (` or an unrelated `(`:
                            // nothing to capture
                            FunctionPending::None | FunctionPending::Name => {}
                        }
                    } else {
                        self.reset_pending(c);
                    }
                    prev = Prev::Start;
                }
            }
        }
        Ok(())
    }

    /// Track function declarations and imports for the validation pass
    /// (doc §17.1): `function` arms a name capture, the declared name arms a
    /// signature capture, and `use function` lists record imports name-only
    /// (they carry no visible parameter list).
    fn track_functions(&mut self, word: &str) {
        let lower = word.to_lowercase();
        if self.import_mode {
            if lower == "as" || lower == "function" {
                return;
            }
            self.context.declare(word);
            return;
        }
        if self.pending_use {
            self.pending_use = false;
            if lower == "function" {
                self.import_mode = true;
            }
            return;
        }
        if lower == "use" {
            self.pending_use = true;
            return;
        }
        if lower == "function" {
            if matches!(self.pending_function, FunctionPending::None) {
                self.pending_function = FunctionPending::Name;
            }
            return;
        }
        // The word right after `function` is the declared name (doc §17.1).
        if matches!(self.pending_function, FunctionPending::Name) {
            let last = word.rsplit('\\').next().unwrap_or(word).to_string();
            self.pending_function = FunctionPending::Signature(last);
        }
    }

    /// Punctuation that ends a pending declaration/import context: `;` ends
    /// `use` lists, `(` closes an anonymous `function (`, anything else but
    /// `&` (name-by-reference) also cancels a pending declaration. Armed
    /// signature captures are consumed by [`Scanner::capture_signature`]
    /// before this runs.
    fn reset_pending(&mut self, c: u8) {
        if c == b';' {
            self.import_mode = false;
            self.pending_use = false;
        }
        if c != b'&' {
            self.pending_function = FunctionPending::None;
        }
    }

    /// Copy a just-declared function's parameter list verbatim and record it
    /// as a [`Signature`] (doc §17.1). `self.pos` sits just past the opening
    /// `(`; compound tokens (strings, comments, attributes) pass through
    /// byte-identically like the main scan, so the parameter list is parsed
    /// from the exact source span that was copied.
    fn capture_signature(&mut self, out: &mut String, name: &str) {
        let start = self.pos;
        let mut depth = 0usize;
        while self.pos < self.b.len() {
            let c = self.b[self.pos];
            let n1 = self.b.get(self.pos + 1).copied();
            match self.copy_compound(out, c, n1) {
                Compound::Copied { next_prev: _ } => continue,
                Compound::None => {}
            }
            match c {
                b'(' => {
                    depth += 1;
                    let s = self.pos;
                    self.pos += 1;
                    self.flush(out, s);
                }
                b')' if depth == 0 => break, // caller consumes the closing ')'
                b')' => {
                    depth -= 1;
                    let s = self.pos;
                    self.pos += 1;
                    self.flush(out, s);
                }
                _ => {
                    let s = self.pos;
                    self.advance_char();
                    self.flush(out, s);
                }
            }
        }
        let params_src = &self.src[start.min(self.pos)..self.pos];
        let signature = parse_params(params_src);
        self.context.declare_signature(name, signature);
        if self.pos < self.b.len() && self.b[self.pos] == b')' {
            out.push(')');
            self.pos += 1;
        }
    }

    /// Strings, comments and heredocs are compound tokens; copies one and
    /// flushes it. Comments leave the previous-token state untouched (PHP
    /// treats them as transparent), strings/heredocs end with an operand.
    fn copy_compound(&mut self, out: &mut String, c: u8, n1: Option<u8>) -> Compound {
        let s = self.pos;
        match c {
            b'\'' => {
                self.copy_single_quoted();
                self.flush(out, s);
                Compound::Copied {
                    next_prev: Some(Prev::Operand),
                }
            }
            b'"' => {
                self.copy_double_quoted();
                self.flush(out, s);
                Compound::Copied {
                    next_prev: Some(Prev::Operand),
                }
            }
            b'`' => {
                self.copy_until_unescaped(b'`');
                self.flush(out, s);
                Compound::Copied {
                    next_prev: Some(Prev::Operand),
                }
            }
            b'/' if n1 == Some(b'/') => {
                self.copy_line_comment();
                self.flush(out, s);
                Compound::Copied { next_prev: None }
            }
            b'#' if n1 != Some(b'[') => {
                self.copy_line_comment();
                self.flush(out, s);
                Compound::Copied { next_prev: None }
            }
            b'/' if n1 == Some(b'*') => {
                self.copy_block_comment();
                self.flush(out, s);
                Compound::Copied { next_prev: None }
            }
            b'<' if n1 == Some(b'<') && self.b.get(self.pos + 2) == Some(&b'<') => {
                self.copy_heredoc();
                self.flush(out, s);
                Compound::Copied {
                    next_prev: Some(Prev::Operand),
                }
            }
            _ => Compound::None,
        }
    }

    fn prev_after_word(word: &str) -> Prev {
        if !word.starts_with('$') && START_KEYWORDS.iter().any(|k| word.eq_ignore_ascii_case(k)) {
            Prev::Start
        } else {
            Prev::Operand
        }
    }

    fn advance_char(&mut self) {
        let width = self.src[self.pos..]
            .chars()
            .next()
            .map_or(1, char::len_utf8);
        self.pos = (self.pos + width).min(self.b.len());
    }

    fn take_word(&mut self) -> &'s str {
        let s = self.pos;
        if self.b[self.pos] == b'$' {
            self.pos += 1;
        }
        while self.pos < self.b.len() {
            let c = self.b[self.pos];
            if c.is_ascii_alphanumeric() || c == b'_' || c == b'\\' || c >= 0x80 {
                self.pos += 1;
            } else {
                break;
            }
        }
        &self.src[s..self.pos]
    }

    /// At `<`: fragment `<>` or `<Name` (doc 6.3 `looks_like_tag`).
    fn looks_like_tag(&self) -> bool {
        match self.b.get(self.pos + 1) {
            Some(b'>') => true,
            Some(c) => c.is_ascii_alphabetic() || *c == b'_',
            None => false,
        }
    }

    // ---------- JSX parser ----------

    fn newline_count(&self, from: usize) -> u32 {
        u32::try_from(
            self.src[from.min(self.src.len())..self.pos]
                .matches('\n')
                .count(),
        )
        .unwrap_or(u32::MAX)
    }

    fn line_of(&self, upto: usize, from: usize) -> u32 {
        let end = upto.min(self.src.len());
        u32::try_from(self.src[from.min(end)..end].matches('\n').count()).unwrap_or(u32::MAX)
    }

    fn parse_element_or_fragment(&mut self, root: usize) -> Result<Node, Diagnostic> {
        let line = self.line_of(self.pos, root);
        let offset = self.pos;
        self.pos += 1; // consume '<'
        if self.b.get(self.pos) == Some(&b'>') {
            self.pos += 1;
            let children = self.parse_children(root, "")?;
            return Ok(Node::Fragment {
                children,
                line,
                offset,
            });
        }
        let name = self.take_tag_name();
        if name.is_empty() {
            return self.fail(Code::UnterminatedTag, "expected a tag name after `<`");
        }
        let is_component =
            name.chars().next().is_some_and(char::is_uppercase) || name.contains('\\');
        let (attrs, self_closing) = self.parse_attrs(root)?;
        if self_closing {
            return Ok(Node::Element(Element {
                name,
                is_component,
                attrs,
                children: vec![],
                self_closing: true,
                line,
                end_line: self.line_of(self.pos, root),
                offset,
                end_offset: self.pos,
            }));
        }
        let raw_text = matches!(name.as_str(), "script" | "style");
        let children = if raw_text {
            self.parse_raw_children(root, &name)?
        } else {
            self.parse_children(root, &name)?
        };
        Ok(Node::Element(Element {
            name,
            is_component,
            attrs,
            children,
            self_closing: false,
            line,
            end_line: self.line_of(self.pos, root),
            offset,
            end_offset: self.pos,
        }))
    }

    /// Parse attributes up to `>` (returns `(attrs, was_self_closing)`).
    fn parse_attrs(&mut self, root: usize) -> Result<(Vec<Attr>, bool), Diagnostic> {
        let mut attrs: Vec<Attr> = vec![];
        loop {
            self.skip_ws();
            match self.b.get(self.pos) {
                None => return self.fail(Code::UnterminatedTag, "unterminated tag"),
                Some(b'/') if self.b.get(self.pos + 1) == Some(&b'>') => {
                    self.pos += 2;
                    return Ok((attrs, true));
                }
                Some(b'>') => {
                    self.pos += 1;
                    return Ok((attrs, false));
                }
                Some(b'{') => attrs.push(self.parse_spread(root)?),
                Some(_) => attrs.push(self.parse_attr(root)?),
            }
        }
    }

    fn parse_spread(&mut self, root: usize) -> Result<Attr, Diagnostic> {
        let line = self.line_of(self.pos, root);
        let offset = self.pos;
        self.pos += 1; // consume '{'
        let mut php = String::new();
        self.php(&mut php, Stop::Brace)?;
        self.expect(b'}')?;
        let trimmed = php.trim_start();
        if let Some(expr) = trimmed.strip_prefix("...") {
            Ok(Attr::Spread {
                php: expr.to_string(),
                line,
                offset,
            })
        } else {
            self.fail(Code::BadSpread, "expected `{...expr}`")
        }
    }

    fn parse_attr(&mut self, root: usize) -> Result<Attr, Diagnostic> {
        let line = self.line_of(self.pos, root);
        let offset = self.pos;
        let s = self.pos;
        while self.pos < self.b.len()
            && (self.b[self.pos].is_ascii_alphanumeric()
                || matches!(self.b[self.pos], b'-' | b'_' | b':' | b'.' | b'@'))
        {
            self.pos += 1;
        }
        if s == self.pos {
            return self.fail(Code::BadAttr, "expected attribute name");
        }
        let name = self.src[s..self.pos].to_string();
        self.skip_ws();
        if self.b.get(self.pos) != Some(&b'=') {
            return Ok(Attr::Bool { name, line, offset });
        }
        self.pos += 1;
        self.skip_ws();
        match self.b.get(self.pos) {
            Some(q @ (b'"' | b'\'')) => {
                let quote = *q;
                self.pos += 1;
                let vs = self.pos;
                while self.pos < self.b.len() && self.b[self.pos] != quote {
                    self.pos += 1;
                }
                let value = self.src[vs..self.pos].to_string();
                self.pos += 1;
                Ok(Attr::Str {
                    name,
                    value,
                    line,
                    offset,
                })
            }
            Some(b'{') => {
                self.pos += 1;
                let mut php = String::new();
                self.php(&mut php, Stop::Brace)?;
                self.expect(b'}')?;
                Ok(Attr::Expr {
                    name,
                    php,
                    line,
                    offset,
                })
            }
            _ => self.fail(
                Code::BadAttr,
                "attribute value must be \"…\", '…' or {expr}",
            ),
        }
    }

    fn parse_children(&mut self, root: usize, parent: &str) -> Result<Vec<Node>, Diagnostic> {
        let mut out: Vec<Node> = vec![];
        let mut text_start = self.pos;
        loop {
            if self.pos >= self.b.len() {
                return self.fail(Code::UnterminatedTag, "unclosed element");
            }
            match self.b[self.pos] {
                b'<' if self.b.get(self.pos + 1) == Some(&b'/') => {
                    self.flush_text(&mut out, text_start, root);
                    self.pos += 2;
                    let name = self.take_tag_name();
                    self.skip_ws();
                    self.expect(b'>')?;
                    if name != parent {
                        return self.fail(
                            Code::MismatchedClose,
                            format!("expected </{parent}>, found </{name}>"),
                        );
                    }
                    return Ok(out);
                }
                b'<' => {
                    self.flush_text(&mut out, text_start, root);
                    let child = self.parse_element_or_fragment(root)?;
                    out.push(child);
                    text_start = self.pos;
                }
                b'{' => {
                    self.flush_text(&mut out, text_start, root);
                    let line = self.line_of(self.pos, root);
                    let offset = self.pos;
                    self.pos += 1;
                    let mut php = String::new();
                    self.php(&mut php, Stop::Brace)?;
                    self.expect(b'}')?;
                    let trimmed = php.trim();
                    let is_comment = trimmed.starts_with("/*")
                        && trimmed.ends_with("*/")
                        && trimmed.matches("*/").count() == 1;
                    if !trimmed.is_empty() && !is_comment {
                        out.push(Node::Expr { php, line, offset });
                    }
                    text_start = self.pos;
                }
                _ => self.pos += 1,
            }
        }
    }

    fn parse_raw_children(&mut self, root: usize, parent: &str) -> Result<Vec<Node>, Diagnostic> {
        let close = format!("</{parent}");
        let start = self.pos;
        let mut scan_from = start;
        let body_end = loop {
            match self.src[scan_from..].find(&close) {
                None => {
                    return self.fail(
                        Code::UnterminatedTag,
                        format!("unclosed raw-text element `<{parent}>`"),
                    )
                }
                Some(rel) => {
                    let after = scan_from + rel + close.len();
                    match self.b.get(after) {
                        Some(b'>' | b' ' | b'\t' | b'\r' | b'\n') => break scan_from + rel,
                        // e.g. `</scripted>` is not a close tag; keep looking
                        _ => scan_from = after,
                    }
                }
            }
        };
        let text = self.src[start..body_end].to_string();
        let line = self.line_of(start, root);
        self.pos = body_end + close.len();
        self.skip_ws();
        self.expect(b'>')?;
        Ok(if text.is_empty() {
            vec![]
        } else {
            vec![Node::Text {
                text,
                line,
                offset: start,
            }]
        })
    }

    fn flush_text(&mut self, out: &mut Vec<Node>, from: usize, root: usize) {
        if from >= self.pos {
            return;
        }
        let raw = &self.src[from..self.pos];
        if let Some(text) = normalize_jsx_text(raw) {
            out.push(Node::Text {
                text,
                line: self.line_of(from, root),
                offset: from,
            });
        }
    }

    fn take_tag_name(&mut self) -> String {
        let s = self.pos;
        while self.pos < self.b.len()
            && (self.b[self.pos].is_ascii_alphanumeric()
                || matches!(self.b[self.pos], b'_' | b'-' | b':' | b'\\' | b'.'))
        {
            self.pos += 1;
        }
        self.src[s..self.pos].to_string()
    }

    fn skip_ws(&mut self) {
        while self.pos < self.b.len() && self.b[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn expect(&mut self, expected: u8) -> Result<(), Diagnostic> {
        if self.b.get(self.pos) == Some(&expected) {
            self.pos += 1;
            Ok(())
        } else {
            let found = self.src[self.pos.min(self.src.len())..]
                .chars()
                .next()
                .map_or("end of input".to_string(), |c| format!("`{c}`"));
            self.fail(
                Code::Expected,
                format!("expected `{}`, found {found}", char::from(expected)),
            )
        }
    }

    fn fail<T>(&self, code: Code, message: impl Into<String>) -> Result<T, Diagnostic> {
        Err(Diagnostic::new(code, message, self.pos))
    }

    // ---------- verbatim copiers ----------

    fn flush(&self, out: &mut String, from: usize) {
        out.push_str(&self.src[from..self.pos]);
    }

    fn copy_single_quoted(&mut self) {
        self.pos += 1;
        while self.pos < self.b.len() {
            match self.b[self.pos] {
                b'\\' => self.pos += 2,
                b'\'' => {
                    self.pos += 1;
                    break;
                }
                _ => self.pos += 1,
            }
        }
        self.pos = self.pos.min(self.b.len());
    }

    /// Double-quoted with `{$expr["k"]}` interpolation support (nested quotes
    /// inside braces).
    fn copy_double_quoted(&mut self) {
        self.pos += 1;
        let mut brace = 0usize;
        while self.pos < self.b.len() {
            let c = self.b[self.pos];
            match c {
                b'\\' => self.pos += 2,
                b'"' if brace == 0 => {
                    self.pos += 1;
                    break;
                }
                b'{' if self.b.get(self.pos + 1) == Some(&b'$') => {
                    brace += 1;
                    self.pos += 1;
                }
                b'}' if brace > 0 => {
                    brace -= 1;
                    self.pos += 1;
                }
                b'\'' | b'"' if brace > 0 => {
                    let quote = c;
                    self.pos += 1;
                    while self.pos < self.b.len() && self.b[self.pos] != quote {
                        if self.b[self.pos] == b'\\' {
                            self.pos += 1;
                        }
                        self.pos += 1;
                    }
                    self.pos += 1;
                }
                _ => self.pos += 1,
            }
        }
        self.pos = self.pos.min(self.b.len());
    }

    fn copy_until_unescaped(&mut self, quote: u8) {
        self.pos += 1;
        while self.pos < self.b.len() {
            if self.b[self.pos] == b'\\' {
                self.pos += 2;
                continue;
            }
            if self.b[self.pos] == quote {
                self.pos += 1;
                break;
            }
            self.pos += 1;
        }
        self.pos = self.pos.min(self.b.len());
    }

    /// `//` / `#` comments end at the newline — or at `?>`, which ends PHP
    /// mode in real PHP and therefore ends the comment here too.
    fn copy_line_comment(&mut self) {
        while self.pos < self.b.len()
            && self.b[self.pos] != b'\n'
            && !(self.b[self.pos] == b'?' && self.b.get(self.pos + 1) == Some(&b'>'))
        {
            self.pos += 1;
        }
    }

    /// Block comments end at `*/` — or at `?>`, which the PHP engine treats
    /// as leaving PHP mode even inside a comment.
    fn copy_block_comment(&mut self) {
        let start = self.pos;
        self.pos += 2;
        let rest = &self.src[self.pos.min(self.src.len())..];
        let block_end = rest.find("*/").map(|rel| start + 2 + rel + 2);
        let close_end = rest.find("?>").map(|rel| start + 2 + rel);
        self.pos = match (block_end, close_end) {
            (Some(block), Some(close)) => block.min(close),
            (Some(end), None) | (None, Some(end)) => end,
            (None, None) => self.b.len(),
        };
        self.pos = self.pos.min(self.b.len());
    }

    /// `<<<LABEL` / `<<<"LABEL"` / `<<<'LABEL'` … closing label (PHP ≥7.3
    /// allows indentation before the label).
    fn copy_heredoc(&mut self) {
        self.pos += 3;
        while self
            .b
            .get(self.pos)
            .is_some_and(|c| matches!(*c, b' ' | b'\t'))
        {
            self.pos += 1;
        }
        let quote = self
            .b
            .get(self.pos)
            .copied()
            .filter(|c| *c == b'"' || *c == b'\'');
        if quote.is_some() {
            self.pos += 1;
        }
        let ls = self.pos;
        while self.pos < self.b.len()
            && (self.b[self.pos].is_ascii_alphanumeric() || self.b[self.pos] == b'_')
        {
            self.pos += 1;
        }
        let label = self.src[ls..self.pos].to_string();
        if quote.is_some() {
            self.pos += 1;
        }
        // Find a line that, after optional indentation, starts with the label
        // followed by a non-identifier character.
        let mut cur = self.pos;
        while let Some(nl) = self.src[cur..].find('\n') {
            let line_start = cur + nl + 1;
            let trimmed = self.src[line_start..].trim_start_matches([' ', '\t']);
            if trimmed.starts_with(&label)
                && !trimmed[label.len()..]
                    .starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_')
            {
                let end = line_start + (self.src[line_start..].len() - trimmed.len()) + label.len();
                self.pos = end;
                return;
            }
            cur = line_start;
        }
        self.pos = self.b.len();
    }
}

/// Parse a captured PHP parameter list (the text between the parens) into a
/// [`Signature`] (doc §17.1): the first `$identifier` of each top-level
/// segment is the parameter name — types never contain `$`, and
/// default-value expressions come after the name — and a segment containing
/// `...` marks a variadic tail.
fn parse_params(params_src: &str) -> Signature {
    let mut params: Vec<String> = vec![];
    let mut variadic = false;
    for segment in split_top_level(params_src, b',') {
        let trimmed = segment.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.contains("...") {
            variadic = true;
        }
        if let Some(name) = first_dollar_identifier(trimmed) {
            params.push(name);
        }
    }
    Signature { params, variadic }
}

/// Split on top-level `sep` bytes: parens/brackets/braces raise the depth and
/// quoted strings (with `\` escapes) are skipped, so default values like
/// `[1, 2]` or `'a,b'` never split a parameter.
fn split_top_level(src: &str, sep: u8) -> Vec<&str> {
    let mut parts: Vec<&str> = vec![];
    let bytes = src.as_bytes();
    let mut depth = 0usize;
    let mut start = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            quote @ (b'\'' | b'"' | b'`') => {
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == quote {
                        break;
                    }
                    i += 1;
                }
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            c if c == sep && depth == 0 => {
                parts.push(&src[start..i]);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    parts.push(&src[start..]);
    parts
}

/// The first `$name` identifier in a parameter segment: the parameter name.
fn first_dollar_identifier(segment: &str) -> Option<String> {
    let bytes = segment.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            let s = i + 1;
            let mut e = s;
            while e < bytes.len() && (bytes[e].is_ascii_alphanumeric() || bytes[e] == b'_') {
                e += 1;
            }
            if e > s {
                return Some(segment[s..e].to_string());
            }
        }
        i += 1;
    }
    None
}

/// Find `tag` in `rest` where the match is followed by whitespace or EOF
/// (`<?phpx` is not an open tag).
fn find_open_tag(rest: &str, tag: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(rel) = rest[from..].find(tag) {
        let at = from + rel;
        let after = at + tag.len();
        if after >= rest.len() || rest[after..].starts_with(|c: char| c.is_ascii_whitespace()) {
            return Some(at);
        }
        from = after;
    }
    None
}
