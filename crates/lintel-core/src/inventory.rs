//! The component inventory: a read-only shape report for one scanned file
//! (`docs/component-inventory.md`).
//!
//! Consumers that describe a file instead of compiling it — LSP outlines,
//! metrics collectors, doc generators, dashboards — need the same two
//! lists over and over: the components the file **declares** (with their
//! parameter lists, doc §17.1) and the components it **invokes** (with the
//! source position and prop names). Both are derivable from a
//! [`crate::scan::Parsed`], but re-deriving them means re-walking the roots
//! and re-reading the context tables; [`collect_inventory`] does it once,
//! on the compiler's own data, and the CLI's `lintel stats` command ships
//! the result.
//!
//! The report is infallible and read-only like the [`crate::NodeVisitor`]
//! walk it is built on: it cannot fail and has no error channel, it never
//! consults the suppression directives (`docs/ignore-directives.md`) — a
//! report of the file's shape stays complete where validation output is
//! silenced — and every position it reports is an absolute byte offset, so
//! [`lintel_diag::SourceMap`] maps it to editor positions without
//! re-scanning.

use std::fmt::Write as _;

use lintel_diag::json_escape;

use crate::ast::{visit_roots, Node, NodeVisitor};
use crate::scan::{ScanContext, Signature};

/// One component the scanned file declares: the function's last path
/// segment in the author's spelling (the signature table's key — PHP
/// function lookups are case-insensitive, see
/// [`ScanContext::signature_of`]) and its recorded parameter list.
///
/// Every declared function is listed, whether or not any JSX in the file
/// invokes it — helpers like `render_header` are declared functions too,
/// and the component-shaped subset is the consumer's filter, not the
/// compiler's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredComponent {
    /// The declared name, as written in the file (last path segment).
    pub name: String,
    /// The recorded parameter list (doc §17.1): parameter names and order,
    /// which parameters are required, whether the list is variadic, and
    /// which parameters are non-nullable `string` (doc §7.3).
    pub signature: Signature,
}

/// One component invocation site in the scanned file (a JSX element whose
/// name is component-shaped: capitalized first letter or containing `\`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    /// The tag name as written, e.g. `"Card"`, `"Ui\\Button"`.
    pub name: String,
    /// Absolute byte offset of the invocation's opening `<` — the same
    /// anchor every [`crate::Diagnostic`] carries, so
    /// [`lintel_diag::SourceMap`] maps it to line/column without
    /// re-scanning.
    pub offset: usize,
    /// The named attributes in source order (static, boolean and dynamic);
    /// a spread carries no name and is not listed here — see `spread`.
    pub props: Vec<String>,
    /// Whether the invocation carries JSX children.
    pub children: bool,
    /// Whether any attribute is a spread (`{...$attrs}`): the invocation
    /// may pass arbitrary named arguments.
    pub spread: bool,
    /// Whether a function with this name is declared in the same file
    /// (the signature table, doc §17.1): `false` for a component defined
    /// in another file, where prop checks are impossible.
    pub declared_in_file: bool,
}

/// The component shape of one scanned file: what it declares and what it
/// invokes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileInventory {
    /// Declared components, in declaration order.
    pub declared: Vec<DeclaredComponent>,
    /// Invocation sites, in source order (sorted by [`Invocation::offset`];
    /// the visitor walk is source-ordered within each root, and a JSX root
    /// nested inside an attribute expression is scanned before its
    /// enclosing root, so the sort restores the true source order — see
    /// `visit_roots`).
    pub invocations: Vec<Invocation>,
}

impl FileInventory {
    /// Render the inventory as a single-line JSON object (JSON Lines), the
    /// machine-readable counterpart of the struct — the same contract as
    /// [`lintel_diag::Diagnostic::to_json`]: no trailing newline, stable
    /// field order (`declared` first, then `invocations`; each declared
    /// entry is `name`, `params`, `required`, `variadic`, `typed_strings`;
    /// each invocation entry is `name`, `offset`, `line`, `col`, `props`,
    /// `children`, `spread`, `declared_in_file`), renderer-parity positions
    /// from [`lintel_diag::SourceMap`] (1-based line, 1-based character
    /// column, out-of-range offsets clamped) and the shared
    /// [`lintel_diag::json_escape`] escaping. String values may never panic:
    /// `src` only feeds the position mapping.
    #[must_use]
    pub fn to_json(&self, src: &str) -> String {
        let map = lintel_diag::SourceMap::new(src);
        let mut out = String::with_capacity(128);
        out.push_str("{\"declared\":[");
        for (index, component) in self.declared.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str("{\"name\":\"");
            json_escape(&component.name, &mut out);
            out.push_str("\",\"params\":[");
            json_string_list(&component.signature.params, &mut out);
            out.push_str("],\"required\":[");
            json_string_list(&component.signature.required, &mut out);
            out.push_str("],\"variadic\":");
            out.push_str(if component.signature.variadic {
                "true"
            } else {
                "false"
            });
            out.push_str(",\"typed_strings\":[");
            json_string_list(&component.signature.typed_strings, &mut out);
            out.push_str("]}");
        }
        out.push_str("],\"invocations\":[");
        for (index, site) in self.invocations.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            let (line, col) = map.line_col(site.offset);
            out.push_str("{\"name\":\"");
            json_escape(&site.name, &mut out);
            out.push_str("\",\"offset\":");
            let _ = write!(out, "{}", site.offset);
            out.push_str(",\"line\":");
            let _ = write!(out, "{line}");
            out.push_str(",\"col\":");
            let _ = write!(out, "{col}");
            out.push_str(",\"props\":[");
            json_string_list(&site.props, &mut out);
            out.push_str("],\"children\":");
            out.push_str(if site.children { "true" } else { "false" });
            out.push_str(",\"spread\":");
            out.push_str(if site.spread { "true" } else { "false" });
            out.push_str(",\"declared_in_file\":");
            out.push_str(if site.declared_in_file {
                "true"
            } else {
                "false"
            });
            out.push('}');
        }
        out.push_str("]}");
        out
    }
}

/// Write a list of strings as a JSON array of escaped strings.
fn json_string_list(values: &[String], out: &mut String) {
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push('"');
        json_escape(value, out);
        out.push('"');
    }
}

/// A [`NodeVisitor`] that collects every component invocation it visits —
/// the public half of [`collect_inventory`]'s walk, reusable when a
/// consumer already drives its own [`NodeVisitor`] pass and wants the
/// invocation list in the same walk instead of a second traversal. Feed it
/// with [`visit_node`]/[`visit_roots`], then call [`InventoryBuilder::finish`].
///
/// The builder is the compiler's own shape knowledge in visitor form: a
/// component element (capitalized first letter or containing `\`) records
/// its name, offset, named props (spreads set `spread` instead), children
/// flag and in-file declaration flag ([`ScanContext::signature_of`]).
pub struct InventoryBuilder<'c> {
    context: &'c ScanContext,
    invocations: Vec<Invocation>,
}

impl<'c> InventoryBuilder<'c> {
    /// Build against a scan context (for the `declared_in_file` lookups).
    #[must_use]
    pub fn new(context: &'c ScanContext) -> Self {
        Self {
            context,
            invocations: vec![],
        }
    }

    /// The collected invocation sites, in visit order. [`collect_inventory`]
    /// sorts them by offset; a consumer driving its own walk keeps visit
    /// order and can sort the same way.
    #[must_use]
    pub fn finish(self) -> Vec<Invocation> {
        self.invocations
    }

    /// Record one invocation site for a component element.
    fn record(&mut self, element: &crate::ast::Element) {
        let props = element
            .attrs
            .iter()
            .filter_map(crate::ast::Attr::name)
            .map(str::to_string)
            .collect();
        self.invocations.push(Invocation {
            name: element.name.clone(),
            offset: element.offset,
            props,
            children: !element.children.is_empty(),
            spread: element
                .attrs
                .iter()
                .any(|attr| matches!(attr, crate::ast::Attr::Spread { .. })),
            declared_in_file: self.context.signature_of(&element.name).is_some(),
        });
    }
}

impl NodeVisitor for InventoryBuilder<'_> {
    fn visit(&mut self, node: &Node) {
        if let Node::Element(element) = node {
            if element.is_component {
                self.record(element);
            }
        }
    }
}

/// Inventory one scanned file: every component the file declares (the
/// signature table, doc §17.1) and every component it invokes, over all
/// roots — top-level and nested in attribute expressions alike.
///
/// The traversal is the [`crate::NodeVisitor`] contract walk
/// ([`visit_roots`]), so the visit sequence is exactly the validation
/// pass's; the invocation list is then sorted by source offset, which makes
/// the report source-ordered across roots (the walk order is
/// source-ordered only within a root — `visit_roots` documents the
/// attribute-nested case). Declared components are the signature table in
/// declaration order; suppression directives play no part — the inventory
/// is a report, not a diagnostic ([`crate::validate::validate_with`] owns
/// that choke point).
#[must_use]
pub fn collect_inventory(roots: &[Node], context: &ScanContext) -> FileInventory {
    let mut builder = InventoryBuilder::new(context);
    visit_roots(roots, &mut builder);
    let mut invocations = builder.finish();
    invocations.sort_by_key(|site| site.offset);
    FileInventory {
        declared: context
            .declared_signatures()
            .map(|(name, signature)| DeclaredComponent {
                name: name.to_string(),
                signature: signature.clone(),
            })
            .collect(),
        invocations,
    }
}
