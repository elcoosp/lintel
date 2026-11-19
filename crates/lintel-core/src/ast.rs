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

#[cfg(test)]
mod tests {
    use super::is_void_element;

    #[test]
    fn void_list_matches_spec() {
        for tag in ["br", "img", "input", "source"] {
            assert!(is_void_element(tag), "{tag} should be void");
        }
        assert!(!is_void_element("div"));
        assert!(!is_void_element("Div"));
    }
}
