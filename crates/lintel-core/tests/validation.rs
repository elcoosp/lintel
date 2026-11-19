//! Compile-time validation pass (architecture doc task T-08): stable `LNT02xx`
//! / `LNT03xx` / `LNT04xx` diagnostics wired into [`lintel_core::compile`].

use lintel_core::scan::ScanContext;
use lintel_core::{compile, Code, Options};

fn expect_one(src: &str, code: Code) {
    match compile(src, &Options::default()) {
        Ok(out) => panic!("expected {code:?}, got output: {}", out.php),
        Err(problems) => {
            assert_eq!(problems.len(), 1, "expected exactly one diagnostic");
            assert_eq!(problems[0].code, code);
            assert!(
                problems[0].render(src).contains(code.docs_url().as_str()),
                "rendered diagnostic lacks the docs link"
            );
        }
    }
}

#[test]
fn component_shadowing_a_builtin_is_rejected() {
    // `Header` resolves to PHP's case-insensitive builtin `header()`.
    expect_one("<?php\nreturn <Header />;\n", Code::BuiltinShadow);
}

#[test]
fn declaring_the_component_suppresses_builtin_shadow() {
    let src = "<?php\nfunction Header(string $title): Html { return <h1>{$title}</h1>; }\nreturn <Header title=\"x\" />;\n";
    match compile(src, &Options::default()) {
        Ok(_) => {}
        Err(problems) => panic!("declared component must compile: {problems:?}"),
    }
}

#[test]
fn importing_the_component_via_use_function_suppresses_shadow() {
    let src = "<?php\nuse function App\\Widgets\\Header;\nreturn <Header title=\"x\" />;\n";
    match compile(src, &Options::default()) {
        Ok(_) => {}
        Err(problems) => panic!("imported component must compile: {problems:?}"),
    }
}

#[test]
fn builtin_check_ignores_lowercase_html_tags() {
    // `<count>` is lowercase → HTML element, not a component call.
    match compile(
        "<?php\nreturn <count class=\"n\" />;\n",
        &Options::default(),
    ) {
        Ok(_) => {}
        Err(problems) => panic!("lowercase tags are not component calls: {problems:?}"),
    }
}

#[test]
fn hyphenated_component_attribute_suggests_spread() {
    let problems = match compile(
        "<?php\nreturn <Card data-x=\"1\" />;\n",
        &Options::default(),
    ) {
        Ok(out) => panic!("expected HyphenArg, got: {}", out.php),
        Err(problems) => problems,
    };
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].code, Code::HyphenArg);
    assert!(problems[0]
        .help
        .as_deref()
        .unwrap_or_default()
        .contains("spread"));
}

#[test]
fn hyphenated_html_attribute_is_fine() {
    match compile("<?php\nreturn <div data-x=\"1\" />;\n", &Options::default()) {
        Ok(_) => {}
        Err(problems) => panic!("html elements keep hyphenated attrs: {problems:?}"),
    }
}

#[test]
fn dynamic_event_handler_attribute_is_rejected() {
    expect_one(
        "<?php\nreturn <button onclick={$handler}>Go</button>;\n",
        Code::DynamicHandler,
    );
}

#[test]
fn event_handler_check_is_case_insensitive_and_covers_onmouse() {
    expect_one(
        "<?php\nreturn <a HREF=\"/x\" onMouseOver={$f}>y</a>;\n",
        Code::DynamicHandler,
    );
}

#[test]
fn static_event_handler_string_is_allowed_but_documented() {
    // Static strings are author-trusted (doc 4.4); only dynamic values error.
    match compile(
        "<?php\nreturn <button onclick=\"alert(1)\">Go</button>;\n",
        &Options::default(),
    ) {
        Ok(_) => {}
        Err(problems) => panic!("static handler strings pass: {problems:?}"),
    }
}

#[test]
fn dynamic_handler_inside_nested_expression_is_caught() {
    // The nested JSX root inside `{...}` must be validated too.
    expect_one(
        "<?php\nreturn ($ok && <button onclick={$h} />);\n",
        Code::DynamicHandler,
    );
}

#[test]
fn void_element_with_children_is_rejected() {
    expect_one("<?php\nreturn <br>text</br>;\n", Code::VoidChildren);
}

#[test]
fn void_element_with_expression_children_is_rejected() {
    expect_one(
        "<?php\nreturn <img src={$u}>{$alt}</img>;\n",
        Code::VoidChildren,
    );
}

#[test]
fn multiple_violations_are_all_reported() {
    let src = "<?php\nreturn <Header data-x=\"1\" />;\n";
    match compile(src, &Options::default()) {
        Ok(out) => panic!("expected diagnostics, got: {}", out.php),
        Err(problems) => {
            let codes: Vec<_> = problems.iter().map(|p| p.code).collect();
            assert!(codes.contains(&Code::BuiltinShadow), "{codes:?}");
            assert!(codes.contains(&Code::HyphenArg), "{codes:?}");
        }
    }
}

#[test]
fn scan_context_reports_declared_functions_case_insensitively() {
    let mut context = ScanContext::default();
    context.declare("App\\Widgets\\Card");
    assert!(context.declares_function("card"));
    assert!(context.declares_function("CARD"));
    assert!(!context.declares_function("header"));
}
