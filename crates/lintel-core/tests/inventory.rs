//! The component-inventory seam (`docs/component-inventory.md`): a read-only
//! report of every component a file **declares** (with its signature) and
//! every component it **invokes** (with the source position and prop names),
//! so LSP outlines, metrics collectors and doc generators consume the
//! compiler's tree instead of re-walking `Parsed::roots` themselves. The
//! tests pin the contract the way `tests/visitor.rs` pins the traversal and
//! `tests/rules.rs` pins the rule registry.

use lintel_core::collect_inventory;
use lintel_core::scan::scan;
use lintel_core::SourceMap;

/// A file with two declared components (one carrying a default value and a
/// non-`string` type), three invocations — the last from a component the
/// file does not declare — and a spread.
const INVENTORY_SOURCE: &str = "\
<?php
function Card(string $title, int $count = 0): Html {
    return <div class=\"card\">{$title}</div>;
}
function Badge(string $label): Html {
    return <b>{$label}</b>;
}
echo <Card title={$t}>text</Card>;
echo <Badge label=\"hi\" />;
echo <Grid rows={$rows} {...$extra} />;
";

#[test]
fn declared_components_carry_their_signatures_in_declaration_order() {
    let parsed = scan(INVENTORY_SOURCE);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let inventory = collect_inventory(&parsed.roots, &parsed.context);

    let names: Vec<&str> = inventory
        .declared
        .iter()
        .map(|component| component.name.as_str())
        .collect();
    assert_eq!(
        names,
        ["Card", "Badge"],
        "declaration order, original spelling"
    );

    let card = &inventory.declared[0].signature;
    assert_eq!(card.params, ["title", "count"]);
    assert_eq!(card.required, ["title"], "`count` has a default value");
    assert_eq!(
        card.typed_strings,
        ["title"],
        "`int $count` is not a string"
    );
    assert!(!card.variadic);

    let badge = &inventory.declared[1].signature;
    assert_eq!(badge.params, ["label"]);
    assert_eq!(badge.required, ["label"]);
}

#[test]
fn invocations_list_props_children_and_spread_in_source_order() {
    let parsed = scan(INVENTORY_SOURCE);
    let inventory = collect_inventory(&parsed.roots, &parsed.context);

    let names: Vec<&str> = inventory
        .invocations
        .iter()
        .map(|site| site.name.as_str())
        .collect();
    assert_eq!(names, ["Card", "Badge", "Grid"], "source order");

    let card = &inventory.invocations[0];
    assert_eq!(card.props, ["title"]);
    assert!(card.children, "<Card> has JSX children");
    assert!(!card.spread);
    assert!(card.declared_in_file);

    let badge = &inventory.invocations[1];
    assert_eq!(badge.props, ["label"]);
    assert!(!badge.children, "<Badge /> is self-closing");
    assert!(badge.declared_in_file);

    let grid = &inventory.invocations[2];
    assert_eq!(grid.props, ["rows"]);
    assert!(grid.spread, "the spread attribute is flagged");
    assert!(!grid.declared_in_file, "`Grid` is declared in another file");
}

#[test]
fn invocation_offsets_point_at_the_tag_open() {
    let parsed = scan(INVENTORY_SOURCE);
    let inventory = collect_inventory(&parsed.roots, &parsed.context);
    let map = SourceMap::new(INVENTORY_SOURCE);

    for (site, line) in inventory.invocations.iter().zip([8usize, 9, 10]) {
        assert!(
            INVENTORY_SOURCE[site.offset..].starts_with(&format!("<{}", site.name)),
            "`{}`'s offset points at its own `<`: {}",
            site.name,
            site.offset
        );
        let (found_line, col) = map.line_col(site.offset);
        assert_eq!(found_line, line, "one echo per line");
        assert_eq!(col, 6, "each echo starts at column 6");
    }
}

#[test]
fn nested_component_children_appear_in_source_order() {
    let parsed =
        scan("<?php\nreturn <PageShell user={$u}><Card title=\"t\" /><br /></PageShell>;\n");
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let inventory = collect_inventory(&parsed.roots, &parsed.context);
    let names: Vec<&str> = inventory
        .invocations
        .iter()
        .map(|site| site.name.as_str())
        .collect();
    assert_eq!(
        names,
        ["PageShell", "Card"],
        "the walk is top-down, source order"
    );
    assert!(inventory.invocations[1]
        .props
        .contains(&"title".to_string()));
}

#[test]
fn html_elements_fragments_and_text_are_not_invocations() {
    let parsed = scan("<?php\nreturn <><div class=\"x\">text<br /></div></>;\n");
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let inventory = collect_inventory(&parsed.roots, &parsed.context);
    assert_eq!(
        inventory.invocations.len(),
        0,
        "lowercase tags are not components: {:?}",
        inventory.invocations
    );
    assert_eq!(inventory.declared.len(), 0);
}

#[test]
fn a_component_used_twice_appears_once_per_site() {
    let parsed = scan("<?php\necho <Badge label=\"a\" />;\necho <Badge label=\"b\" />;\n");
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let inventory = collect_inventory(&parsed.roots, &parsed.context);
    assert_eq!(
        inventory.invocations.len(),
        2,
        "one entry per invocation site"
    );
    assert!(inventory.invocations[0].offset < inventory.invocations[1].offset);
}

#[test]
fn jsx_roots_nested_in_attribute_expressions_are_inventoried() {
    // `avatar={<Avatar />}` is a separate scan root (docs/node-visitor.md);
    // the inventory covers every root, sorted back into true source order.
    let parsed = scan("<?php\necho <Profile avatar={<Avatar />} />;\n");
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let inventory = collect_inventory(&parsed.roots, &parsed.context);
    let names: Vec<&str> = inventory
        .invocations
        .iter()
        .map(|site| site.name.as_str())
        .collect();
    assert_eq!(names, ["Profile", "Avatar"], "sorted by source offset");
    assert!(inventory.invocations[0].offset < inventory.invocations[1].offset);
}

#[test]
fn suppression_directives_do_not_affect_the_inventory() {
    // The inventory is a report, not a diagnostic: the lintel-ignore-*
    // contracts (docs/ignore-directives.md) silence validation output, and
    // a report of the file's shape must stay complete.
    let parsed = scan(
        "<?php\n// lintel-ignore-file: audited\nfunction Badge(string $label): Html {\n    return <b onclick={$h}>{$label}</b>;\n}\necho <Badge label=\"x\" />;\n",
    );
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let inventory = collect_inventory(&parsed.roots, &parsed.context);
    assert_eq!(inventory.declared.len(), 1);
    assert_eq!(inventory.invocations.len(), 1);
}

#[test]
fn an_empty_file_inventories_nothing() {
    let parsed = scan("");
    let inventory = collect_inventory(&parsed.roots, &parsed.context);
    assert_eq!(inventory.declared.len(), 0);
    assert_eq!(inventory.invocations.len(), 0);
}

#[test]
fn declared_functions_that_are_not_component_shaped_stay_listed() {
    // The scanner's signature table records every declared function; the
    // inventory reports them all (the component-shaped subset is the
    // consumer's filter — helpers like `render_header` are declared too).
    let parsed =
        scan("<?php\nfunction render_header(string $t): Html {\n    return <b>{$t}</b>;\n}\n");
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let inventory = collect_inventory(&parsed.roots, &parsed.context);
    assert_eq!(inventory.declared.len(), 1);
    assert_eq!(inventory.declared[0].name, "render_header");
}

#[test]
fn to_json_emits_one_stable_line_with_the_documented_field_order() {
    let parsed = scan(INVENTORY_SOURCE);
    let inventory = collect_inventory(&parsed.roots, &parsed.context);
    let json = inventory.to_json(INVENTORY_SOURCE);
    assert!(!json.contains('\n'), "one line: {json}");
    assert_eq!(
        json,
        "{\"declared\":[{\"name\":\"Card\",\"params\":[\"title\",\"count\"],\"required\":[\"title\"],\"variadic\":false,\"typed_strings\":[\"title\"]},{\"name\":\"Badge\",\"params\":[\"label\"],\"required\":[\"label\"],\"variadic\":false,\"typed_strings\":[\"label\"]}],\"invocations\":[{\"name\":\"Card\",\"offset\":179,\"line\":8,\"col\":6,\"props\":[\"title\"],\"children\":true,\"spread\":false,\"declared_in_file\":true},{\"name\":\"Badge\",\"offset\":214,\"line\":9,\"col\":6,\"props\":[\"label\"],\"children\":false,\"spread\":false,\"declared_in_file\":true},{\"name\":\"Grid\",\"offset\":241,\"line\":10,\"col\":6,\"props\":[\"rows\"],\"children\":false,\"spread\":true,\"declared_in_file\":false}]}"
    );
}

#[test]
fn to_json_escapes_component_names_and_never_panics() {
    let parsed = scan("<?php\necho <Ui\\Button label={$l} />;\n");
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let inventory = collect_inventory(&parsed.roots, &parsed.context);
    assert_eq!(inventory.invocations.len(), 1);
    assert_eq!(inventory.invocations[0].name, "Ui\\Button");
    let json = inventory.to_json("");
    // The backslash is JSON-escaped; the empty source clamps line/col.
    assert!(
        json.contains("\"name\":\"Ui\\\\Button\""),
        "backslash escaped: {json}"
    );
    assert!(json.contains("\"line\":1,\"col\":1"), "clamped: {json}");
}
