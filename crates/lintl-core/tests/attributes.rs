//! Attribute forms: static folding, dynamic helpers, spreads, void elements.

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
fn static_attributes_fold_with_quote_escaping() {
    // Static attr values are re-emitted with double quotes (only `"` is
    // re-escaped to `&quot;`, per doc 4.4); the value content is preserved.
    one_liner(
        "<div class=\"hero\" data-x='it&#39;s'>T</div>;\n",
        "new \\Lintl\\Html('<div class=\"hero\" data-x=\"it&#39;s\">T</div>');\n",
    );
}

#[test]
fn bare_boolean_attribute_emits_bare() {
    one_liner(
        "<input checked />;\n",
        "new \\Lintl\\Html('<input checked>');\n",
    );
}

#[test]
fn dynamic_generic_attribute_uses_a_helper() {
    one_liner(
        "<div id={$uid}>T</div>;\n",
        "new \\Lintl\\Html('<div' . \\Lintl\\a('id', $uid) . '>T</div>');\n",
    );
}

#[test]
fn dynamic_url_attribute_uses_the_url_helper() {
    one_liner(
        "<a href={$url}>link</a>;\n",
        "new \\Lintl\\Html('<a' . \\Lintl\\u('href', $url) . '>link</a>');\n",
    );
}

#[test]
fn static_url_attribute_folds_without_helper() {
    one_liner(
        "<a href=\"https://example.com\">link</a>;\n",
        "new \\Lintl\\Html('<a href=\"https://example.com\">link</a>');\n",
    );
}

#[test]
fn class_list_expression_uses_the_cls_helper() {
    one_liner(
        "<div class={['a', $on ? 'on' : 'off']} />;\n",
        "new \\Lintl\\Html('<div' . \\Lintl\\cls(['a', $on ? 'on' : 'off']) . '></div>');\n",
    );
}

#[test]
fn style_map_expression_uses_the_sty_helper() {
    one_liner(
        "<div style={['color' => 'red']} />;\n",
        "new \\Lintl\\Html('<div' . \\Lintl\\sty(['color' => 'red']) . '></div>');\n",
    );
}

#[test]
fn spread_uses_the_attrs_helper() {
    one_liner(
        "<div {...$attrs} />;\n",
        "new \\Lintl\\Html('<div' . \\Lintl\\attrs($attrs) . '></div>');\n",
    );
}

#[test]
fn void_elements_close_without_a_pair() {
    one_liner(
        "<img src={$u} alt=\"\" />;\n",
        "new \\Lintl\\Html('<img' . \\Lintl\\u('src', $u) . ' alt=\"\">');\n",
    );
    one_liner("<br />;\n", "new \\Lintl\\Html('<br>');\n");
}

#[test]
fn self_closing_non_void_emits_an_empty_pair() {
    one_liner("<div />;\n", "new \\Lintl\\Html('<div></div>');\n");
}

#[test]
fn hyphenated_attribute_names_are_kept_for_html_elements() {
    one_liner(
        "<div data-testid={$id} aria-hidden />;\n",
        "new \\Lintl\\Html('<div' . \\Lintl\\a('data-testid', $id) . ' aria-hidden></div>');\n",
    );
}

#[test]
fn alpine_and_htmx_static_attributes_pass_through() {
    one_liner(
        "<button @click=\"show = true\" hx-post=\"/api\">Go</button>;\n",
        "new \\Lintl\\Html('<button @click=\"show = true\" hx-post=\"/api\">Go</button>');\n",
    );
}

#[test]
fn multiline_attribute_tag_preserves_line_count() {
    let src = "<?php\nreturn <div\n    class=\"a\"\n    id={$id}\n>text</div>;\n";
    let out = compiled(src);
    assert_eq!(out.lines().count(), src.lines().count());
    assert!(out.contains(" \\Lintl\\a('id', $id)"));
}
