//! Consumer example: the node-visitor seam. [`NodeVisitor`] +
//! [`visit_roots`] walk the compiler's AST top-down, depth-first, children
//! in source order — the exact sequence the validation pass walks — as a
//! read-only contract for LSP features, metrics and doc generators that
//! must not re-implement the recursion.
//!
//! This demo implements one visitor that reports the element inventory of a
//! file: every element in walk order, components flagged via
//! `Element::is_component`, and attributes inspected on the element visit
//! (attributes are never separate visits).
//!
//! Run:
//!
//! ```sh
//! cargo run -p lintel-core --example custom_visitor
//! ```

use std::fmt::Write as _;

use lintel_core::ast::{visit_roots, Attr, Node, NodeVisitor};
use lintel_core::scan::scan;

/// Collects one `<Name attr…>` line per visited element, in walk order.
struct ElementInventory {
    report: String,
    elements: usize,
    components: usize,
}

impl NodeVisitor for ElementInventory {
    fn visit(&mut self, node: &Node) {
        let Node::Element(element) = node else {
            return;
        };
        self.elements += 1;
        self.components += usize::from(element.is_component);
        let kind = if element.is_component {
            "component"
        } else {
            "html"
        };
        let attrs = element
            .attrs
            .iter()
            .map(|attr| match attr {
                Attr::Spread { .. } => "{...}".to_string(),
                Attr::Bool { name, .. } => name.clone(),
                Attr::Str { name, .. } => format!("{name}=\"…\""),
                Attr::Expr { name, .. } => format!("{name}={{…}}"),
            })
            .collect::<Vec<_>>()
            .join(" ");
        let _ = writeln!(
            self.report,
            "{kind} <{}{}> — {} child node(s)",
            element.name,
            if attrs.is_empty() {
                String::new()
            } else {
                format!(" {attrs}")
            },
            element.children.len()
        );
    }
}

const SOURCE: &str = r#"<?php
function Card(string $title, bool $active = false): Html {
    return <section class="card"><h3>{$title}</h3><br /></section>;
}
return <PageShell user={$name}>
    <Card title="Today" />
    <Card title="Tomorrow" active />
</PageShell>;
"#;

fn main() {
    let parsed = scan(SOURCE);
    assert!(
        parsed.diagnostics.is_empty(),
        "the demo source parses: {:?}",
        parsed.diagnostics
    );

    let mut inventory = ElementInventory {
        report: String::new(),
        elements: 0,
        components: 0,
    };
    visit_roots(&parsed.roots, &mut inventory);

    println!(
        "walked {} element(s), {} of them component(s):\n",
        inventory.elements, inventory.components
    );
    print!("{}", inventory.report);
    println!(
        "\nwalk order = source order: the function-body `<section>` root is\n\
         visited (with its `<h3>`/`<br>` children) before the `<PageShell>`\n\
         tree, whose `<Card>` children follow their parent."
    );
}
