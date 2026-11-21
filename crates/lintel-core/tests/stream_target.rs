//! Stream lowering target (architecture doc 5.3 `stream`, §10): markup
//! lowers to an immediately-invoked `static function (): \Generator`
//! closure whose body yields the markup one chunk at a time, preserving
//! the source line count (doc 13.4). The closure carries a by-value
//! `use (...)` list for every enclosing local its body references —
//! without it, any dynamic streaming component reads undefined variables
//! at runtime (§21.2 drift fix; `docs/stream-alignment.md`).

use std::path::PathBuf;
use std::process::Command;

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
        "<?php\nreturn (static function () use ($name): \\Generator { yield '<b class=\"x\">'; \
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
        "<?php\nreturn (static function () use ($two): \\Generator { yield '<div><b>one</b>';

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
        "<?php\nreturn (static function () use ($x): \\Generator { yield 'a'; \
         yield \\Lintel\\c($x); yield 'b'; })();\n"
    );
}

#[test]
fn dynamic_attributes_route_through_the_context_helpers() {
    let php = compiled_stream("<?php\nreturn <a href={$u} class={$c} id={$i}>x</a>;\n");
    assert_eq!(
        php,
        "<?php\nreturn (static function () use ($c, $i, $u): \\Generator { yield '<a'; \
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
fn stream_target_rewrites_the_declared_html_return_type() {
    // The Phase 9 parity bug: a stream artifact kept the author's
    // `: Html` while its body returned the generator closure — the first
    // real PHP consumer (`ComponentTester::renderStreaming()`) hit a
    // `TypeError` under `strict_types` the moment it invoked the
    // component. The declaration rewrites to `: \Generator` on the SAME
    // line (line preservation holds), nullable and qualified spellings
    // included; unions and body-less declarations stay verbatim.
    let php = compiled_stream(
        "<?php\nfunction Badge(string $label): Html\n{\n    return <b>{$label}</b>;\n}\n",
    );
    assert!(
        php.contains("function Badge(string $label): \\Generator\n{"),
        "the declared Html return type rewrites: {php}"
    );
    assert!(!php.contains("): Html"), "{php}");

    let mixed = compiled_stream(concat!(
        "<?php\n",
        "function Nullable(?string $x): ?Html\n{\n    return <i>{$x}</i>;\n}\n",
        "function Qualified(): \\Lintel\\Html\n{\n    return <u />;\n}\n",
    ));
    assert!(
        mixed.contains("function Nullable(?string $x): \\Generator\n{"),
        "nullable Html rewrites to the non-null Generator (the generator is never null): {mixed}"
    );
    assert!(
        mixed.contains("function Qualified(): \\Generator\n{"),
        "{mixed}"
    );

    let untouched = compiled_stream(concat!(
        "<?php\n",
        "function Union(): Html|string\n{\n    return <b>x</b>;\n}\n",
        "function Helper(): int\n{\n    return 1;\n}\n",
    ));
    assert!(
        untouched.contains("): Html|string"),
        "a union copies verbatim (the span stops at `|`): {untouched}"
    );
    assert!(
        untouched.contains("): int"),
        "a non-Html type copies verbatim: {untouched}"
    );

    // The string target keeps every declaration verbatim.
    let string_php = {
        let opts = Options {
            target: Target::String,
        };
        match compile(
            "<?php\nfunction Badge(string $label): Html\n{\n    return <b>{$label}</b>;\n}\n",
            &opts,
        ) {
            Ok(out) => out.php,
            Err(problems) => panic!("expected clean compile: {problems:?}"),
        }
    };
    assert!(
        string_php.contains("): Html"),
        "the string target returns Html values exactly as declared: {string_php}"
    );
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

/// Resolve the PHP binary: `PHP_BIN` overrides the default `php` on `PATH`.
fn php_bin() -> Option<PathBuf> {
    let name = std::env::var("PHP_BIN").unwrap_or_else(|_| "php".into());
    if name.contains('/') {
        let path = PathBuf::from(name);
        return path.is_file().then_some(path);
    }
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|dir| dir.join(&name))
        .find(|candidate| candidate.is_file())
}

#[test]
fn stream_closures_capture_the_referenced_enclosing_locals() {
    let php = compiled_stream("<?php\nreturn <b class=\"x\">{$name}</b>;\n");
    assert!(
        php.starts_with("<?php\nreturn (static function () use ($name): \\Generator {"),
        "the closure must capture, by value, every enclosing local its body references: {php}"
    );
}

#[test]
fn captures_are_sorted_deduped_and_skip_superglobals_and_this() {
    let php = compiled_stream(
        "<?php\nreturn <a href={$u} class={$c} id={$i} data-s={$_SERVER['X']} data-t={$this->q}>x</a>;\n",
    );
    assert!(
        php.contains("use ($c, $i, $u): \\Generator"),
        "captures are sorted and deduplicated: {php}"
    );
    assert!(
        !php.contains("use ($_SERVER"),
        "superglobals resolve in any scope: {php}"
    );
    assert!(
        !php.contains("use ($this"),
        "static closures cannot bind $this: {php}"
    );
}

#[test]
fn component_children_closures_capture_referenced_locals_transitively() {
    // The children closure captures `$row` for its own body, and the outer
    // closure must capture it too — the inner closure is created inside the
    // outer one's scope, so the name has to ride in through the outer's
    // `use` list for the inner's `use` evaluation to see it.
    let php = compiled_stream("<?php\nreturn <Card>{$row}</Card>;\n");
    assert_eq!(
        php.matches("use ($row)").count(),
        2,
        "both the children closure and the enclosing closure capture the name: {php}"
    );
}

#[test]
fn nested_jsx_captures_carry_through_the_enclosing_expression() {
    let php = compiled_stream("<?php\nreturn <b>{$ok && <i />}</b>;\n");
    assert!(
        php.contains(
            "\\Lintel\\c($ok && (static function (): \\Generator { yield '<i></i>'; })())"
        ),
        "the inner no-capture closure keeps its shape: {php}"
    );
    assert!(
        php.contains("use ($ok): \\Generator"),
        "the variable lives in the expression, so the root closure captures it: {php}"
    );
}

/// The F1 regression (§21.2): a streaming component whose body references
/// its own locals must **execute** — the emitted closure's `use` list makes
/// the typed parameter and a plain local visible. Pre-fix, the closure read
/// undefined variables: PHP warned and rendered empties.
#[test]
fn streamed_component_with_locals_executes_and_renders_them() {
    let Some(bin) = php_bin() else {
        eprintln!("skip stream_target/php execution: no PHP binary (set PHP_BIN)");
        return;
    };
    let src = "<?php\nfunction Card(string $title): \\Generator\n{\n    $note = 'n1';\n    return <div class=\"card\" data-note={$note}>{$title}</div>;\n}\n";
    let php = compiled_stream(src);
    assert!(
        php.contains("use ($note, $title): \\Generator"),
        "both the typed parameter and the plain local are captured: {php}"
    );

    let dir = std::env::temp_dir().join(format!("lintel-stream-exec-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp exec dir");
    let compiled = dir.join("card.php");
    std::fs::write(&compiled, &php).expect("write compiled component");
    let lint = Command::new(&bin)
        .arg("-l")
        .arg(&compiled)
        .output()
        .expect("spawn php -l");
    assert!(
        lint.status.success(),
        "php -l failed:\n{}{}",
        String::from_utf8_lossy(&lint.stdout),
        String::from_utf8_lossy(&lint.stderr),
    );

    let runtime =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../php/runtime/src/functions.php");
    let runner = dir.join("runner.php");
    std::fs::write(
        &runner,
        format!(
            "<?php\nrequire '{}';\nrequire '{}';\nforeach (Card(title: 'Hi & bye') as $chunk) {{ echo $chunk; }}\n",
            runtime.to_string_lossy().replace('\'', "\\'"),
            compiled.to_string_lossy().replace('\'', "\\'"),
        ),
    )
    .expect("write runner");
    let run = Command::new(&bin).arg(&runner).output().expect("spawn php");
    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        run.status.success() && stderr.is_empty(),
        "the streamed component must execute without warnings:\n--- stderr ---\n{stderr}\n--- stdout ---\n{stdout}"
    );
    assert_eq!(
        stdout, "<div class=\"card\" data-note=\"n1\">Hi &amp; bye</div>",
        "the streamed render must carry the captured locals through"
    );
}
