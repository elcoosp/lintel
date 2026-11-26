//! Raw-text elements: `<script>` and `<style>` bodies stay verbatim (4.7.1).

use lintl_core::{compile, Options};

fn compiled(src: &str) -> String {
    match compile(src, &Options::default()) {
        Ok(out) => out.php,
        Err(problems) => panic!("unexpected diagnostics: {problems:?}"),
    }
}

#[test]
fn script_body_is_verbatim_with_braces_and_comparisons() {
    let src = "<?php\nreturn <script>if (a < b) { go({x: 1}); }</script>;\n";
    let out = compiled(src);
    assert_eq!(
        out,
        "<?php\nreturn new \\Lintl\\Html('<script>if (a < b) { go({x: 1}); }</script>');\n"
    );
}

#[test]
fn style_body_is_verbatim() {
    let src = "<?php\nreturn <style>.a { color: red; }</style>;\n";
    let out = compiled(src);
    assert_eq!(
        out,
        "<?php\nreturn new \\Lintl\\Html('<style>.a { color: red; }</style>');\n"
    );
}

#[test]
fn script_interpolation_is_not_compiled() {
    // §4.7.1: `{}` inside raw-text elements is data, not a JSX expression.
    let src = "<?php\nreturn <script type=\"application/json\">{$data}</script>;\n";
    let out = compiled(src);
    assert_eq!(
        out,
        "<?php\nreturn new \\Lintl\\Html('<script type=\"application/json\">{$data}</script>');\n"
    );
}

#[test]
fn multiline_script_keeps_every_line() {
    let src = "\
<?php
return <script>
var a = 1;
if (a < 2) { go(); }
</script>;
";
    let out = compiled(src);
    assert_eq!(out.lines().count(), src.lines().count());
    assert!(out.contains("var a = 1;\nif (a < 2) { go(); }"));
}

#[test]
fn empty_script_element() {
    let src = "<?php\nreturn <script></script>;\n";
    assert_eq!(
        compiled(src),
        "<?php\nreturn new \\Lintl\\Html('<script></script>');\n"
    );
}

#[test]
fn close_tag_prefix_does_not_swallow_longer_names() {
    // `</scripted>` must not close a `<script>` element.
    let src = "<?php\nreturn <script>a</scripted>b;\n";
    let problems = compile(src, &Options::default()).expect_err("should not close");
    assert_eq!(problems.len(), 1);
}
