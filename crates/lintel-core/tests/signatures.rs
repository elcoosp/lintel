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
    // `$heading` is declared without a default, so the unknown `title` prop
    // (LNT0203) and the missing required `heading` parameter (LNT0205) both
    // fire for the same invocation.
    match compile_src(&format!(
        "<?php\n{CARD_DECL}return <Card title=\"Hi\" />;\n"
    )) {
        Ok(php) => panic!("expected diagnostics, got output: {php}"),
        Err(problems) => {
            assert_eq!(problems.len(), 2, "unknown prop + missing required");
            assert_eq!(problems[0].code, Code::UnknownProp);
            assert_eq!(problems[1].code, Code::MissingRequiredProp);
        }
    }
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
    // `Heading` does not satisfy the `heading` parameter: the unknown-prop
    // report (LNT0203) and the missing required parameter (LNT0205) both fire.
    match compile_src(&format!(
        "<?php\n{CARD_DECL}return <Card Heading=\"Hi\" />;\n"
    )) {
        Ok(php) => panic!("expected diagnostics, got output: {php}"),
        Err(problems) => {
            assert_eq!(problems.len(), 2, "unknown prop + missing required");
            assert_eq!(problems[0].code, Code::UnknownProp);
            assert_eq!(problems[1].code, Code::MissingRequiredProp);
        }
    }
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
        required: vec![],
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

// ----- required-parameter checks (LNT0205, architecture doc §17.1 Q1) -----

/// A signature with a required `$label`, a required `$name`, and an optional
/// `$required` (defaulted).
const FIELD_DECL: &str = "function Field(string $label, string $name, bool $required = false): Html { return <label>{$label}</label>; }\n";

#[test]
fn missing_required_prop_is_rejected() {
    expect_one(
        &format!("<?php\n{FIELD_DECL}return <Field name=\"email\" />;\n"),
        Code::MissingRequiredProp,
    );
}

#[test]
fn supplied_required_props_compile() {
    match compile_src(&format!(
        "<?php\n{FIELD_DECL}return <Field label=\"Mail\" name=\"email\" />;\n"
    )) {
        Ok(_) => {}
        Err(problems) => panic!("supplied required props must compile: {problems:?}"),
    }
}

#[test]
fn defaulted_parameters_stay_optional() {
    match compile_src(&format!(
        "<?php\n{FIELD_DECL}return <Field label=\"Mail\" name=\"email\" required />;\n"
    )) {
        Ok(_) => {}
        Err(problems) => panic!("a defaulted parameter is optional: {problems:?}"),
    }
}

#[test]
fn a_spread_counts_as_supplying_required_params() {
    match compile_src(&format!(
        "<?php\n{FIELD_DECL}return <Field {{...['label' => 'M', 'name' => 'e']}} />;\n"
    )) {
        Ok(_) => {}
        Err(problems) => panic!("a spread may supply required props: {problems:?}"),
    }
}

#[test]
fn jsx_children_satisfy_a_required_children_parameter() {
    let decl = "function Slot(array $children): Html { return <b>slot</b>; }\n";
    match compile_src(&format!("<?php\n{decl}return <Slot>Body</Slot>;\n")) {
        Ok(_) => {}
        Err(problems) => panic!("JSX children satisfy the children parameter: {problems:?}"),
    }
}

#[test]
fn missing_required_children_parameter_is_rejected() {
    let decl = "function Slot(array $children): Html { return <b>slot</b>; }\n";
    expect_one(
        &format!("<?php\n{decl}return <Slot />;\n"),
        Code::MissingRequiredProp,
    );
}

#[test]
fn missing_required_prop_help_names_the_parameter_and_default_advice() {
    let src = format!("<?php\n{FIELD_DECL}return <Field name=\"email\" />;\n");
    match compile_src(&src) {
        Ok(php) => panic!("expected MissingRequiredProp, got: {php}"),
        Err(problems) => {
            let help = problems[0].help.as_deref().unwrap_or_default();
            assert!(help.contains("label:"), "help names the prop: {help}");
            assert!(help.contains("default"), "help suggests a default: {help}");
        }
    }
}

#[test]
fn variadic_signatures_skip_the_required_check() {
    let decl = "function Row(string $must, ...$rest): Html { return <b>rest</b>; }\n";
    match compile_src(&format!("<?php\n{decl}return <Row anything=\"x\" />;\n")) {
        Ok(_) => {}
        Err(problems) => panic!("variadic signatures accept arbitrary props: {problems:?}"),
    }
}

#[test]
fn required_params_are_recorded_from_defaults() {
    let parsed = lintel_core::scan::scan(
        "<?php\nfunction Card(array $opts = [1, 2], string $title, ?Html $children = null, ...$rest): Html {}\n",
    );
    let sig = parsed
        .context
        .signature_of("Card")
        .expect("signature recorded");
    assert_eq!(sig.params, ["opts", "title", "children", "rest"]);
    assert_eq!(sig.required, ["title"], "only the non-defaulted param");
    assert!(sig.variadic);
}

#[test]
fn quoted_default_values_still_mark_the_param_optional() {
    let parsed = lintel_core::scan::scan(
        "<?php\nfunction Card(string $a = 'x=y', string $b = \"z\", int $c = 1 + 2): Html {}\n",
    );
    let sig = parsed
        .context
        .signature_of("Card")
        .expect("signature recorded");
    assert!(
        sig.required.is_empty(),
        "every segment carries a top-level `=`: {:?}",
        sig.required
    );
}

#[test]
fn missing_required_prop_code_is_stable() {
    assert_eq!(Code::MissingRequiredProp.lnt(), "LNT0205");
}
