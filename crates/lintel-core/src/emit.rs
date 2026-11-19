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

/// Lower a root node into `new \Lintel\Html(...)` PHP, padding with newlines
/// until the emitted expression spans `total_lines` (the newline count of the
/// source span it replaces).
pub(crate) fn emit_root(node: &Node, total_lines: u32) -> String {
    let mut parts = Parts::default();
    lower(node, &mut parts);
    format!("new \\Lintel\\Html({})", concat(parts, 0, total_lines))
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
            out.push_str(" . ");
        }
        match part {
            Part::Lit(text) => {
                cur += u32::try_from(text.matches('\n').count()).unwrap_or(u32::MAX);
                out.push_str(&php_str(&text));
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
