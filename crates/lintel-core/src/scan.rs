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
//! The scanner also tracks the enclosing function scope for each JSX root
//! (doc §7.3 escape elision): every declared function's or braced closure's
//! body pushes a [`FunctionScope`] carrying its non-nullable `string`
//! parameter names, rebinding constructs unprove names, and the innermost
//! scope rides along to [`Lowering::emit_root_scoped`].
//!
//! Known, deliberate v0.1 limitations (documented, safe): `<?` short open
//! tags are not recognized (deprecated upstream) and `<\Namespaced>` tags
//! written with a leading backslash are treated as operators.

use crate::analyze::{resolve_decisions, TypeHints};
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

/// A suppression directive parsed out of a PHP line comment
/// (`docs/ignore-directives.md`): the one-line `lintel-ignore-next` window
/// and the file-level `lintel-ignore-file` flag. Anything else — including
/// near-miss spellings, block comments, strings and heredocs — is data.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Directive {
    /// `// lintel-ignore-next` — suppress the line after the comment.
    IgnoreNext,
    /// `// lintel-ignore-file` — suppress every validation diagnostic in
    /// the file.
    IgnoreFile,
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
    /// Where each declared function's name and parameter names sit in the
    /// source (the LSP go-to-definition seam, §17.1 Q3). Recorded by the
    /// same declaration walk that fills the signature table — the single
    /// source of truth — and keyed like it (last path segment, author's
    /// spelling, case-insensitive lookups).
    declarations: Vec<(String, Declaration)>,
    /// Source byte ranges covered by a `lintel-ignore-next` directive
    /// (`docs/ignore-directives.md`); consulted by the validation pass.
    suppressions: Vec<Suppression>,
    /// Whether a `lintel-ignore-file` directive was seen anywhere in the
    /// file (`docs/ignore-directives.md`): the validation pass drops every
    /// diagnostic for the file, and parse diagnostics stay fatal because
    /// they never reach that pass.
    file_suppressed: bool,
}

/// A source byte range `[start, end)` in which validation diagnostics are
/// dropped — the window recorded for a `// lintel-ignore-next` (or
/// `# lintel-ignore-next`) line comment: the physical source line
/// immediately after the directive's line. Byte offsets are absolute, like
/// every other offset the scanner produces. See `docs/ignore-directives.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suppression {
    /// First byte offset covered by the window (inclusive).
    pub start: usize,
    /// One past the last byte offset covered by the window (exclusive).
    pub end: usize,
}

/// The declared parameter list of one PHP function (doc §17.1 signature
/// scanning). Recorded verbatim from `function Name(...)` declarations; the
/// validation pass checks component invocations against it.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Signature {
    /// Parameter names as written (`string $title` → `"title"`), in order.
    /// Types never contain `$`, so the first `$identifier` of each
    /// comma-separated segment is the name; default-value expressions come
    /// after it and are not recorded verbatim — but a segment carrying a
    /// top-level `=` marks its parameter optional (see [`Signature::required`]).
    pub params: Vec<String>,
    /// Whether the list ends in a variadic `...$rest` parameter, which
    /// accepts arbitrary named arguments and disables prop checking.
    pub variadic: bool,
    /// Parameter names declared **without** a default value, in declaration
    /// order. PHP rejects a call that omits one with `Argument ... not
    /// given`, so the validation pass reports a missing required prop as
    /// `LNT0205`. Variadic parameters are optional by nature and never
    /// listed here.
    pub required: Vec<String>,
    /// Parameter names declared with a non-nullable `string` type (doc §7.3
    /// escape elision: "direct `htmlspecialchars` when the expression is a
    /// typed parameter of the enclosing function"). Recorded by
    /// [`parse_params`] from the segment text before the `$name` —
    /// visibility-promotion modifiers (`public`/`private`/`protected`/
    /// `readonly`) and a trailing `&` are stripped, the remaining type must
    /// be exactly `string` (case-insensitively), and nullable (`?string`),
    /// union (`string|int`), qualified (`\App\Name`) and variadic
    /// (`string ...$rest`) declarations do not qualify. The emit side folds
    /// bare `$name` children against this set (`docs/escape-elision.md`).
    pub typed_strings: Vec<String>,
}

/// The enclosing PHP function scope a JSX root lowers within (doc §7.3
/// escape elision). The scanner maintains a stack of these while it scans:
/// each declared function's (or braced closure's) body pushes one scope
/// carrying the parameter names the function declares as non-nullable
/// `string`, and each scope loses a name the moment the body can rebind it
/// (an assignment, `global`/`static`/`unset`/`list` binding, destructuring)
/// or clears entirely under a dynamic-binding construct (`extract`,
/// variable variables, `include`/`require`, `eval`) or an untracked arrow
/// function. The emit side folds bare `$name` children against
/// [`FunctionScope::typed_strings`] — `docs/escape-elision.md` records the
/// exact contract.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FunctionScope {
    /// The scope's proven string-typed names, with the internal
    /// brace-bookkeeping the scanner needs to pop the scope.
    pub(crate) typed_strings: Vec<String>,
    /// The depth value the scope's closing `}` returns the scanner to.
    pub(crate) close_depth: usize,
    /// Whether the next `$name` word rebinds (`global $x`, `static $x`,
    /// `unset($x)`, `list($x)`, `foreach (… as $x)`).
    pub(crate) rebind_armed: bool,
}

impl FunctionScope {
    /// The scope-less root context: JSX lowered outside any tracked
    /// function body has no proven names, so nothing elides.
    pub const EMPTY: FunctionScope = FunctionScope {
        typed_strings: Vec::new(),
        close_depth: 0,
        rebind_armed: false,
    };

    /// The parameter names this scope proves are non-nullable `string`
    /// right now — the set a bare `$name` child folds against.
    #[must_use]
    pub fn typed_strings(&self) -> &[String] {
        &self.typed_strings
    }
}

/// One declared function's source position (the LSP go-to-definition seam,
/// §17.1 Q3): the byte offsets a definition request resolves to. Recorded
/// verbatim from the same `function Name(...)` walk that fills the
/// signature table, so definition resolves against the compiler's own
/// declaration facts and every offset is an absolute source byte like the
/// ones [`crate::Diagnostic`] and [`crate::inventory::Invocation`] carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    /// The declared name, as written (last path segment).
    pub name: String,
    /// Absolute byte offset of the declared name identifier — the span a
    /// `<Card>` definition request jumps to.
    pub offset: usize,
    /// The verbatim parameter-list text between the declaration's parens
    /// (a signature-help label renders it; [`parse_params`] segments the
    /// same span).
    pub params_src: String,
    /// Parameter names in declaration order paired with the absolute byte
    /// offset of each name's `$` — the span a prop definition request
    /// jumps to. Segments without a `$identifier` (which [`parse_params`]
    /// also skips) contribute no entry, so the names align with
    /// [`Signature::params`] one-to-one.
    pub params: Vec<(String, usize)>,
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
    /// parameter list; `use function` imports are recorded name-only. The
    /// table keeps the name's **last path segment in its original
    /// spelling** ([`ScanContext::declared_signatures`] reports what the
    /// author wrote); every lookup on it stays case-insensitive, like PHP
    /// function calls.
    pub fn declare_signature(&mut self, name: &str, signature: Signature) {
        self.declare(name);
        let candidate = name.rsplit('\\').next().unwrap_or(name).to_string();
        if !self
            .signatures
            .iter()
            .any(|(existing, _)| existing.eq_ignore_ascii_case(&candidate))
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
            .find(|(existing, _)| existing.to_lowercase() == candidate)
            .map(|(_, signature)| signature)
    }

    /// Record a `lintel-ignore-next` suppression window: the byte range
    /// `[start, end)` of the source line following the directive comment
    /// (`docs/ignore-directives.md`). Empty ranges are ignored, so a
    /// directive on the last line without a trailing newline registers
    /// nothing.
    pub fn add_suppression(&mut self, start: usize, end: usize) {
        if end > start {
            self.suppressions.push(Suppression { start, end });
        }
    }

    /// Whether a diagnostic anchored at `offset` falls inside a recorded
    /// [`Suppression`] window (start inclusive, end exclusive). The
    /// validation pass filters its output with this, so a directive can only
    /// silence lint-severity diagnostics, never parse errors.
    #[must_use]
    pub fn suppressed_at(&self, offset: usize) -> bool {
        self.suppressions
            .iter()
            .any(|window| window.start <= offset && offset < window.end)
    }

    /// The recorded suppression windows, in source order.
    #[must_use]
    pub fn suppressions(&self) -> &[Suppression] {
        &self.suppressions
    }

    /// Latch the file-level suppression flag: a `// lintel-ignore-file` (or
    /// `# lintel-ignore-file`) line comment seen anywhere in the file
    /// (`docs/ignore-directives.md`). Idempotent; there is no opt-back-out
    /// form. The validation pass consults [`ScanContext::file_suppressed`]
    /// before running any rule; parse diagnostics are never affected.
    pub fn suppress_file(&mut self) {
        self.file_suppressed = true;
    }

    /// Whether a `lintel-ignore-file` directive was seen anywhere in the
    /// file. Unlike the [`Suppression`] windows this flag is position
    /// independent: the directive suppresses the whole file from wherever it
    /// sits, including the last line.
    #[must_use]
    pub fn file_suppressed(&self) -> bool {
        self.file_suppressed
    }

    /// Every declared function's parameter list, in declaration order: each
    /// item is `(last path segment in the author's spelling, &Signature)` —
    /// the table the validation pass looks up case-insensitively through
    /// [`ScanContext::signature_of`]. Consumers that report the file's shape
    /// (the component inventory, LSP outlines) iterate this instead of
    /// re-reading the source: every declared function is listed, whether or
    /// not any JSX in the file invokes it.
    pub fn declared_signatures(&self) -> impl Iterator<Item = (&str, &Signature)> {
        self.signatures
            .iter()
            .map(|(name, signature)| (name.as_str(), signature))
    }

    /// Record a declared function's source position (the LSP
    /// go-to-definition seam, §17.1 Q3). Fed by the same declaration walk
    /// as [`ScanContext::declare_signature`], keyed the same way (last
    /// path segment in the author's spelling, case-insensitive lookups);
    /// the first declaration of a name wins, matching the signature table.
    pub fn declare_declaration(&mut self, declaration: Declaration) {
        let candidate = declaration
            .name
            .rsplit('\\')
            .next()
            .unwrap_or(&declaration.name)
            .to_string();
        if !self
            .declarations
            .iter()
            .any(|(existing, _)| existing.eq_ignore_ascii_case(&candidate))
        {
            self.declarations.push((candidate, declaration));
        }
    }

    /// The recorded source position of a function declared in this file,
    /// looked up case-insensitively by its last path segment (PHP function
    /// calls are case-insensitive). `use function` imports carry no
    /// position — only `function Name(...)` declarations do.
    #[must_use]
    pub fn declaration_of(&self, name: &str) -> Option<&Declaration> {
        let candidate = name.rsplit('\\').next().unwrap_or(name).to_lowercase();
        self.declarations
            .iter()
            .find(|(existing, _)| existing.to_lowercase() == candidate)
            .map(|(_, declaration)| declaration)
    }

    /// Every declared function's source position, in declaration order:
    /// each item is `(last path segment in the author's spelling,
    /// &Declaration)` — the table an LSP definition provider iterates for
    /// its workspace symbols.
    pub fn declarations(&self) -> impl Iterator<Item = (&str, &Declaration)> {
        self.declarations
            .iter()
            .map(|(name, declaration)| (name.as_str(), declaration))
    }

    /// Whether a function with this name is declared/imported. PHP function
    /// lookups are case-insensitive, so the comparison is too.
    #[must_use]
    pub fn declares_function(&self, name: &str) -> bool {
        let needle = name.to_lowercase();
        self.declared.contains(&needle)
    }

    /// Merge `other`'s declaration tables into a copy of `self` for
    /// cross-file signature checks (`docs/cross-file-signatures.md`): the
    /// declared-function set and the signature table union, with `self`
    /// — the file being validated — winning case-insensitive name
    /// collisions and keeping its declaration order, while `other`'s new
    /// entries append in their own declaration order. Suppression state is
    /// deliberately **not** merged: the `lintel-ignore-next` windows are
    /// byte ranges of *their own* source and the `lintel-ignore-file` flag
    /// silences one file, so neither fact is meaningful in another file's
    /// coordinate space. The same per-source rule keeps the
    /// [`Declaration`] position table out of the merge — its offsets point
    /// into *their own* file's bytes, so a merged definition would jump
    /// into the wrong file. Cross-file definition is the workspace-index
    /// seam's job (§17.1 Q3 backlog), not the merge's. The merge is
    /// read-only on both inputs and documented non-commutative:
    /// `a.merged_with(&b)` resolves
    /// [`ScanContext::signature_of`]/[`ScanContext::declares_function`]
    /// lookups against `a`'s tables first, the way PHP resolves a call in
    /// the file it appears in.
    #[must_use]
    pub fn merged_with(&self, other: &ScanContext) -> ScanContext {
        let mut merged = ScanContext {
            declared: self.declared.clone(),
            signatures: self.signatures.clone(),
            declarations: Vec::new(),
            suppressions: Vec::new(),
            file_suppressed: false,
        };
        for name in &other.declared {
            if !merged.declared.contains(name) {
                merged.declared.push(name.clone());
            }
        }
        for (name, signature) in &other.signatures {
            let known = merged
                .signatures
                .iter()
                .any(|(existing, _)| existing.to_lowercase() == name.to_lowercase());
            if !known {
                merged.signatures.push((name.clone(), signature.clone()));
            }
        }
        merged
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
    scan_with_typed(src, lowering, &TypeHints::default())
}

/// Scan a full `.lintel.php` source with the §7.3-Y2 analyze hints
/// (`crate::analyze`, `docs/type-elision.md`): every JSX root lowers with
/// the given strategy through [`Lowering::emit_root_typed`], carrying the
/// per-child elision decisions resolved from the `PHPStan` bridge's report —
/// keyed onto each child's absolute source byte offset before lowering, so
/// the strategy looks decisions up with the offsets the tree already
/// carries. Hints that match no child (a stale report, a line/text
/// mismatch, an ambiguous entry) are silently ignored: a stale report must
/// degrade to the wrapped emission, never to a guessed fold.
#[must_use]
pub fn scan_with_typed(src: &str, lowering: &dyn Lowering, hints: &TypeHints) -> Parsed {
    let mut scanner = Scanner::new(src, lowering, hints);
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
    /// The declared name is recorded (with its byte offset for the
    /// [`Declaration`] go-to-definition table); the next `(` starts its
    /// parameter list.
    Signature(String, usize),
}

/// One copied parameter list and everything parsed out of it: the
/// [`Signature`] the validation pass checks against, the verbatim span it
/// came from (a signature-help label), and each parameter name's absolute
/// byte offset (a prop's go-to-definition target). Produced by
/// [`Scanner::copy_param_list`].
struct CapturedParams {
    signature: Signature,
    params_src: String,
    params: Vec<(String, usize)>,
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
    /// The §7.3-Y2 analyze hints (`crate::analyze`): per-root elision
    /// decisions are resolved from this table at each [`Self::lower_root`].
    hints: &'s TypeHints,
    /// Function-declaration state machine (see [`FunctionPending`]).
    pending_function: FunctionPending,
    /// Saw `use` and expect the import list next.
    pending_use: bool,
    /// Inside `use function …;` — every identifier is an imported name.
    import_mode: bool,
    /// Enclosing-function scope stack (doc §7.3): each tracked body pushes
    /// one [`FunctionScope`]; JSX roots lower against the innermost scope.
    scopes: Vec<FunctionScope>,
    /// A declared function's (or closure's) parameter scope armed for its
    /// upcoming `{` body — armed by [`Scanner::capture_signature`] and the
    /// anonymous-`(` arm, consumed by the next `{`. A declaration without a
    /// body (`abstract`/interface `;`) clears it, and a stale armed scope is
    /// harmless: an empty scope shadows correctly and pops at its own brace.
    pending_body: Option<FunctionScope>,
    diagnostics: Vec<Diagnostic>,
}

impl<'s> Scanner<'s> {
    fn new(src: &'s str, lowering: &'s dyn Lowering, hints: &'s TypeHints) -> Self {
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
            hints,
            pending_function: FunctionPending::default(),
            pending_use: false,
            import_mode: false,
            scopes: vec![],
            pending_body: None,
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
                    self.lower_root(out, start)?;
                    prev = Prev::Operand;
                }
                b'?' if n1 == Some(b'>') && stop == Stop::Eof => {
                    let s = self.pos;
                    self.pos += 2;
                    self.flush(out, s);
                    return Ok(()); // back to inline HTML
                }
                b'{' => {
                    self.open_brace(depth);
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
                    self.close_brace(depth);
                    let s = self.pos;
                    self.pos += 1;
                    self.flush(out, s);
                    prev = Prev::Operand;
                }
                b'$' | b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'\\' | 0x80..=0xff => {
                    let s = self.pos;
                    let word = self.take_word();
                    self.flush(out, s);
                    self.track_word(word, s);
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
                    self.end_group();
                    prev = Prev::Operand;
                }
                _ => {
                    let s = self.pos;
                    if c == b'=' && n1 == Some(b'>') {
                        self.arrow(out, s);
                        prev = Prev::Start;
                        continue;
                    }
                    self.advance_char();
                    self.flush(out, s);
                    if c == b'(' {
                        self.open_paren(out);
                    } else {
                        self.reset_pending(c);
                    }
                    prev = Prev::Start;
                }
            }
        }
        Ok(())
    }

    /// Parse the JSX root at `start` (an expression-start `<`) and splice
    /// its lowering into the output: the root lowers against the innermost
    /// enclosing [`FunctionScope`] (root-level context proves nothing), with
    /// the analyze pass's elision decisions resolved from the hint table
    /// first (§7.3-Y2) so the strategy sees exact per-child verdicts.
    fn lower_root(&mut self, out: &mut String, start: usize) -> Result<(), Diagnostic> {
        let node = self.parse_element_or_fragment(start)?;
        let total = self.newline_count(start);
        let scopeless = &FunctionScope::EMPTY;
        let scope = self.scopes.last().unwrap_or(scopeless);
        let decisions = resolve_decisions(&node, self.src, self.hints);
        out.push_str(
            &self
                .lowering
                .emit_root_typed(&node, total, scope, &decisions),
        );
        self.roots.push(node);
        Ok(())
    }

    /// `{`: the first `{` after a declaration's parameter list is the body,
    /// so push the armed scope at this depth (a stale armed scope is
    /// harmless — an empty scope shadows and pops at its own brace).
    fn open_brace(&mut self, depth: usize) {
        if let Some(mut scope) = self.pending_body.take() {
            scope.close_depth = depth;
            self.scopes.push(scope);
        }
    }

    /// `}`: pop the innermost scope when its body closes (the depth this
    /// `}` returns to matches the depth the body's `{` was entered at).
    fn close_brace(&mut self, depth: usize) {
        if self
            .scopes
            .last()
            .is_some_and(|scope| scope.close_depth == depth)
        {
            self.scopes.pop();
        }
    }

    /// `)` (or `]`) ends every named-rebinding construct in flight
    /// (`unset($x)`, `list($x)`, `foreach (… as $k => $v)`).
    fn end_group(&mut self) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.rebind_armed = false;
        }
    }

    /// `=>` — for a `fn(...) =>` arrow function the body is a braceless
    /// expression: the armed declaration scope is never braced, and the
    /// arrow's untracked parameters may shadow anything, so the enclosing
    /// scope's proven set is cleared (conservative; a plain
    /// `foreach (… as $k => $v)` arrow arrives with nothing armed and
    /// clears nothing).
    fn arrow(&mut self, out: &mut String, s: usize) {
        self.pos += 2;
        self.flush(out, s);
        if self.pending_body.take().is_some() {
            if let Some(scope) = self.scopes.last_mut() {
                scope.typed_strings.clear();
            }
        }
    }

    /// `(` after a word: a declared function's parameter list is captured
    /// into the context (arming its body [`FunctionScope`]), and an
    /// anonymous `function (` / `fn (` has its parameters parsed into a
    /// body scope too — a closure's parameters still shadow the enclosing
    /// scope, but the closure registers no name, so nothing lands in the
    /// context's signature table. The stream target rewrites a braced
    /// closure's declared `Html` return type exactly like a named
    /// declaration's (the M2 backlog): the braced body returns the
    /// generator closure the lowering emits; a body-less arrow function
    /// (`fn(): Html => …`) copies verbatim — [`Scanner::copy_return_type`]
    /// rewrites only when a `{` body follows, the same rule the named
    /// body-less declarations follow.
    fn open_paren(&mut self, out: &mut String) {
        let pending = std::mem::take(&mut self.pending_function);
        match pending {
            FunctionPending::Signature(name, name_offset) => {
                self.capture_signature(out, &name, name_offset);
            }
            FunctionPending::Name => {
                let captured = self.copy_param_list(out);
                self.pending_body = Some(FunctionScope {
                    typed_strings: captured.signature.typed_strings,
                    close_depth: 0,
                    rebind_armed: false,
                });
                self.copy_return_type(out);
            }
            FunctionPending::None => {}
        }
    }

    /// Track function declarations and imports for the validation pass
    /// (doc §17.1): `function`/`fn` arm a name capture, the declared name
    /// arms a signature capture (with the name's byte offset for the
    /// [`Declaration`] table), and `use function` lists record imports
    /// name-only (they carry no visible parameter list). Also tracks the
    /// scope-taint words of the typed-elision contract
    /// (`docs/escape-elision.md`): constructs that can rebind *arbitrary*
    /// names (`extract`, variable variables via `$$`, `include`/`require`,
    /// `eval`) clear the innermost scope's proven string set outright.
    fn track_word(&mut self, word: &str, start: usize) {
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
        if matches!(
            lower.as_str(),
            "extract" | "include" | "include_once" | "require" | "require_once" | "eval"
        ) {
            if let Some(scope) = self.scopes.last_mut() {
                scope.typed_strings.clear();
            }
        }
        if matches!(
            lower.as_str(),
            "global" | "static" | "unset" | "list" | "as"
        ) {
            // A named-rebinding construct is in flight (`global $a, $b`,
            // `unset($x)`, `list($x) = …`, `foreach (… as $k => $v)`): the
            // next `$name` words are unproven until the construct's `;`/`)`.
            if let Some(scope) = self.scopes.last_mut() {
                scope.rebind_armed = true;
            }
        }
        if lower == "function" || lower == "fn" {
            if matches!(self.pending_function, FunctionPending::None) {
                self.pending_function = FunctionPending::Name;
            }
            return;
        }
        // The word right after `function` is the declared name (doc §17.1);
        // `start` is where the word begins in the source — the offset a
        // definition request jumps to.
        if matches!(self.pending_function, FunctionPending::Name) {
            let last = word.rsplit('\\').next().unwrap_or(word).to_string();
            let name_offset = start + word.len() - last.len();
            self.pending_function = FunctionPending::Signature(last, name_offset);
        }
        if word.starts_with('$') {
            self.track_rebinding(word);
        }
    }

    /// Rebinding tracking for a `$name` word (doc §7.3 conservatism). While
    /// a named-rebinding construct is in flight the name is unconditionally
    /// unproven; otherwise an assignment-shaped token after the word —
    /// `=`, a compound assignment (`+=`, `.=`…), `??=`, shift assignment,
    /// or `[` (a `$title[0] = …` write is indistinguishable from a read) —
    /// unproves just that name. A lone `$` word is a `$$name`
    /// variable-variable and clears the whole scope. Every rule errs toward
    /// *removing* proven names: a false negative costs one `\Lintel\c()`
    /// call at render time, a false elision would fold a rebound value.
    fn track_rebinding(&mut self, word: &str) {
        let name = &word[1..];
        // The lookahead reads `self`, so it runs before the scope borrow.
        let assigns = if name.is_empty() {
            true
        } else {
            let b = self.b;
            let mut i = self.pos;
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            match b.get(i) {
                Some(b'=') => !matches!(b.get(i + 1), Some(b'=' | b'>')),
                Some(b'[') => true,
                // Destructuring/list assignment (`[$title, $label] = $pair;`):
                // the `[` sits before the name and the `=` after the list's
                // `]`, so scan (bounded by the statement's `;`) for a `]`
                // followed by an assignment `=`. Reads end in `];` and never
                // match.
                Some(b']' | b',') => self.list_assign_ahead(i),
                Some(c1 @ (b'+' | b'-' | b'*' | b'/' | b'%' | b'&' | b'|' | b'^' | b'.')) => {
                    matches!(b.get(i + 1), Some(b'='))
                        || (*c1 == b'*'
                            && matches!(b.get(i + 1), Some(b'*'))
                            && matches!(b.get(i + 2), Some(b'=')))
                }
                Some(b'?') => {
                    matches!(b.get(i + 1), Some(b'?')) && matches!(b.get(i + 2), Some(b'='))
                }
                Some(shift @ (b'<' | b'>')) => {
                    matches!(b.get(i + 1), Some(c2) if c2 == shift)
                        && matches!(b.get(i + 2), Some(b'='))
                }
                _ => false,
            }
        };
        if let Some(scope) = self.scopes.last_mut() {
            if name.is_empty() {
                // `$$name` variable-variable: rebinding is untrackable, so
                // the whole proven set is dropped.
                scope.typed_strings.clear();
                scope.rebind_armed = false;
                return;
            }
            if scope.rebind_armed {
                scope.typed_strings.retain(|proven| proven != name);
            } else if assigns {
                scope.typed_strings.retain(|proven| proven != name);
            }
        }
    }

    /// Whether a `]` followed by an assignment `=` appears before the
    /// statement ends (`;`, `{`, `}`) — the shape of
    /// `[$title, $label] = $pair;` destructuring seen from inside the list.
    fn list_assign_ahead(&self, mut i: usize) -> bool {
        let b = self.b;
        while i < b.len() {
            match b[i] {
                b';' | b'{' | b'}' => return false,
                b']' => {
                    let mut j = i + 1;
                    while j < b.len() && b[j].is_ascii_whitespace() {
                        j += 1;
                    }
                    return matches!(b.get(j), Some(b'='))
                        && !matches!(b.get(j + 1), Some(b'=' | b'>'));
                }
                _ => i += 1,
            }
        }
        false
    }

    /// Punctuation that ends a pending declaration/import context: `;` ends
    /// `use` lists and closes body-less `abstract`/interface declarations
    /// (clearing the armed [`FunctionScope`]), `(` closes an anonymous
    /// `function (`, anything else but `&` (name-by-reference) also cancels
    /// a pending declaration. Armed signature captures are consumed by
    /// [`Scanner::capture_signature`] before this runs.
    fn reset_pending(&mut self, c: u8) {
        if c == b';' {
            self.import_mode = false;
            self.pending_use = false;
            self.pending_body = None;
            if let Some(scope) = self.scopes.last_mut() {
                scope.rebind_armed = false;
            }
        }
        if c != b'&' {
            self.pending_function = FunctionPending::None;
        }
    }

    /// Copy a just-declared function's parameter list verbatim and record it
    /// as a [`Signature`] (doc §17.1). `self.pos` sits just past the opening
    /// `(`; compound tokens (strings, comments, attributes) pass through
    /// byte-identically like the main scan, so the parameter list is parsed
    /// from the exact source span that was copied. Arms the function body's
    /// [`FunctionScope`] (`docs/escape-elision.md`) for the upcoming `{`
    /// and records the [`Declaration`] position table entry (§17.1 Q3).
    fn capture_signature(&mut self, out: &mut String, name: &str, name_offset: usize) {
        let captured = self.copy_param_list(out);
        let typed_strings = captured.signature.typed_strings.clone();
        self.context.declare_signature(name, captured.signature);
        self.context.declare_declaration(Declaration {
            name: name.rsplit('\\').next().unwrap_or(name).to_string(),
            offset: name_offset,
            params_src: captured.params_src,
            params: captured.params,
        });
        self.pending_body = Some(FunctionScope {
            typed_strings,
            close_depth: 0,
            rebind_armed: false,
        });
        self.copy_return_type(out);
    }

    /// Copy the declared function's or anonymous closure's closing `)`
    /// and — when the lowering rewrites return types
    /// ([`Lowering::rewrites_return_types`], the stream target) — its
    /// `Html`-typed return declaration, rewritten to `\Generator`: the
    /// stream body returns the generator closure the lowering emits, and
    /// the author's `Html` return type `TypeError`s against it under
    /// `strict_types` the moment a real PHP consumer invokes the artifact
    /// (the Phase 9 parity bug the `renderStreaming()` acceptance proved;
    /// braced anonymous closures joined with the M2 backlog pass).
    /// Nullable and qualified spellings (`?Html`, `\Lintel\Html`) rewrite
    /// to the non-null `\Generator` too (the generator is never null); any
    /// other type, a union (`Html|string` — the span stops at `|`), or a
    /// body-less declaration (`abstract`/interface `;`, an arrow
    /// `fn(): Html => …`) copies verbatim. The rewrite is same-line: line
    /// preservation holds.
    fn copy_return_type(&mut self, out: &mut String) {
        // The closing parenthesis is already copied (and `self.pos` stepped
        // past it) by [`Scanner::copy_param_list`]'s tail — this capture
        // picks up exactly where it left off.
        if !self.lowering.rewrites_return_types() {
            return;
        }
        let Some((colon, type_end)) = self.lookahead_return_type() else {
            return; // no type, a union, or no body: the loop copies verbatim
        };
        out.push_str(&self.src[self.pos..colon]);
        out.push_str(": \\Generator");
        self.pos = type_end;
    }

    /// The declared return type's span when it is an `Html` spelling with
    /// a body following: `(colon_start, type_end)` — whitespace and
    /// comments between the tokens are lexed; anything else (no colon, no
    /// type, a union, a `;` body-less declaration) yields `None`.
    fn lookahead_return_type(&self) -> Option<(usize, usize)> {
        let mut cursor = self.skip_ws_comments(self.pos)?;
        if self.b.get(cursor) != Some(&b':') {
            return None;
        }
        let colon = cursor;
        cursor = self.skip_ws_comments(cursor + 1)?;
        let type_start = cursor;
        if self.b.get(cursor) == Some(&b'?') {
            cursor += 1;
        }
        while self
            .b
            .get(cursor)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'\\')
        {
            cursor += 1;
        }
        let type_end = cursor;
        if type_end == type_start || cursor >= self.b.len() {
            return None;
        }
        // A BODY must follow: `;` (abstract/interface) keeps its contract.
        cursor = self.skip_ws_comments(cursor)?;
        if self.b.get(cursor) != Some(&b'{') {
            return None;
        }
        let spelled = self.src[type_start..type_end].trim_start_matches('?');
        let html = spelled
            .rsplit('\\')
            .next()
            .is_some_and(|segment| segment.eq_ignore_ascii_case("Html"));
        html.then_some((colon, type_end))
    }

    /// Lexically skip whitespace and comments from `cursor` (line `//`/`#`
    /// comments stop at their newline, block comments at `*/`); `None`
    /// past EOF or on a malformed region.
    fn skip_ws_comments(&self, mut cursor: usize) -> Option<usize> {
        loop {
            match (self.b.get(cursor), self.b.get(cursor + 1)) {
                (Some(b), _) if b.is_ascii_whitespace() => cursor += 1,
                (Some(b'/'), Some(b'*')) => {
                    cursor += self.src[cursor..].find("*/")? + 2;
                }
                (Some(b'#'), Some(b'[')) => {
                    // An attribute (`#[…]`) is not a comment: the return
                    // type region never carries one — stop.
                    return Some(cursor);
                }
                // `// …` and `# …` line comments share the same skip.
                (Some(b'/'), Some(b'/')) | (Some(b'#'), _) => {
                    cursor += self.src[cursor..].find('\n')?;
                }
                _ => return Some(cursor),
            }
        }
    }

    /// Copy a parameter list verbatim (through the closing `)`) and parse
    /// it into a [`Signature`] plus its position facts. Shared by named
    /// declarations ([`Scanner::capture_signature`]) and anonymous
    /// closures, whose parameters shadow the enclosing scope but are not
    /// registered in the context (they carry no name to look components up
    /// by — only [`CapturedParams::signature`] feeds a closure's scope).
    fn copy_param_list(&mut self, out: &mut String) -> CapturedParams {
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
        // Each parameter name's absolute offset: the first `$identifier` of
        // its comma-separated segment ([`parse_params`]'s own rule), located
        // within the same verbatim span the signature was parsed from, so
        // the positions cannot drift from the names.
        let params = split_top_level(params_src, b',')
            .iter()
            .filter_map(|segment| {
                let (name, dollar) = first_dollar_span(segment)?;
                let segment_start = segment.as_ptr() as usize - params_src.as_ptr() as usize;
                Some((name, start.min(self.pos) + segment_start + dollar))
            })
            .collect();
        if self.pos < self.b.len() && self.b[self.pos] == b')' {
            out.push(')');
            self.pos += 1;
        }
        CapturedParams {
            signature,
            params_src: params_src.to_string(),
            params,
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
                self.copy_line_comment(s);
                self.flush(out, s);
                Compound::Copied { next_prev: None }
            }
            b'#' if n1 != Some(b'[') => {
                self.copy_line_comment(s);
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
            return self.fail_help(
                Code::UnterminatedTag,
                "expected a tag name after `<`",
                "tag names start with a letter or `_`; `<>` opens a fragment",
            );
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
                None => {
                    return self.fail_help(
                        Code::UnterminatedTag,
                        "unterminated tag",
                        "close the tag before the end of input: `>` or `/>`",
                    )
                }
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
        self.expect(b'}', "close the `{ … }` spread expression")?;
        let trimmed = php.trim_start();
        if let Some(expr) = trimmed.strip_prefix("...") {
            Ok(Attr::Spread {
                php: expr.to_string(),
                line,
                offset,
            })
        } else {
            self.fail_help(
                Code::BadSpread,
                "expected `{...expr}`",
                "write the spread as `{...$attrs}`",
            )
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
            return self.fail_help(
                Code::BadAttr,
                "expected attribute name",
                "write `name=\"value\"`, a bare boolean `name`, or end the tag with `>`",
            );
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
                self.expect(b'}', "close the `{ … }` attribute expression")?;
                Ok(Attr::Expr {
                    name,
                    php,
                    line,
                    offset,
                })
            }
            _ => self.fail_help(
                Code::BadAttr,
                "attribute value must be \"…\", '…' or {expr}",
                "quote the value: `name=\"text\"`, `name='text'` or `name={expr}`",
            ),
        }
    }

    fn parse_children(&mut self, root: usize, parent: &str) -> Result<Vec<Node>, Diagnostic> {
        let mut out: Vec<Node> = vec![];
        let mut text_start = self.pos;
        loop {
            if self.pos >= self.b.len() {
                return self.fail_help(
                    Code::UnterminatedTag,
                    "unclosed element",
                    format!("close the element: `</{parent}>`"),
                );
            }
            match self.b[self.pos] {
                b'<' if self.b.get(self.pos + 1) == Some(&b'/') => {
                    self.flush_text(&mut out, text_start, root);
                    self.pos += 2;
                    let name = self.take_tag_name();
                    self.skip_ws();
                    self.expect(b'>', "end the closing tag with `>`")?;
                    if name != parent {
                        // A near-miss spelling of the open tag gets a
                        // "did you mean" help line (§21.1); an unrelated
                        // name must not guess.
                        let mut problem = Diagnostic::new(
                            Code::MismatchedClose,
                            format!("expected </{parent}>, found </{name}>"),
                            self.pos,
                        );
                        if let Some(help) = close_tag_help(&name, parent) {
                            problem = problem.with_help(help);
                        }
                        return Err(problem);
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
                    self.expect(b'}', "close the `{ … }` child expression")?;
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
                    return self.fail_help(
                        Code::UnterminatedTag,
                        format!("unclosed raw-text element `<{parent}>`"),
                        format!("close the raw-text element: `</{parent}>`"),
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
        self.expect(b'>', "end the closing tag with `>`")?;
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

    fn expect(&mut self, expected: u8, help: &str) -> Result<(), Diagnostic> {
        if self.b.get(self.pos) == Some(&expected) {
            self.pos += 1;
            Ok(())
        } else {
            let found = self.src[self.pos.min(self.src.len())..]
                .chars()
                .next()
                .map_or("end of input".to_string(), |c| format!("`{c}`"));
            self.fail_help(
                Code::Expected,
                format!("expected `{}`, found {found}", char::from(expected)),
                help,
            )
        }
    }

    /// The actionable `help:` line the operating manual requires on every
    /// diagnostic (architecture doc §18); the message names the problem,
    /// the help names the fix.
    fn fail_help<T>(
        &self,
        code: Code,
        message: impl Into<String>,
        help: impl Into<String>,
    ) -> Result<T, Diagnostic> {
        Err(Diagnostic::new(code, message, self.pos).with_help(help))
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
    /// mode in real PHP and therefore ends the comment here too. A comment
    /// carrying the `lintel-ignore-next` directive records a suppression
    /// window covering the physical line after the comment's own line
    /// (`docs/ignore-directives.md`); `start` is the comment's first byte.
    /// A comment carrying the file-level `lintel-ignore-file` directive
    /// latches [`ScanContext::suppress_file`] — position independent, so no
    /// window is recorded for it.
    fn copy_line_comment(&mut self, start: usize) {
        while self.pos < self.b.len()
            && self.b[self.pos] != b'\n'
            && !(self.b[self.pos] == b'?' && self.b.get(self.pos + 1) == Some(&b'>'))
        {
            self.pos += 1;
        }
        match Self::directive_of(&self.src[start..self.pos]) {
            Some(Directive::IgnoreNext) => {
                let line_end = self.src[self.pos..]
                    .find('\n')
                    .map_or(self.b.len(), |rel| self.pos + rel);
                let window_start = line_end + 1;
                if window_start < self.b.len() {
                    let window_end = self.src[window_start..]
                        .find('\n')
                        .map_or(self.b.len(), |rel| window_start + rel);
                    self.context.add_suppression(window_start, window_end);
                }
            }
            Some(Directive::IgnoreFile) => self.context.suppress_file(),
            None => {}
        }
    }

    /// Which suppression directive a copied line comment carries, if any:
    /// the comment marker(s) stripped, leading whitespace trimmed, then the
    /// directive word, then end-of-comment, whitespace, or a `:` before the
    /// optional reason. The word is exact, so `lintel-ignore-nextline` and
    /// `lintel-ignore-filex` match nothing.
    fn directive_of(comment: &str) -> Option<Directive> {
        let body = comment.trim_start_matches(['/', '#']).trim_start();
        for (word, directive) in [
            ("lintel-ignore-next", Directive::IgnoreNext),
            ("lintel-ignore-file", Directive::IgnoreFile),
        ] {
            if let Some(rest) = body.strip_prefix(word) {
                if rest.is_empty() || rest.starts_with(':') || rest.starts_with(char::is_whitespace)
                {
                    return Some(directive);
                }
            }
        }
        None
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
        let start = self.pos;
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
        // A heredoc needs a label: PHP lexes `<<<` with none (e.g. `<<<"…"`
        // where the next byte is not an identifier) as no heredoc at all.
        // Rewind to just after `<<<` so the bytes pass through verbatim —
        // an empty label would both swallow following lines as the body and
        // (through the quote skip) leave `pos` inside a multi-byte char.
        if label.is_empty() {
            self.pos = start + 3;
            return;
        }
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
/// [`Signature`] (doc §17.1): the first `$name` identifier of each top-level
/// segment is the parameter name — types never contain `$`, and
/// default-value expressions come after the name — a segment containing
/// `...` marks a variadic tail, a segment carrying a top-level `=` marks
/// its parameter optional (see [`Signature::required`]), and a segment
/// typed exactly `string` records its name in
/// [`Signature::typed_strings`] (doc §7.3, `docs/escape-elision.md`).
fn parse_params(params_src: &str) -> Signature {
    let mut params: Vec<String> = vec![];
    let mut required: Vec<String> = vec![];
    let mut typed_strings: Vec<String> = vec![];
    let mut variadic = false;
    for segment in split_top_level(params_src, b',') {
        let trimmed = segment.trim();
        if trimmed.is_empty() {
            continue;
        }
        let is_variadic_segment = trimmed.contains("...");
        if is_variadic_segment {
            variadic = true;
        }
        if let Some(name) = first_dollar_identifier(trimmed) {
            params.push(name.clone());
            // A variadic parameter is optional by nature; any other segment
            // without a top-level `=` declares a required parameter.
            if !is_variadic_segment && !segment_has_default(trimmed) {
                required.push(name);
            }
        }
        if let Some(name) = typed_string_param(trimmed) {
            typed_strings.push(name);
        }
    }
    Signature {
        params,
        variadic,
        required,
        typed_strings,
    }
}

/// The parameter name of a segment typed exactly `string` (doc §7.3 escape
/// elision): the type text before the first `$name` — visibility-promotion
/// modifiers (`public`/`private`/`protected`/`static`/`readonly`) and a
/// trailing `&` stripped, matched case-insensitively like every PHP keyword
/// — must be exactly `string`. Nullable (`?string`), union
/// (`string|int`), qualified (`\App\Name`) and variadic (`string ...$rest`)
/// segments do not qualify, nor does a segment whose type text carries a
/// comment (the stripped text no longer proves the type).
fn typed_string_param(segment: &str) -> Option<String> {
    if segment.contains("...") {
        return None;
    }
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
                let name = &segment[s..e];
                let mut prefix = segment[..i].trim().to_ascii_lowercase();
                loop {
                    let stripped = prefix
                        .strip_prefix("public")
                        .or_else(|| prefix.strip_prefix("private"))
                        .or_else(|| prefix.strip_prefix("protected"))
                        .or_else(|| prefix.strip_prefix("static"))
                        .or_else(|| prefix.strip_prefix("readonly"))
                        .map(str::trim_start);
                    match stripped {
                        Some(next) if !next.is_empty() && next != prefix => {
                            prefix = next.to_string();
                        }
                        _ => break,
                    }
                }
                let prefix = prefix.trim_end_matches('&').trim_end();
                if prefix == "string" {
                    return Some(name.to_string());
                }
                return None;
            }
        }
        i += 1;
    }
    None
}

/// Whether a parameter segment carries a default value: a top-level `=`
/// after the parameter name. Quoted strings and bracket groups are skipped
/// while scanning, so `'a=b'` or `[1, 2]` cannot hide or fake the `=` — and
/// any `=` inside a default expression still proves the default exists.
fn segment_has_default(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    let mut depth = 0usize;
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
            b'=' if depth == 0 => return true,
            _ => {}
        }
        i += 1;
    }
    false
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

/// The first `$name` identifier in a parameter segment: the parameter name
/// and the byte offset of its `$` within the segment. Types never contain
/// `$` (the [`parse_params`] invariant), so the first `$identifier` is the
/// name.
fn first_dollar_span(segment: &str) -> Option<(String, usize)> {
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
                return Some((segment[s..e].to_string(), i));
            }
        }
        i += 1;
    }
    None
}

/// The first `$name` identifier in a parameter segment: the parameter name.
fn first_dollar_identifier(segment: &str) -> Option<String> {
    first_dollar_span(segment).map(|(name, _)| name)
}

/// The `did you mean` help for a mismatched close tag (§21.1): when the
/// written name is a near-miss of the open tag actually in scope — same
/// first character and a small edit distance — the diagnostic names the
/// correct spelling; anything else returns `None` so the plain message
/// stands alone and never guesses.
fn close_tag_help(found: &str, expected: &str) -> Option<String> {
    if !found.starts_with(expected.chars().next()?) {
        return None;
    }
    // Short names tolerate one edit; four or more bytes tolerate two, which
    // covers transpositions (`</sapn>` for `</span>`).
    let limit = if found.len().min(expected.len()) < 4 {
        1
    } else {
        2
    };
    let distance = bounded_edit_distance(found, expected, limit)?;
    Some(format!(
        "did you mean `</{expected}>`? ({distance} edit(s) away)"
    ))
}

/// Bounded Levenshtein distance between two tag names: `None` as soon as
/// every cell of the active row exceeds `limit`, so wildly different names
/// cost O(len) and never produce a suggestion.
fn bounded_edit_distance(a: &str, b: &str, limit: usize) -> Option<usize> {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.len().abs_diff(b.len()) > limit {
        return None;
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur: Vec<usize> = vec![0; b.len() + 1];
    for (i, a_char) in a.iter().enumerate() {
        cur[0] = i + 1;
        let mut row_min = cur[0];
        for j in 1..=b.len() {
            let cost = usize::from(a_char != &b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
            row_min = row_min.min(cur[j]);
        }
        if row_min > limit {
            return None;
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    let distance = prev[b.len()];
    (distance <= limit).then_some(distance)
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
