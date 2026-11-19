//! Byte-identical pass-through of non-JSX PHP (architecture doc task T-03).
//!
//! Sacred invariant (AGENTS.md prime directive 2): PHP that contains no JSX
//! must come out of [`lintel_core::compile`] byte-for-byte identical to what
//! went in, across strings, comments, heredocs, attributes, comparisons and
//! inline-HTML interleave.

use lintel_core::{compile, Options};

fn assert_passthrough(src: &str) {
    match compile(src, &Options::default()) {
        Ok(out) => assert_eq!(out.php, src, "output diverged from input"),
        Err(problems) => panic!("unexpected diagnostics: {problems:?}"),
    }
}

#[test]
fn plain_php_with_comparisons_and_generics_like_text() {
    assert_passthrough(
        "<?php\n$a = 1 < 2;\n$b = 3 <=> 1;\n$c = 2 <= 3;\nif ($a && $b < 10) { echo $c << 1; }\n",
    );
}

#[test]
fn strings_with_angle_brackets_and_interpolation() {
    assert_passthrough(
        "<?php\n$s = 'it\\'s <b>bold</b>';\n$d = \"x < y {$arr['k']} >\";\n$t = \"plain {$obj->prop}\";\n",
    );
}

#[test]
fn double_quoted_braces_with_nested_quotes() {
    assert_passthrough("<?php\n$x = \"a {$a['b']} {$b[\"c\"]} {\"no interp\"} end\";\n");
}

#[test]
fn backtick_shell_and_comments() {
    assert_passthrough(
        "<?php\n$out = `ls -la | grep <x>`;\n// one-line comment with <div>\n/* block\n   comment <span> */\n",
    );
}

#[test]
fn attributes_bracket_hash() {
    assert_passthrough("#[Route('/x')]\nfunction f(): int { return 1; }\n");
}

#[test]
fn heredoc_and_nowdoc_with_markup_inside() {
    assert_passthrough(
        "<?php\n$h = <<<EOT\n  <div class=\"x\">{$row['a']}</div>\n  EOT;\n$n = <<<'NOW'\n{$no} <interp> here\nNOW;\n",
    );
}

#[test]
fn inline_html_interleave_and_close_tag() {
    assert_passthrough(
        "<html><body><?php echo 1; ?> <p>hi &amp; bye</p> <?php $a < $b; ?></body></html>\n",
    );
}

#[test]
fn close_tag_terminates_one_line_comment_like_php() {
    // In real PHP, `?>` inside a `//` comment ends PHP mode; the scanner
    // mirrors that, and the whole file still passes through unchanged.
    assert_passthrough("<?php // looks like <div> ?>\n<p>inline</p>\n<?php echo 1;\n");
}

#[test]
fn arrow_functions_pipes_and_pfa_position_independent() {
    assert_passthrough("<?php\n$f = fn(int $x) => $x * 2;\n$r = trim('x ') |> strtoupper(...);\n");
}

#[test]
fn bare_file_without_open_tag_is_left_alone() {
    // v0.1 compiles `.lintel.php` sources, which open with `<?php`. A bare
    // file is treated as a pure HTML template and never touched.
    assert_passthrough("<div class=\"tpl\">{$not_php}</div>\n");
}

#[test]
fn php_open_tag_prefix_without_boundary_is_html() {
    // `<?phpx` is not an open tag: the scanner must not enter PHP mode.
    assert_passthrough("<p><?phpx stays html ?></p>\n");
}

#[test]
fn multibyte_identifiers_and_punctuation_pass_through() {
    assert_passthrough("<?php\n$caf\u{e9} = 'na\u{ef}ve';\necho $caf\u{e9} . '\u{2014}';\n");
}

#[test]
fn whitespace_runs_are_preserved_exactly() {
    assert_passthrough("<?php\n\n\n   \t$x = 1;\t\n\t\treturn 2;\n");
}

#[test]
fn js_like_code_in_script_php_string() {
    assert_passthrough("<?php\n$js = \"if (a < b && c > d) { x(); }\";\n");
}
