//! Component lowering: named arguments, children, spreads, namespaces.

use lintl_core::{compile, Options};

fn compiled(src: &str) -> String {
    match compile(src, &Options::default()) {
        Ok(out) => out.php,
        Err(problems) => panic!("unexpected diagnostics: {problems:?}"),
    }
}

fn one_liner(body: &str, expected_expr: &str) {
    let src = format!("<?php\nreturn {body}\n");
    let expected = format!("<?php\nreturn {expected_expr}\n");
    assert_eq!(compiled(&src), expected, "for body {body:?}");
}

#[test]
fn attributes_become_named_arguments() {
    one_liner(
        "<Card title=\"Hi\" />;\n",
        "new \\Lintl\\Html(\\Lintl\\c(Card(title: 'Hi')));\n",
    );
}

#[test]
fn dynamic_and_boolean_attributes_become_named_arguments() {
    one_liner(
        "<Card title={$t} active />;\n",
        "new \\Lintl\\Html(\\Lintl\\c(Card(title: $t, active: true)));\n",
    );
}

#[test]
fn children_become_a_single_named_children_argument() {
    one_liner(
        "<Card title=\"Hi\">Body text</Card>;\n",
        "new \\Lintl\\Html(\\Lintl\\c(Card(title: 'Hi', children: new \\Lintl\\Html('Body text'))));\n",
    );
}

#[test]
fn children_argument_is_omitted_when_absent() {
    one_liner("<Card />;\n", "new \\Lintl\\Html(\\Lintl\\c(Card()));\n");
}

#[test]
fn spread_becomes_argument_unpacking() {
    one_liner(
        "<Card {...$props} />;\n",
        "new \\Lintl\\Html(\\Lintl\\c(Card(...($props))));\n",
    );
}

#[test]
fn spread_mixes_with_named_arguments() {
    one_liner(
        "<Card title=\"Hi\" {...$rest} />;\n",
        "new \\Lintl\\Html(\\Lintl\\c(Card(title: 'Hi', ...($rest))));\n",
    );
}

#[test]
fn namespaced_components_resolve_relative() {
    one_liner(
        "<Ui\\Button x={1}>y</Ui\\Button>;\n",
        "new \\Lintl\\Html(\\Lintl\\c(Ui\\Button(x: 1, children: new \\Lintl\\Html('y'))));\n",
    );
}

#[test]
fn html_children_inside_components_lower_normally() {
    one_liner(
        "<Card><b>bold</b> tail</Card>;\n",
        "new \\Lintl\\Html(\\Lintl\\c(Card(children: new \\Lintl\\Html('<b>bold</b> tail'))));\n",
    );
}

#[test]
fn expression_children_with_nested_jsx_lower_recursively() {
    let src = "<?php\nreturn <Card>{$ok && <b>bold</b>}</Card>;\n";
    let out = compiled(src);
    assert!(out.contains(
        "children: new \\Lintl\\Html(\\Lintl\\c($ok && new \\Lintl\\Html('<b>bold</b>')))"
    ));
}

#[test]
fn nested_components_keep_line_count() {
    let src = "\
<?php
function Page(): Html {
    return <Layout title=\"A\">
        <Card>
            <Row>one</Row>
            <Row>two</Row>
        </Card>
    </Layout>;
}
";
    let out = compiled(src);
    assert_eq!(out.lines().count(), src.lines().count());
    assert!(out.contains("\\Lintl\\c(Layout(title: 'A',"));
    assert!(out.contains("\\Lintl\\c(Row("));
}
