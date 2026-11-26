//! Typed-parameter escape elision (architecture doc §7.3 v0.1, the last
//! compiler-side bullet): a dynamic child whose expression is *exactly* a
//! parameter the enclosing function declares as a non-nullable `string`
//! folds to a direct `\htmlspecialchars(...)` call instead of the generic
//! `\Lintl\c()` helper — the type declaration already proves the value is
//! a string, so the wrapper's runtime type check is pure overhead. The
//! scanner records parameter types and tracks the enclosing function scope
//! for each JSX root; everything the scope cannot prove keeps the helper,
//! conservative by design (`docs/escape-elision.md`).

use lintl_core::scan::scan;
use lintl_core::{compile, Options, Target};

fn php_of(src: &str, target: Target) -> String {
    let opts = Options { target };
    match compile(src, &opts) {
        Ok(out) => out.php,
        Err(problems) => panic!("expected clean compile: {problems:?}"),
    }
}

/// The direct-escape emission every elided child lowers to.
const DIRECT: &str = "\\htmlspecialchars($title, \\ENT_QUOTES | \\ENT_SUBSTITUTE | \\ENT_HTML5)";

#[test]
fn a_string_typed_parameter_child_folds_to_direct_htmlspecialchars() {
    let out = php_of(
        "<?php\nfunction Card(string $title): Html {\n    return <b>{$title}</b>;\n}\n",
        Target::String,
    );
    assert!(out.contains(DIRECT), "{out}");
    assert!(!out.contains("\\Lintl\\c($title)"), "{out}");
}

#[test]
fn untyped_mistyped_and_unknown_names_keep_the_helper() {
    // No declaration, an untyped parameter, a non-string type, and a name
    // the scope never declared: none of them prove `string` at compile time.
    for decl in [
        "function Card($title): Html { return <b>{$title}</b>; }",
        "function Card(int $title): Html { return <b>{$title}</b>; }",
        "function Card(bool $title): Html { return <b>{$title}</b>; }",
        "function Card(array $title): Html { return <b>{$title}</b>; }",
    ] {
        let src = format!("<?php\n{decl}\n");
        let out = php_of(&src, Target::String);
        assert!(
            out.contains("\\Lintl\\c($title)"),
            "`{decl}` must keep the helper: {out}"
        );
    }
    let out = php_of("<?php\nreturn <b>{$title}</b>;\n", Target::String);
    assert!(out.contains("\\Lintl\\c($title)"), "{out}");
}

#[test]
fn nullable_union_qualified_and_variadic_types_keep_the_helper() {
    // `?string` can be null, `string|int` is not provably a string, a
    // qualified class type is not `string`, and a variadic `string ...$rest`
    // makes the bare name an array. All keep the wrapper.
    for decl in [
        "function Card(?string $title): Html { return <b>{$title}</b>; }",
        "function Card(string|int $title): Html { return <b>{$title}</b>; }",
        "function Card(\\App\\Name $title): Html { return <b>{$title}</b>; }",
        "function Card(string ...$title): Html { return <b>{$title}</b>; }",
    ] {
        let src = format!("<?php\n{decl}\n");
        let out = php_of(&src, Target::String);
        assert!(
            out.contains("\\Lintl\\c($title)"),
            "`{decl}` must keep the helper: {out}"
        );
    }
}

#[test]
fn promotion_uppercase_spelling_and_defaults_still_elide() {
    // Constructor promotion, case-insensitive type names (PHP keywords are
    // case-insensitive), reference parameters and defaults all still prove
    // `string`.
    for decl in [
        "function Card(private string $title): Html { return <b>{$title}</b>; }",
        "function Card(STRING $title): Html { return <b>{$title}</b>; }",
        "function Card(string &$title): Html { return <b>{$title}</b>; }",
        "function Card(string $title = 'x'): Html { return <b>{$title}</b>; }",
        "function Card(  string   $title  ): Html { return <b>{$title}</b>; }",
    ] {
        let src = format!("<?php\n{decl}\n");
        let out = php_of(&src, Target::String);
        assert!(
            out.contains(DIRECT),
            "`{decl}` must elide to the direct escape: {out}"
        );
    }
}

#[test]
fn only_child_expressions_elide_attributes_keep_their_helpers() {
    // §7.1: attribute helpers carry semantics a scalar/text fold would drop
    // (bool/null handling, URL checks); the typed fold is the *text* row of
    // the matrix, so an attribute value keeps `\Lintl\a()`.
    let out = php_of(
        "<?php\nfunction Card(string $title): Html {\n    return <b title={$title}>{$title}</b>;\n}\n",
        Target::String,
    );
    assert!(out.contains(DIRECT), "{out}");
    assert!(
        out.contains("\\Lintl\\a('title', $title)"),
        "attributes keep the helper: {out}"
    );
}

#[test]
fn rebound_names_keep_the_helper() {
    // Any construct that can rebind the parameter name in the function body
    // removes that name from the scope's proven set — the elision must only
    // fire while the name still denotes the declared string parameter.
    for body_prefix in [
        "$title = 'overwritten';",
        "global $title;",
        "static $title = 0;",
        "unset($title);",
        "foreach ($rows as $title) {}",
        "[$title] = $pair;",
        "list($title) = $pair;",
        "$title[0] = 'x';",
        "$title .= 'suffix';",
    ] {
        // `%CHILD%` sidesteps the format-string braces of the JSX child.
        let src = format!(
            "<?php\nfunction Card(string $title, array $rows, array $pair): Html {{\n    {body_prefix}\n    return <b>%CHILD%</b>;\n}}\n"
        )
        .replace("%CHILD%", "{$title}");
        let out = php_of(&src, Target::String);
        assert!(
            out.contains("\\Lintl\\c($title)"),
            "`{body_prefix}` must keep the helper: {out}"
        );
    }
}

#[test]
fn dynamic_binding_constructs_clear_the_whole_scope() {
    // `extract`, variable variables, `include`/`require` and `eval` can
    // rebind arbitrary names, so they clear the scope's proven set instead
    // of trying to enumerate what moved.
    for body_prefix in [
        "extract($defs);",
        "${$name} = 'x';",
        "include 'partial.php';",
        "require_once 'partial.php';",
        "eval('return 1;');",
    ] {
        let src = format!(
            "<?php\nfunction Card(string $title, array $defs, string $name): Html {{\n    {body_prefix}\n    return <b>%CHILD%</b>;\n}}\n"
        )
        .replace("%CHILD%", "{$title}");
        let out = php_of(&src, Target::String);
        assert!(
            out.contains("\\Lintl\\c($title)"),
            "`{body_prefix}` must clear the scope: {out}"
        );
    }
}

#[test]
fn a_closure_body_shadows_with_its_own_scope() {
    // A braced closure pushes its own scope: its typed parameters elide
    // inside it, its untyped parameters do not, and the enclosing
    // function's elisions are untouched by the closure body.
    let src = "<?php\n\
        function Card(string $title): Html {\n\
            $render = function (string $greet) { return <b>{$greet}</b>; };\n\
            $plain = function ($other) { return <i>{$other}</i>; };\n\
            return <b>{$title}</b>;\n\
        }\n\
    ";
    let out = php_of(src, Target::String);
    assert!(
        out.contains("\\htmlspecialchars($greet,"),
        "the closure's string parameter elides: {out}"
    );
    assert!(
        out.contains("\\Lintl\\c($other)"),
        "the closure's untyped parameter keeps the helper: {out}"
    );
    assert!(
        out.contains(DIRECT),
        "the outer parameter still elides: {out}"
    );
}

#[test]
fn an_arrow_function_clears_the_enclosing_scope() {
    // An arrow function's body is an expression with no braces to mark its
    // scope, and its parameters may shadow the enclosing names — the
    // compiler conservatively clears the enclosing scope's proven set.
    let src = "<?php\n\
        function Card(string $title, string $fallback): Html {\n\
            $render = fn($title) => <b>{$title}</b>;\n\
            return <i>{$fallback}</i>;\n\
        }\n\
    ";
    let out = php_of(src, Target::String);
    assert!(
        !out.contains("\\htmlspecialchars("),
        "no elision under an untracked arrow body: {out}"
    );
    assert!(out.contains("\\Lintl\\c($title)"), "{out}");
    assert!(out.contains("\\Lintl\\c($fallback)"), "{out}");
}

#[test]
fn stream_target_folds_typed_children_into_yield_chunks() {
    let out = php_of(
        "<?php\nfunction Card(string $title): Html {\n    return <b>{$title}</b>;\n}\n",
        Target::Stream,
    );
    assert!(
        out.contains(&format!("yield {};", DIRECT.replace('\\', "\\\\")))
            || out.contains(&format!("yield {DIRECT};")),
        "the stream target yields the direct escape: {out}"
    );
    assert!(!out.contains("\\Lintl\\c($title)"), "{out}");
}

#[test]
fn rfc_target_stays_helper_free() {
    // The RFC model has no `\Lintl\` helpers and no compiler-side text
    // escape; children pass through as raw expressions either way.
    let out = php_of(
        "<?php\nfunction Card(string $title): Html {\n    return <b>{$title}</b>;\n}\n",
        Target::Rfc,
    );
    assert!(!out.contains("\\Lintl\\c("), "{out}");
    assert!(!out.contains("htmlspecialchars"), "{out}");
    assert!(
        out.contains("['<b>', $title, '</b>']") || out.contains("$title"),
        "{out}"
    );
}

#[test]
fn typed_elision_preserves_line_numbers() {
    let src = "<?php\n\
        function Page(string $title): Html {\n\
            return <main>\n\
                <h1>{$title}</h1>\n\
            </main>;\n\
        }\n\
    ";
    let out = php_of(src, Target::String);
    assert_eq!(out.lines().count(), src.lines().count(), "{out}");
    assert!(out.contains(DIRECT), "{out}");
}

#[test]
fn signatures_expose_the_proven_string_parameters() {
    // The scanner's recorded types are public API (`scan::Signature`), so
    // LSP/tooling consumers read the same fact the emitter folds on.
    let parsed = scan(
        "<?php\nfunction Card(string $title, int $count, ?string $note, string ...$rest): Html {}\n",
    );
    let signature = parsed
        .context
        .signature_of("Card")
        .expect("signature recorded");
    assert_eq!(signature.typed_strings, ["title"]);
}

#[test]
fn methods_and_nested_declarations_elide() {
    let src = "<?php\n\
        class Card {\n\
            public function render(string $title): string {\n\
                return <b>{$title}</b>;\n\
            }\n\
        }\n\
        namespace App;\n\
        function widget(string $label): Html { return <i>{$label}</i>; }\n\
    ";
    let out = php_of(src, Target::String);
    assert!(out.contains("\\htmlspecialchars($title,"), "{out}");
    assert!(out.contains("\\htmlspecialchars($label,"), "{out}");
}
