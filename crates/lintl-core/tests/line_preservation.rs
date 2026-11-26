//! Line-preservation invariant (architecture doc 13.4 / task T-06):
//! the emitted file has exactly as many lines as the source file, so every
//! PHP token after a JSX root keeps its line number (Xdebug/PHPStan-safe).

use lintl_core::{compile, Options};

const CASES: &[&str] = &[
    // Spec 6.9 seed.
    "<?php\nfunction Badge(string $label, bool $verified = false): Html {\n    return (\n        <span class=\"badge\">\n            {$label}\n            {$verified && <i class=\"icon-check\" />}\n        </span>\n    );\n}\n",
    // Component with children and named args.
    "<?php\nfunction Page(string $name): Html {\n    return <Card title={$name} flagged>\n        <p>Hello</p>\n    </Card>;\n}\n",
    // Multi-line attribute list.
    "<?php\nreturn <div\n    class=\"a\"\n    id={$id}\n>text</div>;\n",
    // Nested components.
    "<?php\nreturn <Layout title=\"A\">\n    <Card>\n        <Row>one</Row>\n        <Row>two</Row>\n    </Card>\n</Layout>;\n",
    // Raw-text element with newlines.
    "<?php\nreturn <script>\nvar a = 1;\nif (a < 2) { go(); }\n</script>;\n",
    // Fragment with dynamic children.
    "<?php\nreturn <>\n    <h1>Hi</h1>\n    {($a && <b>one</b>) ?? null}\n</>;\n",
    // Spread + void + bare attrs on one line.
    "<?php\nreturn <ul {...$list}><li data-x /><br /></ul>;\n",
    // JSX inside a PHP block body.
    "<?php\nif ($ok) {\n    echo <b>yes</b>;\n}\n",
];

#[test]
fn every_golden_source_keeps_its_line_count() {
    for src in CASES {
        match compile(src, &Options::default()) {
            Ok(out) => {
                assert_eq!(
                    out.php.lines().count(),
                    src.lines().count(),
                    "line drift for source:\n{src}"
                );
            }
            Err(problems) => panic!("unexpected diagnostics for {src:?}: {problems:?}"),
        }
    }
}

#[test]
fn jsx_inside_if_block_body_preserves_lines() {
    let src = "<?php\nif ($ok) {\n    echo <b>yes</b>;\n}\n";
    let out = match compile(src, &Options::default()) {
        Ok(out) => out.php,
        Err(problems) => panic!("unexpected diagnostics: {problems:?}"),
    };
    assert_eq!(
        out,
        "<?php\nif ($ok) {\n    echo new \\Lintl\\Html('<b>yes</b>');\n}\n"
    );
}
