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
//! fold through full HTML escaping, and integer children fold to literals.

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

/// Fold trivially-literal child expressions at compile time.
fn dyn_child(php: &str) -> Part {
    let trimmed = php.trim();
    if let Some(inner) = simple_single_quoted(trimmed) {
        return Part::Lit(html_escape(&inner));
    }
    if trimmed.parse::<i64>().is_ok() {
        return Part::Lit(trimmed.to_string());
    }
    Part::Dyn(format!("\\Lintel\\c({trimmed})"))
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
            let call = if URL_ATTRS.contains(&lower_name.as_str()) {
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
    if !element.children.is_empty() {
        let mut inner = Parts::default();
        for child in &element.children {
            lower(child, &mut inner);
        }
        args.push(format!(
            "children: new \\Lintel\\Html({})",
            concat(inner, element.line, element.end_line)
        ));
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
    let mut merged: Vec<(u32, Part)> = vec![];
    for (line, part) in parts.items {
        match (merged.last_mut(), &part) {
            (Some((_, Part::Lit(existing))), Part::Lit(next)) => existing.push_str(next),
            (_, _) => merged.push((line, part)),
        }
    }

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
