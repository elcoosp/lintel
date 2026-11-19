//! Component signature scanning (architecture doc §17.1 Q1): the scanner
//! records declared `function Name(...)` parameter lists in
//! [`lintel_core::scan::ScanContext`], and the validation pass checks
//! component invocations against them. A prop that is not a declared
//! parameter — and children passed to a signature without a `children`
//! parameter — is a guaranteed PHP runtime fatal (`Unknown named parameter`),
//! so both are compile-time errors, exactly like `LNT0202`.

use lintel_core::scan::ScanContext;
use lintel_core::{compile, Code, Options};

fn compile_src(src: &str) -> Result<String, Vec<lintel_core::Diagnostic>> {
    compile(src, &Options::default()).map(|out| out.php)
}

fn expect_one(src: &str, code: Code) {
    match compile_src(src) {
        Ok(php) => panic!("expected {code:?}, got output: {php}"),
        Err(problems) => {
            assert_eq!(problems.len(), 1, "expected exactly one diagnostic");
            assert_eq!(problems[0].code, code);
        }
    }
}

const CARD_DECL: &str = "function Card(string $heading): Html { return <b>{$heading}</b>; }\n";

#[test]
fn unknown_prop_on_a_declared_component_is_rejected() {
    expect_one(
        &format!("<?php\n{CARD_DECL}return <Card title=\"Hi\" />;\n"),
        Code::UnknownProp,
    );
}

#[test]
fn props_matching_declared_parameters_compile() {
    match compile_src(&format!(
        "<?php\n{CARD_DECL}return <Card heading=\"Hi\" />;\n"
    )) {
        Ok(_) => {}
        Err(problems) => panic!("declared props must compile: {problems:?}"),
    }
}

#[test]
fn prop_names_match_case_sensitively() {
    // PHP named arguments are case-sensitive (unlike function calls), so
    // `Heading` does not satisfy the `heading` parameter.
    expect_one(
        &format!("<?php\n{CARD_DECL}return <Card Heading=\"Hi\" />;\n"),
        Code::UnknownProp,
    );
}

#[test]
fn boolean_and_expression_props_are_checked_too() {
    expect_one(
        &format!("<?php\n{CARD_DECL}return <Card heading={{$x}} open />;\n"),
        Code::UnknownProp,
    );
}

#[test]
fn spread_props_bypass_the_signature_check() {
    match compile_src(&format!(
        "<?php\n{CARD_DECL}return <Card {{...$props}} />;\n"
    )) {
        Ok(_) => {}
        Err(problems) => panic!("spreads bypass named-argument rules: {problems:?}"),
    }
}

#[test]
fn variadic_signatures_accept_any_prop() {
    let decl = "function Card(...$rest): Html { return <b>rest</b>; }\n";
    match compile_src(&format!(
        "<?php\n{decl}return <Card anything=\"goes\" />;\n"
    )) {
        Ok(_) => {}
        Err(problems) => panic!("variadic signatures accept arbitrary props: {problems:?}"),
    }
}

#[test]
fn children_require_a_children_parameter() {
    expect_one(
        &format!("<?php\n{CARD_DECL}return <Card heading=\"Hi\">Body</Card>;\n"),
        Code::UnexpectedChildren,
    );
}

#[test]
fn children_parameter_accepts_jsx_children() {
    let decl =
        "function Card(string $heading, iterable $children): Html { return <b>{$heading}</b>; }\n";
    match compile_src(&format!(
        "<?php\n{decl}return <Card heading=\"Hi\">Body</Card>;\n"
    )) {
        Ok(_) => {}
        Err(problems) => panic!("declared children param accepts JSX children: {problems:?}"),
    }
}

#[test]
fn undeclared_components_are_not_signature_checked() {
    // The signature is only knowable for components declared in this file.
    match compile_src("<?php\nreturn <Card title=\"Hi\">Body</Card>;\n") {
        Ok(_) => {}
        Err(problems) => panic!("undeclared components stay unchecked: {problems:?}"),
    }
}

#[test]
fn imported_functions_are_not_signature_checked() {
    // `use function` imports carry no visible parameter list.
    match compile_src("<?php\nuse function App\\Widgets\\Card;\nreturn <Card anything=\"x\" />;\n")
    {
        Ok(_) => {}
        Err(problems) => panic!("imported functions stay unchecked: {problems:?}"),
    }
}

#[test]
fn signature_params_survive_defaults_types_and_brackets() {
    let parsed = lintel_core::scan::scan(
        "<?php\nfunction Card(array $opts = [1, 2], ?Html $children = null, ...$rest): Html {}\n",
    );
    let sig = parsed
        .context
        .signature_of("Card")
        .expect("signature recorded");
    assert_eq!(sig.params, ["opts", "children", "rest"]);
    assert!(sig.variadic);
}

#[test]
fn signature_lookup_is_case_insensitive_like_php_calls() {
    let mut context = ScanContext::default();
    context.declare_signature("App\\Widgets\\Card", crate_sig());
    assert!(context.signature_of("card").is_some());
    assert!(context.signature_of("CARD").is_some());
    assert!(context.signature_of("header").is_none());
}

fn crate_sig() -> lintel_core::scan::Signature {
    lintel_core::scan::Signature {
        params: vec!["heading".to_string()],
        variadic: false,
    }
}

#[test]
fn unknown_prop_help_suggests_the_parameter_list() {
    let src = format!("<?php\n{CARD_DECL}return <Card title=\"Hi\" />;\n");
    match compile_src(&src) {
        Ok(php) => panic!("expected UnknownProp, got: {php}"),
        Err(problems) => {
            let help = problems[0].help.as_deref().unwrap_or_default();
            assert!(
                help.contains("heading"),
                "help names declared params: {help}"
            );
            assert!(help.contains("spread"), "help suggests a spread: {help}");
        }
    }
}
