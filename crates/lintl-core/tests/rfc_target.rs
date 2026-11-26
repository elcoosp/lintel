//! RFC-parity lowering (architecture doc 5.3, `--target=rfc`): markup lowers
//! to `new \Markup\Element(...)` constructor calls / named-argument component
//! calls, with no `\Lintl\` runtime helpers in the output.

use lintl_core::{compile, Options, Target};

fn compile_with(src: &str, target: Target) -> String {
    let opts = Options { target };
    match compile(src, &opts) {
        Ok(out) => out.php,
        Err(problems) => panic!("expected clean compile ({target:?}): {problems:?}"),
    }
}

#[test]
fn html_elements_lower_to_the_markup_element_constructor() {
    let php = compile_with("<?php\nreturn <b class=\"x\">hi</b>;\n", Target::Rfc);
    assert_eq!(
        php,
        "<?php\nreturn new \\Markup\\Element('b', ['class' => 'x'], ['hi']);\n"
    );
}

#[test]
fn components_lower_to_named_argument_calls() {
    let php = compile_with("<?php\nreturn <Card title=\"Hi\" />;\n", Target::Rfc);
    assert_eq!(php, "<?php\nreturn Card(title: 'Hi');\n");
}

#[test]
fn component_children_become_a_children_array_argument() {
    let php = compile_with(
        "<?php\nreturn <Card title=\"Hi\"><p>Body</p></Card>;\n",
        Target::Rfc,
    );
    assert_eq!(
        php,
        "<?php\nreturn Card(title: 'Hi', children: [new \\Markup\\Element('p', [], ['Body'])]);\n"
    );
}

#[test]
fn dynamic_and_boolean_and_spread_attributes_pass_through() {
    let php = compile_with(
        "<?php\nreturn <div data-id={$id} disabled {...$props} />;\n",
        Target::Rfc,
    );
    assert_eq!(
        php,
        "<?php\nreturn new \\Markup\\Element('div', ['data-id' => $id, 'disabled' => true, ...$props], []);\n"
    );
}

#[test]
fn fragment_roots_lower_to_a_plain_php_array() {
    let php = compile_with("<?php\nreturn <>a{$x}b</>;\n", Target::Rfc);
    assert_eq!(php, "<?php\nreturn ['a', $x, 'b'];\n");
}

#[test]
fn nested_jsx_inside_child_expressions_lowers_recursively() {
    let php = compile_with("<?php\nreturn <b>{$ok && <i />}</b>;\n", Target::Rfc);
    assert_eq!(
        php,
        "<?php\nreturn new \\Markup\\Element('b', [], [$ok && new \\Markup\\Element('i', [], [])]);\n"
    );
}

#[test]
fn rfc_output_never_references_lintl_runtime_helpers() {
    let src = "<?php\nreturn <a class={$cls} href={$url}>{'<b>evil</b>'}{$body}</a>;\n";
    let php = compile_with(src, Target::Rfc);
    assert!(
        !php.contains("\\Lintl\\"),
        "rfc target must not emit Lintl runtime helpers: {php}"
    );
}

#[test]
fn rfc_target_preserves_line_count() {
    let src = "<?php\nfunction Badge(string $label): Html {\n    return (\n        <span class=\"badge\">\n            {$label}\n            <i class=\"icon-check\" />\n        </span>\n    );\n}\n";
    let php = compile_with(src, Target::Rfc);
    assert_eq!(
        php.lines().count(),
        src.lines().count(),
        "lines in must equal lines out:\n{php}"
    );
}

#[test]
fn passthrough_is_byte_identical_across_targets() {
    let src = "<?php\n// renders <b>{nested}</b> markup\n$a = 1;\nif ($a < 10 && $a > 0) { $sum += $a <=> 2; }\n$s = \"<tr class=\\\"r\\\">{{$data['c']}}</tr>\";\n$t = <<<EOT\n  <li>{{$a}}</li>\n  EOT;\n";
    let string_php = compile_with(src, Target::String);
    let rfc_php = compile_with(src, Target::Rfc);
    assert_eq!(string_php, src, "string target passes through verbatim");
    assert_eq!(rfc_php, src, "rfc target passes through verbatim");
}

#[test]
fn string_target_output_is_unchanged_by_the_new_seam() {
    let php = compile_with("<?php\nreturn <b>bold</b>;\n", Target::String);
    assert_eq!(php, "<?php\nreturn new \\Lintl\\Html('<b>bold</b>');\n");
}

#[test]
fn void_and_self_closing_elements_emit_empty_children() {
    let php = compile_with("<?php\nreturn [<br />, <div />];\n", Target::Rfc);
    assert_eq!(
        php,
        "<?php\nreturn [new \\Markup\\Element('br', [], []), new \\Markup\\Element('div', [], [])];\n"
    );
}
