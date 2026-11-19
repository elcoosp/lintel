//! The validation-rule seam: the compile-time validation pass is a registry
//! of [`ValidationRule`] strategies, injected the same way the emit side
//! injects a [`lintel_core::emit::Lowering`] through `scan_with`. Consumers
//! compile with their own rules alongside (or instead of) the built-in set
//! via `compile_with`, and every diagnostic they push flows through the same
//! severity split and `lintel-ignore-next` choke point as the built-in lints.

use lintel_core::validate::{builtin_rules, validate_with};
use lintel_core::{builtin_rules as root_builtin_rules, compile, compile_with, Code, Options};

/// A consumer policy rule: every `<img>` must carry an `alt` attribute.
/// Custom rules reuse the closest stable diagnostic class (`LNT0103` here);
/// see the `ValidationRule` trait docs for the decision.
struct RequireAlt;

impl lintel_core::ValidationRule for RequireAlt {
    fn check(
        &self,
        node: &lintel_core::ast::Node,
        _context: &lintel_core::scan::ScanContext,
        out: &mut Vec<lintel_core::Diagnostic>,
    ) {
        let lintel_core::ast::Node::Element(element) = node else {
            return;
        };
        if element.name != "img" {
            return;
        }
        let has_alt = element
            .attrs
            .iter()
            .any(|attr| matches!(attr, lintel_core::ast::Attr::Str { name, .. } if name == "alt"));
        if !has_alt {
            out.push(
                lintel_core::Diagnostic::new(
                    Code::BadAttr,
                    "`<img>` is missing its `alt` attribute",
                    element.offset,
                )
                .with_help("describe the image for screen readers: `<img alt=\"…\">`"),
            );
        }
    }
}

/// A warning-severity custom rule: advisory only, rides on the output.
struct DiscourageMarquee;

impl lintel_core::ValidationRule for DiscourageMarquee {
    fn check(
        &self,
        node: &lintel_core::ast::Node,
        _context: &lintel_core::scan::ScanContext,
        out: &mut Vec<lintel_core::Diagnostic>,
    ) {
        let lintel_core::ast::Node::Element(element) = node else {
            return;
        };
        if element.name == "marquee" {
            out.push(
                lintel_core::Diagnostic::new(
                    Code::BadAttr,
                    "`<marquee>` is deprecated by every HTML spec",
                    element.offset,
                )
                .with_severity(lintel_core::Severity::Warning),
            );
        }
    }
}

#[test]
fn a_custom_rule_fires_alongside_the_builtins_in_walk_order() {
    // `<Header>` trips the built-in shadow lint (LNT0201) and `<img>` trips
    // the custom rule; the registry runs built-ins first, so the shadow
    // diagnostic precedes the policy diagnostic in DFS source order.
    let src = "<?php\nreturn <Header />;\nreturn <img />;\n";
    let rules: &[&dyn lintel_core::ValidationRule] = &[root_builtin_rules()[0], &RequireAlt];
    let problems = compile_with(src, &Options::default(), rules)
        .expect_err("both the shadow lint and the custom rule fire");
    assert_eq!(problems.len(), 2, "{problems:?}");
    assert_eq!(problems[0].code, Code::BuiltinShadow);
    assert_eq!(problems[1].code, Code::BadAttr);
    assert!(
        problems[1].message.contains("`<img>` is missing"),
        "{problems:?}"
    );
}

#[test]
fn compile_with_empty_rules_skips_builtin_validation_but_keeps_parse_errors() {
    // Rules are validation-only: parse diagnostics (LNT01xx) are produced by
    // the scanner before validation and can never be turned off by rule
    // selection — the same choke-point rule the ignore directives follow.
    let validation_only = "<?php\nreturn <Header />;\n";
    assert!(compile_with(validation_only, &Options::default(), &[]).is_ok());

    let parse_only = "<?php\nreturn <div foo=3 />;\n";
    match compile_with(parse_only, &Options::default(), &[]) {
        Ok(out) => panic!("parse errors survive rule selection: {}", out.php),
        Err(problems) => {
            assert_eq!(problems.len(), 1);
            assert_eq!(problems[0].code, Code::BadAttr);
        }
    }
}

#[test]
fn custom_rule_diagnostics_respect_the_ignore_next_choke_point() {
    let src = "<?php\n// lintel-ignore-next: legacy icon, alt tracked in CMS\nreturn <img />;\n";
    let rules: &[&dyn lintel_core::ValidationRule] = &[&RequireAlt];
    assert!(
        compile_with(src, &Options::default(), rules).is_ok(),
        "a suppression window drops custom-rule diagnostics too"
    );
    let unsuppressed = "<?php\nreturn <img />;\n";
    assert!(compile_with(unsuppressed, &Options::default(), rules).is_err());
}

#[test]
fn custom_rule_warnings_ride_on_the_output() {
    let src = "<?php\nreturn <marquee>slow</marquee>;\n";
    let rules: &[&dyn lintel_core::ValidationRule] = &[&DiscourageMarquee];
    let out = compile_with(src, &Options::default(), rules).expect("warning severity compiles");
    assert_eq!(out.warnings.len(), 1, "{:?}", out.warnings);
    assert_eq!(out.warnings[0].severity, lintel_core::Severity::Warning);
    assert!(out.php.contains("new \\Lintel\\Html("), "{}", out.php);
}

#[test]
fn builtin_rules_cover_the_validation_pass_and_stay_the_default() {
    // Eight built-in rules; the default `compile` path is exactly
    // `compile_with(..., builtin_rules())`, so the seam changes nothing for
    // existing callers.
    assert_eq!(builtin_rules().len(), 8);
    assert_eq!(builtin_rules().len(), root_builtin_rules().len());

    let src = "<?php\nfunction Card(string $title): Html {\n    return <b>{$title}</b>;\n}\n\
               return <Card nope=\"x\" onclick={$h}>y</Card>;\n";
    let via_default = compile(src, &Options::default()).expect_err("builtin diagnostics");
    let via_seam = compile_with(src, &Options::default(), builtin_rules())
        .expect_err("the same diagnostics through the seam");
    assert_eq!(via_default, via_seam);
}

#[test]
fn validate_with_walks_nested_children_in_the_same_order_as_validate() {
    // A rule that flags every element with no attributes: the diagnostics
    // must come out in depth-first source order, one per node, exactly like
    // the built-in pass walks the tree.
    struct FlagBare;

    impl lintel_core::ValidationRule for FlagBare {
        fn check(
            &self,
            node: &lintel_core::ast::Node,
            _context: &lintel_core::scan::ScanContext,
            out: &mut Vec<lintel_core::Diagnostic>,
        ) {
            if let lintel_core::ast::Node::Element(element) = node {
                if element.attrs.is_empty() && !element.is_component {
                    out.push(lintel_core::Diagnostic::new(
                        Code::BadAttr,
                        format!("bare `<{}>`", element.name),
                        element.offset,
                    ));
                }
            }
        }
    }

    let src = "<?php\nreturn <div><p>nested</p><p>also</p></div>;\n";
    let parsed = lintel_core::scan::scan(src);
    let problems = validate_with(&parsed.roots, &parsed.context, &[&FlagBare]);
    assert_eq!(problems.len(), 3, "{problems:?}");
    let offsets: Vec<usize> = problems.iter().map(|p| p.offset).collect();
    assert!(
        offsets.windows(2).all(|pair| pair[0] < pair[1]),
        "{offsets:?}"
    );
}

#[test]
fn validate_with_satisfies_the_same_suppression_contract_as_validate() {
    // Parity check at the raw-seam level: the suppression filter lives in the
    // shared walk, not in any single rule.
    let src = "<?php\n// lintel-ignore-next\nreturn <img />;\n";
    let parsed = lintel_core::scan::scan(src);
    assert_eq!(
        validate_with(&parsed.roots, &parsed.context, &[&RequireAlt]).len(),
        0,
        "a suppression window drops custom-rule diagnostics too"
    );
    let unsuppressed = lintel_core::scan::scan("<?php\nreturn <img />;\n");
    assert_eq!(
        validate_with(&unsuppressed.roots, &unsuppressed.context, &[&RequireAlt]).len(),
        1
    );
}
