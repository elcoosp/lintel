//! Stream lowering target (architecture doc 5.3 `stream`, §10): markup
//! lowers to an immediately-invoked `static function (): \Generator`
//! closure whose body yields the markup one chunk at a time, preserving
//! the source line count (doc 13.4).

use lintel_core::{compile, Options, Target};

fn compiled_stream(src: &str) -> String {
    let opts = Options {
        target: Target::Stream,
    };
    match compile(src, &opts) {
        Ok(out) => out.php,
        Err(problems) => panic!("expected clean compile: {problems:?}"),
    }
}

#[test]
fn html_elements_lower_to_a_generator_closure() {
    let php = compiled_stream("<?php\nreturn <b class=\"x\">{$name}</b>;\n");
    assert_eq!(
        php,
        "<?php\nreturn (static function (): \\Generator { yield '<b class=\"x\">'; \
         yield \\Lintel\\c($name); yield '</b>'; })();\n"
    );
}

#[test]
fn multiline_elements_interleave_yields_per_source_line() {
    // Adjacent literal markup merges into one chunk (same convention as the
    // string target's concat), and the newlines the merge moved are
    // compensated after it, keeping the file's line count intact.
    let src = "<?php\nreturn <div>\n    <b>one</b>\n    {$two}\n</div>;\n";
    let php = compiled_stream(src);
    assert_eq!(
        php,
        "<?php\nreturn (static function (): \\Generator { yield '<div><b>one</b>';

 yield \\Lintel\\c($two);
 yield '</div>'; })();\n"
    );
    assert_eq!(php.lines().count(), src.lines().count(), "{php}");
}

#[test]
fn components_lower_to_helper_calls_with_a_children_generator() {
    let php = compiled_stream("<?php\nreturn <Card title=\"Hi\"><p>Body</p></Card>;\n");
    assert_eq!(
        php,
        "<?php\nreturn (static function (): \\Generator { \
         yield \\Lintel\\c(Card(title: 'Hi', children: (static function (): \\Generator { \
         yield '<p>Body</p>'; })())); })();\n"
    );
}

#[test]
fn fragment_roots_yield_their_children_inline() {
    let php = compiled_stream("<?php\nreturn <>a{$x}b</>;\n");
    assert_eq!(
        php,
        "<?php\nreturn (static function (): \\Generator { yield 'a'; \
         yield \\Lintel\\c($x); yield 'b'; })();\n"
    );
}

#[test]
fn dynamic_attributes_route_through_the_context_helpers() {
    let php = compiled_stream("<?php\nreturn <a href={$u} class={$c} id={$i}>x</a>;\n");
    assert_eq!(
        php,
        "<?php\nreturn (static function (): \\Generator { yield '<a'; \
         yield \\Lintel\\u('href', $u); yield \\Lintel\\cls($c); \
         yield \\Lintel\\a('id', $i); yield '>x</a>'; })();\n"
    );
}

#[test]
fn nested_jsx_inside_child_expressions_lowers_recursively() {
    let php = compiled_stream("<?php\nreturn <b>{$ok && <i />}</b>;\n");
    assert!(
        php.contains(
            "\\Lintel\\c($ok && (static function (): \\Generator { yield '<i></i>'; })())"
        ),
        "{php}"
    );
}

#[test]
fn stream_output_never_wraps_the_root_in_an_html_value() {
    let php = compiled_stream("<?php\nreturn <b class=\"x\">{$name}</b>;\n");
    assert!(!php.contains("new \\Lintel\\Html("), "{php}");
    assert!(php.contains(": \\Generator"), "{php}");
}

#[test]
fn stream_target_preserves_line_count() {
    let src = "<?php\nfunction Page(): Html {\n    return <Layout title=\"A\">\n        <Row>one</Row>\n    </Layout>;\n}\n";
    let php = compiled_stream(src);
    assert_eq!(php.lines().count(), src.lines().count());
    // Components yield through `\Lintel\c()`; their children argument is a
    // nested generator closure.
    assert!(
        php.contains(
            "\\Lintel\\c(Row(children: (static function (): \\Generator { yield 'one'; })()))"
        ),
        "{php}"
    );
}

#[test]
fn passthrough_is_byte_identical_under_the_stream_target() {
    let src = "<?php\ndeclare(strict_types=1);\n$x = 1;\necho $x < 2 ? 'a' : 'b';\n";
    let opts = Options {
        target: Target::Stream,
    };
    match compile(src, &opts) {
        Ok(out) => assert_eq!(out.php, src, "non-JSX PHP must pass through byte-identical"),
        Err(problems) => panic!("unexpected diagnostics: {problems:?}"),
    }
}
