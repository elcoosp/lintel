//! The JSX abstract syntax tree produced by the scanner.
//!
//! Line numbers (`line`, `end_line`) are **relative to the start of the root
//! JSX expression** (0-based) and drive line-preserving emission. The
//! additional `offset`/`end_offset` fields are absolute byte offsets into the
//! scanned source; they exist so the validation pass and the LSP can point
//! diagnostics at exact source locations (an intentional, documented
//! extension of the 6.2 reference skeleton).

/// A node in the JSX tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    /// Whitespace-normalized text, emitted verbatim (author-trusted,
    /// entities pass through).
    Text {
        /// Normalized text content.
        text: String,
        /// Line offset relative to the root JSX expression.
        line: u32,
        /// Absolute byte offset of the raw text.
        offset: usize,
    },
    /// `{ php }` — `php` is already transpiled (nested JSX replaced).
    Expr {
        /// Transpiled PHP expression body.
        php: String,
        /// Line offset relative to the root JSX expression.
        line: u32,
        /// Absolute byte offset of the `{`.
        offset: usize,
    },
    /// `<Name ...>children</Name>` or `<Name ... />`.
    Element(Element),
    /// `<>…</>` — no wrapper element.
    Fragment {
        /// Child nodes.
        children: Vec<Node>,
        /// Line offset relative to the root JSX expression.
        line: u32,
        /// Absolute byte offset of the `<`.
        offset: usize,
    },
}

/// A JSX element or component invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    /// Tag name as written, e.g. `"div"`, `"Card"`, `"Ui\\Button"`.
    pub name: String,
    /// Capitalized first letter or containing `\` means component.
    pub is_component: bool,
    /// Attributes in source order.
    pub attrs: Vec<Attr>,
    /// Children in source order.
    pub children: Vec<Node>,
    /// Written as a self-closing `<div />`.
    pub self_closing: bool,
    /// Line offset of the tag start, relative to the root expression.
    pub line: u32,
    /// Line offset of the closing tag end, relative to the root expression.
    pub end_line: u32,
    /// Absolute byte offset of the opening `<`.
    pub offset: usize,
    /// Absolute byte offset just past the closing `>`.
    pub end_offset: usize,
}

/// One attribute (or spread) of an element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Attr {
    /// `name="str"` / `name='str'` — folded at compile time.
    Str {
        /// Attribute name.
        name: String,
        /// Raw string value.
        value: String,
        /// Line offset relative to the root expression.
        line: u32,
        /// Absolute byte offset of the attribute name.
        offset: usize,
    },
    /// `name` (bare) — boolean `true`.
    Bool {
        /// Attribute name.
        name: String,
        /// Line offset relative to the root expression.
        line: u32,
        /// Absolute byte offset of the attribute name.
        offset: usize,
    },
    /// `name={expr}` — dynamic value.
    Expr {
        /// Attribute name.
        name: String,
        /// Transpiled PHP expression body.
        php: String,
        /// Line offset relative to the root expression.
        line: u32,
        /// Absolute byte offset of the attribute name.
        offset: usize,
    },
    /// `{...$attrs}` — spread of an array of attributes.
    Spread {
        /// Transpiled PHP expression (without the leading `...`).
        php: String,
        /// Line offset relative to the root expression.
        line: u32,
        /// Absolute byte offset of the `{`.
        offset: usize,
    },
}

/// HTML void elements (architecture doc 4.2): emitted without a close tag.
pub(crate) const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr",
];

/// Whether `name` (case-sensitively, as written) is an HTML void element.
#[must_use]
pub fn is_void_element(name: &str) -> bool {
    VOID_ELEMENTS.contains(&name)
}

/// A read-only consumer of the JSX tree (doc §21.1 seam): the traversal the
/// validation pass uses, extracted the way [`crate::SourceMap`] extracted
/// the renderer's position math, so an LSP, metrics collector or doc
/// generator walks the compiler's tree instead of re-implementing the
/// recursion over [`Node`].
///
/// Contract, mirroring the validation walk ([`crate::validate::validate_with`]):
///
/// - **Top-down, depth-first, source order.** [`visit_node`] calls
///   [`NodeVisitor::visit`] on the node first, then recurses into the
///   children in source order — the same sequence a
///   [`crate::ValidationRule`] sees, minus the rule registry.
/// - **Exactly once per node.** Over one `scan()` result the root trees are
///   disjoint: an element's children hold their own subtrees, while JSX
///   embedded inside a PHP expression is a separate root — so [`visit_roots`]
///   visits every node exactly once.
/// - **Attributes are not separate visits.** Inspect [`Element::attrs`] when
///   the owning element arrives, the way a rule reads its element.
/// - **Read-only.** The visitor cannot mutate the tree; accumulate state in
///   the implementor through `&mut self`.
///
/// The seam is infallible like the walk it mirrors: traversal cannot fail,
/// so there is no error channel — a visitor that needs to report pushes
/// diagnostics through [`crate::validate::ValidationRule`] instead.
pub trait NodeVisitor {
    /// Inspect one node, called top-down before its children are visited.
    fn visit(&mut self, node: &Node);
}

/// Walk one node and its subtree with a [`NodeVisitor`]: the node first,
/// then each child in source order, depth-first ([`NodeVisitor`] contract).
pub fn visit_node(node: &Node, visitor: &mut dyn NodeVisitor) {
    visitor.visit(node);
    match node {
        Node::Element(element) => {
            for child in &element.children {
                visit_node(child, visitor);
            }
        }
        Node::Fragment { children, .. } => {
            for child in children {
                visit_node(child, visitor);
            }
        }
        Node::Text { .. } | Node::Expr { .. } => {}
    }
}

/// Walk every root in source order (the same node sequence
/// [`crate::validate::validate_with`]'s walk covers over
/// `scan().roots`): each root is one [`NodeVisitor`] walk
/// ([`NodeVisitor`] contract).
pub fn visit_roots(roots: &[Node], visitor: &mut dyn NodeVisitor) {
    for root in roots {
        visit_node(root, visitor);
    }
}

#[cfg(test)]
mod tests {
    use super::{is_void_element, visit_node, Node, NodeVisitor};

    #[test]
    fn void_list_matches_spec() {
        for tag in ["br", "img", "input", "source"] {
            assert!(is_void_element(tag), "{tag} should be void");
        }
        assert!(!is_void_element("div"));
        assert!(!is_void_element("Div"));
    }

    /// A leaf is one visit and a fragment recurses: the smallest unit checks
    /// of the [`NodeVisitor`] contract (the integration side lives in
    /// `tests/visitor.rs`).
    #[derive(Default)]
    struct Counter {
        text: usize,
        fragments: usize,
    }

    impl NodeVisitor for Counter {
        fn visit(&mut self, node: &Node) {
            match node {
                Node::Text { .. } => self.text += 1,
                Node::Fragment { .. } => self.fragments += 1,
                Node::Element(_) | Node::Expr { .. } => {}
            }
        }
    }

    #[test]
    fn a_text_leaf_is_visited_exactly_once() {
        let leaf = Node::Text {
            text: "hi".to_string(),
            line: 0,
            offset: 0,
        };
        let mut counter = Counter::default();
        visit_node(&leaf, &mut counter);
        assert_eq!(counter.text, 1);
        assert_eq!(counter.fragments, 0);
    }

    #[test]
    fn a_fragment_recurses_into_its_children() {
        let fragment = Node::Fragment {
            children: vec![
                Node::Text {
                    text: "a".to_string(),
                    line: 0,
                    offset: 0,
                },
                Node::Text {
                    text: "b".to_string(),
                    line: 0,
                    offset: 1,
                },
            ],
            line: 0,
            offset: 0,
        };
        let mut counter = Counter::default();
        visit_node(&fragment, &mut counter);
        assert_eq!(counter.fragments, 1);
        assert_eq!(counter.text, 2);
    }
}
