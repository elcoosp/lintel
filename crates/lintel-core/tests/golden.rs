//! Golden compile tests: JSX in, plain PHP out (architecture doc 6.9).
//!
//! Each golden asserts the full emitted PHP plus the line-preservation
//! invariant (every following line keeps its number). These goldens are the
//! executable fix for the doc's aspirational §6.9 sample, which does not
//! follow its own `line_of` semantics; see `docs/architecture.md` status note.

use lintel_core::{compile, Options};

const BADGE_SRC: &str = "\
<?php
function Badge(string $label, bool $verified = false): Html {
    return (
        <span class=\"badge\">
            {$label}
            {$verified && <i class=\"icon-check\" />}
        </span>
    );
}
";

const BADGE_EXPECTED: &str = "\
<?php
function Badge(string $label, bool $verified = false): Html {
    return (
        new \\Lintel\\Html('<span class=\"badge\">'
 . \\Lintel\\c($label)
 . \\Lintel\\c($verified && new \\Lintel\\Html('<i class=\"icon-check\"></i>'))
 . '</span>')
    );
}
";

const PAGE_SRC: &str = "\
<?php
function Page(string $name): Html {
    return <Card title={$name} flagged>
        <p>Hello</p>
    </Card>;
}
";

const PAGE_EXPECTED: &str = "\
<?php
function Page(string $name): Html {
    return new \\Lintel\\Html(\\Lintel\\c(Card(title: $name, flagged: true, children: new \\Lintel\\Html(
'<p>Hello</p>'
))));
}
";

fn compiled(src: &str) -> String {
    match compile(src, &Options::default()) {
        Ok(out) => out.php,
        Err(problems) => panic!("unexpected diagnostics: {problems:?}"),
    }
}

#[test]
fn badge_golden_is_line_preserving() {
    let out = compiled(BADGE_SRC);
    assert_eq!(out, BADGE_EXPECTED);
    assert_eq!(out.lines().count(), BADGE_SRC.lines().count());
}

#[test]
fn nested_jsx_inside_child_expression_is_transpiled_recursively() {
    let out = compiled(BADGE_SRC);
    assert!(out
        .contains("\\Lintel\\c($verified && new \\Lintel\\Html('<i class=\"icon-check\"></i>'))"));
}

#[test]
fn component_children_become_the_named_children_argument() {
    let out = compiled(PAGE_SRC);
    assert_eq!(out, PAGE_EXPECTED);
    assert_eq!(out.lines().count(), PAGE_SRC.lines().count());
    assert!(out.contains("children: new \\Lintel\\Html("));
}

#[test]
fn fragment_has_no_wrapper_and_folds_escaped_literals() {
    let src = "<?php\nreturn <>{'a & b'}<br/><p>x</p></>;\n";
    let out = compiled(src);
    assert_eq!(
        out,
        "<?php\nreturn new \\Lintel\\Html('a &amp; b<br><p>x</p>');\n"
    );
}

#[test]
fn integer_literal_children_fold_to_literals() {
    let src = "<?php\nreturn <b>{42}</b>;\n";
    let out = compiled(src);
    assert_eq!(out, "<?php\nreturn new \\Lintel\\Html('<b>42</b>');\n");
}

#[test]
fn comparison_operators_are_not_tags() {
    // `$a < $b` after an operand is the less-than operator, not JSX.
    let src = "<?php\n$smaller = $a < $b;\n";
    assert_eq!(compiled(src), src);
}

#[test]
fn multiline_component_page_preserves_line_count() {
    let src = "\
<?php
function Page(string $user): Html {
    return <>
        <h1>Hi</h1>
        {match ($kind) {
            'a' => <b>one</b>,
            default => <i>two</i>,
        }}
        <img src={$avatar} alt=\"\" />
    </>;
}
";
    let out = compiled(src);
    assert_eq!(out.lines().count(), src.lines().count());
    assert!(out.contains("\\Lintel\\c(match ($kind)"));
}
