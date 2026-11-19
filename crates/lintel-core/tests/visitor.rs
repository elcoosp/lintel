//! The `NodeVisitor` traversal seam (`docs/node-visitor.md`): a read-only
//! consumer walks the exact node sequence the validation pass walks —
//! top-down, depth-first, children in source order, every node exactly once
//! over one scan. The tests pin the traversal contract the way
//! `tests/rules.rs` pins the rule seam.

use lintel_core::ast::{visit_node, visit_roots, Node, NodeVisitor};
use lintel_core::scan::scan;

/// Records one compact description per visited node, in walk order.
#[derive(Default)]
struct Recorder {
    trace: Vec<String>,
}

impl NodeVisitor for Recorder {
    fn visit(&mut self, node: &Node) {
        let description = match node {
            Node::Element(element) => format!("<{}>", element.name),
            Node::Fragment { .. } => "<>".to_string(),
            Node::Text { text, .. } => format!("text({text})"),
            Node::Expr { php, .. } => format!("expr({php})"),
        };
        self.trace.push(description);
    }
}

fn trace_of_roots(roots: &[Node]) -> Vec<String> {
    let mut recorder = Recorder::default();
    visit_roots(roots, &mut recorder);
    recorder.trace
}

#[test]
fn visit_node_walks_top_down_depth_first_in_source_order() {
    let parsed =
        scan("<?php\nreturn <main><Card id={$id}>hello <b>bold</b></Card><br /></main>;\n");
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let mut recorder = Recorder::default();
    visit_node(&parsed.roots[0], &mut recorder);
    assert_eq!(
        recorder.trace,
        [
            "<main>",
            "<Card>",
            "text(hello )",
            "<b>",
            "text(bold)",
            "<br>"
        ],
        "attributes are inspected on the element visit, never as separate visits"
    );
}

#[test]
fn leaves_are_single_visits_and_fragments_recurse() {
    let parsed = scan("<?php\nreturn <>{$title}<p>body</p></>;\n");
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(
        trace_of_roots(&parsed.roots),
        ["<>", "expr($title)", "<p>", "text(body)"]
    );
}

#[test]
fn visit_roots_visits_each_root_in_source_order() {
    let parsed = scan("<?php\n$a = <b>one</b>;\n$c = <i>two</i>;\n");
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(parsed.roots.len(), 2, "two independent JSX roots");
    assert_eq!(
        trace_of_roots(&parsed.roots),
        ["<b>", "text(one)", "<i>", "text(two)"]
    );
}

#[test]
fn every_node_is_visited_exactly_once_across_expression_roots() {
    // The `<img>` inside the attribute expression is a root of its own (the
    // expression path), not a structural child of `<Card>` — the root trees
    // are disjoint, so the walk never double-visits.
    let parsed = scan("<?php\nreturn <Card avatar={<img src={$u} />} id=\"7\" />;\n");
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(parsed.roots.len(), 2, "the expression root plus the card");
    assert_eq!(trace_of_roots(&parsed.roots), ["<img>", "<Card>"]);
}

#[test]
fn a_mutable_visitor_accumulates_state_across_the_walk() {
    #[derive(Default)]
    struct Counts {
        elements: usize,
        components: usize,
        text: usize,
        expr: usize,
    }

    impl NodeVisitor for Counts {
        fn visit(&mut self, node: &Node) {
            match node {
                Node::Element(element) => {
                    self.elements += 1;
                    self.components += usize::from(element.is_component);
                }
                Node::Fragment { .. } => {}
                Node::Text { .. } => self.text += 1,
                Node::Expr { .. } => self.expr += 1,
            }
        }
    }

    let parsed = scan("<?php\nreturn <PageShell><Row key={$k} /><b>x</b>{n}</PageShell>;\n");
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let mut counts = Counts::default();
    visit_roots(&parsed.roots, &mut counts);
    assert_eq!(counts.elements, 3, "PageShell, Row and b");
    assert_eq!(counts.components, 2, "PageShell and Row are components");
    assert_eq!(counts.text, 1);
    assert_eq!(counts.expr, 1);
}
