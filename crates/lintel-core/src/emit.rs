//! String-target lowering: AST → concatenated `new \Lintel\Html(...)` PHP.
//!
//! Lowering walks the tree into a list of `(line offset, Part)` pairs where a
//! part is either a literal string or a dynamic PHP expression. Adjacent
//! literals merge; newlines are interleaved so that the number of newlines in
//! the emitted expression equals the number of newlines in the source span it
//! replaces — the line-preservation invariant (doc 13.4).
//!
//! Literal folding happens here: static attribute strings fold into the
//! literal (only `"` re-escaped, per doc 4.4), single-quoted string children
//! fold through full HTML escaping, integer children fold to literals, and
//! provably-scalar children (`(int)`/`(float)` casts and `count()` calls —
//! doc §7.3 "escape elision, honest version") fold to the raw PHP
//! expression, dropping the `\Lintel\c()` call.

use std::fmt::Write as _;

use crate::ast::{is_void_element, Attr, Element, Node};
use crate::Target;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Part {
    Lit(String),
    Dyn(String),
}

#[derive(Debug, Default)]
struct Parts {
    items: Vec<(u32, Part)>,
}

/// URL attributes routed to the `\Lintel\u()` scheme-allowlist helper (doc 4.4).
const URL_ATTRS: &[&str] = &[
    "href",
    "src",
    "action",
    "formaction",
    "poster",
    "cite",
    "data",
    "manifest",
    "ping",
    "background",
];

/// `srcset` attributes route to their own `\Lintel\srcset()` helper (doc
/// §4.4/§7.1) instead of [`URL_ATTRS`]'s `\Lintel\u()`: the runtime helper
/// splits the candidate list (`url width`, `url density`, …) and
/// scheme-checks each URL, which a single-value attribute escape cannot do.
const SRCSET_ATTR: &str = "srcset";

/// Lowering strategy for one compilation target (doc 5.3).
///
/// The scanner lowers each JSX root as it parses it, so a lowering is a
/// synchronous transform of one parsed root into the PHP expression that
/// replaces it in the source. New targets (e.g. the future `stream`
/// generator lowering) implement this trait and are wired up through
/// [`lowering_for`].
pub trait Lowering {
    /// Lower a parsed JSX root into the PHP expression that replaces it.
    ///
    /// `total_lines` is the number of newlines inside the replaced source
    /// span; the returned expression must contain the same number so the
    /// file's line numbers are preserved (doc 13.4).
    fn emit_root(&self, node: &Node, total_lines: u32) -> String;
}

/// String-concat AOT lowering (default, doc 5.3): the root becomes one
/// `new \Lintel\Html(...)` value built from merged literal parts and
/// dynamic `\Lintel\*()` helper calls.
#[derive(Debug, Default)]
pub struct StringLowering;

impl Lowering for StringLowering {
    fn emit_root(&self, node: &Node, total_lines: u32) -> String {
        let mut parts = Parts::default();
        lower(node, &mut parts);
        format!("new \\Lintel\\Html({})", concat(parts, 0, total_lines))
    }
}

/// Streaming lowering (doc 5.3 `stream`, §10): the root becomes an
/// immediately-invoked `static function (): \Generator` closure whose body
/// yields the markup one chunk at a time — literal runs become quoted string
/// chunks, dynamic children/attributes ride through the same `\Lintel\`
/// helpers as the string target, and component calls stay wrapped in
/// `\Lintel\c()` so a component may return either a `\Generator` (streaming)
/// or an `\Lintel\Html` (sync). The exact contract, including the deliberate
/// deviations from doc §10, lives in `docs/stream-alignment.md`.
#[derive(Debug, Default)]
pub struct StreamLowering;

impl Lowering for StreamLowering {
    fn emit_root(&self, node: &Node, total_lines: u32) -> String {
        let mut parts = Parts::default();
        match node {
            Node::Element(element) if element.is_component => stream_component(element, &mut parts),
            Node::Element(element) => html_element(element, &mut parts),
            Node::Fragment { children, .. } => {
                for child in children {
                    lower_stream(child, &mut parts);
                }
            }
            Node::Text { text, line, .. } => parts.items.push((*line, Part::Lit(text.clone()))),
            Node::Expr { php, line, .. } => parts.items.push((*line, dyn_child(php))),
        }
        stream_closure(&concat_stream(parts, 0, total_lines))
    }
}

/// RFC-parity lowering (doc 5.3 `rfc`, §0.1/§4.9): markup lowers to
/// `new \Markup\Element('tag', [attrs], [children])`; components become
/// plain PHP calls with named arguments; a fragment root becomes a plain
/// PHP array of children.
///
/// No `\Lintel\` runtime helpers are emitted: escaping under the RFC model
/// is the runtime's contract (escape-by-default), so attribute values and
/// text children pass through as raw PHP strings/expressions. The exact
/// lowering names and this crate's documented deviations live in
/// `docs/rfc-alignment.md`.
#[derive(Debug, Default)]
pub struct RfcLowering;

impl Lowering for RfcLowering {
    fn emit_root(&self, node: &Node, total_lines: u32) -> String {
        let mut parts = Parts::default();
        match node {
            Node::Element(element) => rfc_node_value(element, &mut parts),
            Node::Fragment { children, line, .. } => {
                parts.items.push((*line, Part::Lit("[".into())));
                let mut first = true;
                for child in children {
                    rfc_child(child, &mut parts, &mut first);
                }
                parts.items.push((*line, Part::Lit("]".into())));
            }
            Node::Text { text, line, .. } => {
                parts.items.push((*line, Part::Lit(php_str(text))));
            }
            Node::Expr { php, line, .. } => {
                parts.items.push((*line, rfc_expr_child(php)));
            }
        }
        concat_rfc(parts, 0, total_lines)
    }
}

/// The lowering strategy registered for a [`Target`] (doc 5.3).
#[must_use]
pub fn lowering_for(target: Target) -> &'static dyn Lowering {
    match target {
        Target::String => &StringLowering,
        Target::Stream => &StreamLowering,
        Target::Rfc => &RfcLowering,
    }
}

/// Push one array/argument entry, inserting a `", "` separator before it
/// unless it is the first entry of its list.
fn rfc_entry(parts: &mut Parts, first: &mut bool, line: u32, part: Part) {
    if !*first {
        parts.items.push((line, Part::Lit(", ".into())));
    }
    *first = false;
    parts.items.push((line, part));
}

/// RFC array entry for an attribute (or spread): `'name' => value` / `true`
/// / raw expression / `...$spread`.
fn rfc_attr(attr: &Attr, parts: &mut Parts, first: &mut bool) {
    match attr {
        Attr::Str {
            name, value, line, ..
        } => {
            let entry = Part::Lit(format!("{} => {}", php_str(name), php_str(value)));
            rfc_entry(parts, first, *line, entry);
        }
        Attr::Bool { name, line, .. } => {
            let entry = Part::Lit(format!("{} => true", php_str(name)));
            rfc_entry(parts, first, *line, entry);
        }
        Attr::Expr {
            name, php, line, ..
        } => {
            let entry = Part::Dyn(format!("{} => {}", php_str(name), php.trim()));
            rfc_entry(parts, first, *line, entry);
        }
        Attr::Spread { php, line, .. } => {
            let entry = Part::Dyn(format!("...{}", php.trim()));
            rfc_entry(parts, first, *line, entry);
        }
    }
}

/// RFC value for a child expression: folded literal (string/integer) or the
/// raw PHP expression — never a `\Lintel\` helper call.
fn rfc_expr_child(php: &str) -> Part {
    let trimmed = php.trim();
    if let Some(inner) = simple_single_quoted(trimmed) {
        return Part::Lit(php_str(&inner));
    }
    if trimmed.parse::<i64>().is_ok() {
        return Part::Lit(trimmed.to_string());
    }
    Part::Dyn(trimmed.to_string())
}

/// RFC children-list entries: text and expressions become array entries,
/// fragments flatten, nested elements/components become values.
fn rfc_child(node: &Node, parts: &mut Parts, first: &mut bool) {
    match node {
        Node::Fragment { children, .. } => {
            for child in children {
                rfc_child(child, parts, first);
            }
        }
        Node::Text { text, line, .. } => {
            rfc_entry(parts, first, *line, Part::Lit(php_str(text)));
        }
        Node::Expr { php, line, .. } => {
            rfc_entry(parts, first, *line, rfc_expr_child(php));
        }
        Node::Element(element) => {
            rfc_entry(parts, first, element.line, Part::Lit(String::new()));
            rfc_node_value(element, parts);
        }
    }
}

/// Lower an element (HTML or component) as one RFC markup value.
fn rfc_node_value(element: &Element, parts: &mut Parts) {
    if element.is_component {
        rfc_component(element, parts);
    } else {
        rfc_html_element(element, parts);
    }
}

/// `new \Markup\Element('tag', [attrs], [children])` (doc 0.1).
fn rfc_html_element(element: &Element, parts: &mut Parts) {
    parts.items.push((
        element.line,
        Part::Lit(format!(
            "new \\Markup\\Element({}, [",
            php_str(&element.name)
        )),
    ));
    let mut first = true;
    for attr in &element.attrs {
        rfc_attr(attr, parts, &mut first);
    }
    parts
        .items
        .push((element.end_line, Part::Lit("], [".into())));
    let mut child_first = true;
    for child in &element.children {
        rfc_child(child, parts, &mut child_first);
    }
    parts.items.push((element.end_line, Part::Lit("])".into())));
}

/// `Component(arg: value, ..., children: [...])` — components are called
/// with named arguments under the RFC model (doc 4.7).
fn rfc_component(element: &Element, parts: &mut Parts) {
    parts
        .items
        .push((element.line, Part::Lit(format!("{}(", element.name))));
    let mut first = true;
    for attr in &element.attrs {
        match attr {
            Attr::Str {
                name, value, line, ..
            } => {
                let entry = Part::Lit(format!("{name}: {}", php_str(value)));
                rfc_entry(parts, &mut first, *line, entry);
            }
            Attr::Bool { name, line, .. } => {
                let entry = Part::Lit(format!("{name}: true"));
                rfc_entry(parts, &mut first, *line, entry);
            }
            Attr::Expr {
                name, php, line, ..
            } => {
                let entry = Part::Dyn(format!("{name}: {}", php.trim()));
                rfc_entry(parts, &mut first, *line, entry);
            }
            Attr::Spread { php, line, .. } => {
                let entry = Part::Dyn(format!("...{}", php.trim()));
                rfc_entry(parts, &mut first, *line, entry);
            }
        }
    }
    if !element.children.is_empty() {
        if !first {
            parts.items.push((element.line, Part::Lit(", ".into())));
        }
        parts
            .items
            .push((element.line, Part::Lit("children: [".into())));
        let mut child_first = true;
        for child in &element.children {
            rfc_child(child, parts, &mut child_first);
        }
        parts.items.push((element.end_line, Part::Lit("]".into())));
    }
    parts.items.push((element.end_line, Part::Lit(")".into())));
}

fn lower(node: &Node, parts: &mut Parts) {
    match node {
        Node::Text { text, line, .. } => parts.items.push((*line, Part::Lit(text.clone()))),
        Node::Expr { php, line, .. } => parts.items.push((*line, dyn_child(php))),
        Node::Fragment { children, .. } => {
            for child in children {
                lower(child, parts);
            }
        }
        Node::Element(element) if element.is_component => component(element, parts),
        Node::Element(element) => html_element(element, parts),
    }
}

/// Stream-target walk: mirrors [`lower`] chunk-for-chunk — markup, text and
/// dynamic children emit the same parts — but component invocations keep
/// their children argument as a generator closure instead of an
/// `new \Lintel\Html(...)` value (doc 5.3 `stream`).
fn lower_stream(node: &Node, parts: &mut Parts) {
    match node {
        Node::Text { text, line, .. } => parts.items.push((*line, Part::Lit(text.clone()))),
        Node::Expr { php, line, .. } => parts.items.push((*line, dyn_child(php))),
        Node::Fragment { children, .. } => {
            for child in children {
                lower_stream(child, parts);
            }
        }
        Node::Element(element) if element.is_component => stream_component(element, parts),
        Node::Element(element) => html_element(element, parts),
    }
}

/// Fold trivially-literal child expressions at compile time.
fn dyn_child(php: &str) -> Part {
    let trimmed = php.trim();
    if let Some(inner) = simple_single_quoted(trimmed) {
        return Part::Lit(html_escape(&inner));
    }
    if trimmed.parse::<i64>().is_ok() {
        return Part::Lit(trimmed.to_string());
    }
    if is_scalar_elidable(trimmed) {
        // Provably int/float (doc §7.3): the raw expression cannot carry
        // markup, so the `\Lintel\c()` call is pure overhead.
        return Part::Dyn(trimmed.to_string());
    }
    Part::Dyn(format!("\\Lintel\\c({trimmed})"))
}

/// Whether a dynamic child expression is provably a non-string scalar at
/// compile time (architecture doc §7.3, "escape elision (honest version)",
/// v0.1 subset): an integer/float cast spanning the whole expression, or a
/// `count()` call. An int/float interpolates as digits — it cannot carry
/// markup — so the child folds to the raw PHP expression instead of the
/// `\Lintel\c()` escape helper, on both markup targets (the `rfc` target is
/// helper-free by contract).
///
/// Conservative by design: any top-level binary operator, unknown function
/// call, or `(string)` cast keeps the wrapper. False negatives cost one
/// helper call at render time; false positives would smuggle markup into
/// the output.
fn is_scalar_elidable(expr: &str) -> bool {
    is_scalar_cast(expr) || is_count_call(expr)
}

/// Whether `expr` is exactly a PHP integer/float cast — `(int)`, `(integer)`,
/// `(float)` or `(double)`, case-insensitive like every PHP keyword —
/// followed by one operand atom the cast fully covers: a `$var` accessor
/// chain, a balanced parenthesized group, a literal/constant/call atom, or
/// another cast. A top-level binary operator after the atom keeps the
/// wrapper: `(int)$a . $b` is a string concat (`((int)$a) . $b`), not a
/// cast of the whole expression, and its result can carry markup.
fn is_scalar_cast(expr: &str) -> bool {
    cast_rest(expr.trim()).is_some_and(is_cast_operand)
}

/// The part of `expr` after a leading integer/float cast keyword, or `None`.
/// Aliases are matched longest-first so `(integer)` is not read as `(int)`.
fn cast_rest(expr: &str) -> Option<&str> {
    let lower = expr.to_ascii_lowercase();
    let len = if lower.starts_with("(integer)") {
        "(integer)".len()
    } else if lower.starts_with("(double)") {
        "(double)".len()
    } else if lower.starts_with("(int)") {
        "(int)".len()
    } else if lower.starts_with("(float)") {
        "(float)".len()
    } else {
        return None;
    };
    Some(&expr[len..])
}

/// Whether `rest` is one operand atom a cast fully covers.
fn is_cast_operand(rest: &str) -> bool {
    let rest = rest.trim_start();
    if rest.is_empty() {
        return false;
    }
    if cast_rest(rest).is_some() {
        // Another cast: recurse, so `(int)(float)$x` elides too.
        return is_scalar_cast(rest);
    }
    let bytes = rest.as_bytes();
    match bytes[0] {
        b'$' => var_chain_exhausts(rest),
        b'(' => balanced_group_exhausts(rest),
        b'+' | b'-' => is_cast_operand(&rest[1..]), // unary sign
        b'0'..=b'9' => numeric_literal_exhausts(rest),
        b'_' | b'a'..=b'z' | b'A'..=b'Z' | 0x80..=0xff => ident_atom_exhausts(rest),
        _ => false,
    }
}

/// Whether a `$var` expression is one accessor chain that consumes the whole
/// string: `$name`, then repeated `->prop`, `->method(...)` and `[...]`
/// links. Links are matched without intervening whitespace, so
/// `(int)$row -> count` conservatively stays wrapped.
fn var_chain_exhausts(rest: &str) -> bool {
    let after_dollar = &rest[1..];
    let name_len = php_ident_len(after_dollar);
    if name_len == 0 {
        return false;
    }
    let mut rest = &after_dollar[name_len..];
    loop {
        if rest.is_empty() {
            return true;
        }
        if let Some(after) = rest.strip_prefix("->") {
            let len = php_ident_len(after);
            if len == 0 {
                return false;
            }
            rest = &after[len..];
            if rest.starts_with('(') {
                match balanced_prefix(rest, b'(', b')') {
                    Some(after_call) => rest = after_call,
                    None => return false,
                }
            }
            continue;
        }
        if rest.starts_with('[') {
            match balanced_prefix(rest, b'[', b']') {
                Some(after_index) => rest = after_index,
                None => return false,
            }
            continue;
        }
        return false;
    }
}

/// Whether `rest` is a balanced parenthesized group with nothing after it:
/// the cast covers whatever binary operators hide inside the group.
fn balanced_group_exhausts(rest: &str) -> bool {
    balanced_prefix(rest, b'(', b')').is_some_and(|after| after.trim().is_empty())
}

/// Whether `rest` is a numeric literal that exhausts the string (`5`, `1.5`,
/// `0x1A`, with the exponent/sign bytes the PHP lexer accepts inside one).
fn numeric_literal_exhausts(rest: &str) -> bool {
    let len = rest
        .as_bytes()
        .iter()
        .take_while(|c| c.is_ascii_alphanumeric() || **c == b'.' || **c == b'_')
        .count();
    rest[len..].trim().is_empty()
}

/// Whether a bare identifier atom exhausts `rest`: a constant
/// (`PHP_INT_MAX`) or a function call closing at the end (`strlen($s)`).
/// The cast sanitizes either result type, so the callee stays unknown.
fn ident_atom_exhausts(rest: &str) -> bool {
    let len = php_ident_len(rest);
    let after = &rest[len..];
    if after.is_empty() {
        return true; // a constant
    }
    after.starts_with('(') && balanced_group_exhausts(after)
}

/// Whether `expr` is exactly a `count(...)` call (case-insensitive, like
/// every PHP function name): `count` always returns an int, whatever the
/// argument, so the child folds raw. The `sizeof` alias deliberately stays
/// wrapped — the doc's v0.1 list names `count()` only.
fn is_count_call(expr: &str) -> bool {
    let expr = expr.trim();
    expr.len() > "count()".len()
        && expr.to_ascii_lowercase().starts_with("count(")
        && balanced_group_exhausts(expr)
}

/// Length of the PHP identifier prefix of `src`: ASCII alphanumerics,
/// underscores and backslashes (namespace paths); byte-oriented like the
/// scanner, which slices only at ASCII-delimited boundaries.
fn php_ident_len(src: &str) -> usize {
    src.as_bytes()
        .iter()
        .take_while(|c| c.is_ascii_alphanumeric() || **c == b'_' || **c == b'\\')
        .count()
}

/// Consume one balanced bracket group (`open`…`close`) at the start of
/// `src`, skipping quoted strings; `Some(rest)` just past the closing
/// bracket, or `None` when the group never closes.
fn balanced_prefix(src: &str, open: u8, close: u8) -> Option<&str> {
    let bytes = src.as_bytes();
    let mut depth = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            q @ (b'\'' | b'"') => {
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == q {
                        break;
                    }
                    i += 1;
                }
            }
            c if c == open => depth += 1,
            c if c == close => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(&src[i + 1..]);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn html_element(element: &Element, parts: &mut Parts) {
    let tag = element.name.as_str();
    parts
        .items
        .push((element.line, Part::Lit(format!("<{tag}"))));
    for attr in &element.attrs {
        push_attr(attr, parts);
    }
    if element.self_closing || is_void_element(tag) {
        if is_void_element(tag) {
            parts.items.push((element.line, Part::Lit(">".into())));
        } else {
            parts
                .items
                .push((element.line, Part::Lit(format!("></{tag}>"))));
        }
        return;
    }
    parts.items.push((element.line, Part::Lit(">".into())));
    for child in &element.children {
        lower(child, parts);
    }
    parts
        .items
        .push((element.end_line, Part::Lit(format!("</{tag}>"))));
}

fn push_attr(attr: &Attr, parts: &mut Parts) {
    match attr {
        Attr::Str {
            name, value, line, ..
        } => {
            parts.items.push((
                *line,
                Part::Lit(format!(" {name}=\"{}\"", value.replace('"', "&quot;"))),
            ));
        }
        Attr::Bool { name, line, .. } => {
            parts.items.push((*line, Part::Lit(format!(" {name}"))));
        }
        Attr::Spread { php, line, .. } => {
            parts
                .items
                .push((*line, Part::Dyn(format!("\\Lintel\\attrs({})", php.trim()))));
        }
        Attr::Expr {
            name, php, line, ..
        } => {
            let trimmed = php.trim();
            if let Some(inner) = simple_single_quoted(trimmed) {
                parts.items.push((
                    *line,
                    Part::Lit(format!(" {name}=\"{}\"", html_escape(&inner))),
                ));
                return;
            }
            let lower_name = name.to_ascii_lowercase();
            let call = if lower_name == SRCSET_ATTR {
                format!("\\Lintel\\srcset('{name}', {trimmed})")
            } else if URL_ATTRS.contains(&lower_name.as_str()) {
                format!("\\Lintel\\u('{name}', {trimmed})")
            } else if lower_name == "class" {
                format!("\\Lintel\\cls({trimmed})")
            } else if lower_name == "style" {
                format!("\\Lintel\\sty({trimmed})")
            } else {
                format!("\\Lintel\\a('{name}', {trimmed})")
            };
            parts.items.push((*line, Part::Dyn(call)));
        }
    }
}

fn component(element: &Element, parts: &mut Parts) {
    let children = if element.children.is_empty() {
        None
    } else {
        let mut inner = Parts::default();
        for child in &element.children {
            lower(child, &mut inner);
        }
        Some(format!(
            "new \\Lintel\\Html({})",
            concat(inner, element.line, element.end_line)
        ))
    };
    component_call(element, children, parts);
}

/// Stream-target component invocation: the `children:` named argument is a
/// generator closure yielding the children chunks, so a component body can
/// iterate/`yield from` it (doc 5.3 `stream`); the call itself stays wrapped
/// in `\Lintel\c()` so returning `\Generator` or `\Lintel\Html` both render.
fn stream_component(element: &Element, parts: &mut Parts) {
    let children = if element.children.is_empty() {
        None
    } else {
        let mut inner = Parts::default();
        for child in &element.children {
            lower_stream(child, &mut inner);
        }
        Some(stream_closure(&concat_stream(
            inner,
            element.line,
            element.end_line,
        )))
    };
    component_call(element, children, parts);
}

/// Shared component-call emit: attributes become named arguments, the built
/// `children` value (if any) is appended last, and the whole call is wrapped
/// in `\Lintel\c()` (doc 4.7).
fn component_call(element: &Element, children: Option<String>, parts: &mut Parts) {
    let mut args: Vec<String> = vec![];
    for attr in &element.attrs {
        match attr {
            Attr::Str { name, value, .. } => {
                args.push(format!("{name}: {}", php_str(value.as_str())));
            }
            Attr::Bool { name, .. } => {
                args.push(format!("{name}: true"));
            }
            Attr::Expr { name, php, .. } => {
                args.push(format!("{name}: {}", php.trim()));
            }
            Attr::Spread { php, .. } => {
                args.push(format!("...({})", php.trim()));
            }
        }
    }
    if let Some(children) = children {
        args.push(format!("children: {children}"));
    }
    parts.items.push((
        element.line,
        Part::Dyn(format!(
            "\\Lintel\\c({}({}))",
            element.name,
            args.join(", ")
        )),
    ));
}

/// Merge adjacent literals, interleave newlines so emitted line numbers match
/// the source span (doc 6.5 `concat`, with the separator placed after the
/// newline so lines start with ` . ` like the reference golden).
fn concat(parts: Parts, base: u32, total: u32) -> String {
    concat_parts(parts, base, total, " . ", true)
}

/// [`concat`] for the RFC lowering: the RFC emitters build final PHP text
/// (already string-quoted where needed) and carry their own list separators,
/// so parts are emitted verbatim with no inter-part separator.
fn concat_rfc(parts: Parts, base: u32, total: u32) -> String {
    concat_parts(parts, base, total, "", false)
}

/// [`concat`] for the streaming lowering: each merged part becomes one
/// `yield` statement — literal parts are PHP-quoted string chunks, dynamic
/// parts are yielded as-is — so the generator produces one chunk per literal
/// run / dynamic slot (doc 5.3 `stream`).
fn concat_stream(parts: Parts, base: u32, total: u32) -> String {
    let merged = merge_adjacent(parts.items);
    let mut out = String::new();
    let mut cur = base;
    let mut first = true;
    if merged.is_empty() {
        out.push_str("yield '';");
    }
    for (line, part) in merged {
        while cur < base.max(line) {
            out.push('\n');
            cur += 1;
        }
        if !first {
            out.push(' ');
        }
        match part {
            Part::Lit(text) => {
                cur += u32::try_from(text.matches('\n').count()).unwrap_or(u32::MAX);
                let _ = write!(out, "yield {};", php_str(&text));
            }
            Part::Dyn(code) => {
                cur += u32::try_from(code.matches('\n').count()).unwrap_or(u32::MAX);
                let _ = write!(out, "yield {code};");
            }
        }
        first = false;
    }
    while cur < total {
        out.push('\n');
        cur += 1;
    }
    out
}

/// Wrap a lowered body in the immediately-invoked generator closure the
/// scanner splices in place of a stream-target JSX root (doc 5.3, §10).
fn stream_closure(body: &str) -> String {
    format!("(static function (): \\Generator {{ {body} }})()")
}

/// Shared line-preserving concat: merge adjacent literals, interleave
/// newlines, and either PHP-quote literal parts (`quote_literals`, string
/// target) or emit them verbatim (RFC target).
fn concat_parts(
    parts: Parts,
    base: u32,
    total: u32,
    separator: &str,
    quote_literals: bool,
) -> String {
    let merged = merge_adjacent(parts.items);

    let mut out = String::new();
    let mut cur = base;
    let mut first = true;
    if merged.is_empty() {
        out.push_str("''");
    }
    for (line, part) in merged {
        while cur < base.max(line) {
            out.push('\n');
            cur += 1;
        }
        if !first {
            out.push_str(separator);
        }
        match part {
            Part::Lit(text) => {
                cur += u32::try_from(text.matches('\n').count()).unwrap_or(u32::MAX);
                if quote_literals {
                    out.push_str(&php_str(&text));
                } else {
                    out.push_str(&text);
                }
            }
            Part::Dyn(code) => {
                cur += u32::try_from(code.matches('\n').count()).unwrap_or(u32::MAX);
                out.push_str(&code);
            }
        }
        first = false;
    }
    while cur < total {
        out.push('\n');
        cur += 1;
    }
    out
}

/// Merge adjacent literal parts into single chunks: one string operand /
/// yielded chunk per literal run, with the first part's line kept (doc 6.5).
fn merge_adjacent(items: Vec<(u32, Part)>) -> Vec<(u32, Part)> {
    let mut merged: Vec<(u32, Part)> = vec![];
    for (line, part) in items {
        match (merged.last_mut(), &part) {
            (Some((_, Part::Lit(existing))), Part::Lit(next)) => existing.push_str(next),
            (_, _) => merged.push((line, part)),
        }
    }
    merged
}

/// A PHP single-quoted string literal with `\` and `'` escaped.
fn php_str(text: &str) -> String {
    format!("'{}'", text.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// Full HTML escaping used when literal children fold at compile time.
fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#039;")
}

/// `'literal'` with no escapes inside — the only foldable expression form.
fn simple_single_quoted(expr: &str) -> Option<String> {
    let bytes = expr.as_bytes();
    if bytes.len() >= 2
        && bytes[0] == b'\''
        && bytes[bytes.len() - 1] == b'\''
        && !expr[1..expr.len() - 1].contains(['\\', '\''])
    {
        Some(expr[1..expr.len() - 1].to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{is_count_call, is_scalar_cast, is_scalar_elidable};

    #[test]
    fn casts_spanning_the_whole_expression_elide() {
        for expr in [
            "(int)$n",
            "(INT)$n",
            "(integer)$n",
            "(float)$rate",
            "(double)$rate",
            "  (int) $n  ",
            "(int)$row->count",
            "(int)$row['key']",
            "(int)$row->cells()",
            "(int)($a . $b)",
            "(int)((float)$x)",
            "(int)(float)$x",
            "(int)PHP_INT_MAX",
            "(int)strlen($s)",
            "(int)-$n",
            "(int)1.5",
            "(int)-1",
        ] {
            assert!(is_scalar_cast(expr), "{expr} should elide");
        }
    }

    #[test]
    fn top_level_operators_keep_the_wrapper() {
        // The cast binds its atom, so anything after it is a string context.
        for expr in [
            "(int)$n . $suffix",
            "(int)$n++",
            "(int)$n ?? 0",
            "(int)$n, $m",
            "(int)$n)",
            "(int)($a) . $b",
            "(int)$a -> count",
        ] {
            assert!(!is_scalar_cast(expr), "{expr} must not elide");
        }
    }

    #[test]
    fn string_casts_and_broken_casts_never_elide() {
        for expr in ["(string)$s", "(int)", "(int))", "(int$", "(integerish)"] {
            assert!(!is_scalar_cast(expr), "{expr} must not elide");
        }
    }

    #[test]
    fn count_calls_elide_only_when_they_exhaust_the_expression() {
        assert!(is_count_call("count($rows)"));
        assert!(is_count_call("COUNT($rows)"));
        assert!(is_count_call("count( $rows, COUNT_RECURSIVE )"));
        assert!(!is_count_call("count($rows) . ' rows'"));
        assert!(!is_count_call("counta($rows)"));
        assert!(!is_count_call("\\count($rows)"));
    }

    #[test]
    fn elision_is_cast_or_count_only() {
        assert!(is_scalar_elidable("(int)$n"));
        assert!(is_scalar_elidable("count($rows)"));
        assert!(!is_scalar_elidable("$n"));
        assert!(!is_scalar_elidable("(string)$s"));
        assert!(!is_scalar_elidable("strlen($s)"));
    }
}
