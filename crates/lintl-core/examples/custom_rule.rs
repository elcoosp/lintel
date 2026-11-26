//! Consumer example: the validation-rule seam. The compile-time validation
//! pass is a registry of [`ValidationRule`] strategies (the lint-side
//! mirror of the `Lowering` emit seam): `compile_with` runs your rules
//! alongside (or instead of) the built-in lints, and everything they push
//! flows through the same severity split and `lintl-ignore-next` choke
//! point as the built-in diagnostics.
//!
//! This demo registers one policy rule — every `<img>` must carry an `alt`
//! attribute — next to the full built-in registry. Custom rules reuse the
//! closest stable diagnostic class (here `LNL0103`); the message and `help:`
//! lines carry the policy specifics.
//!
//! Run:
//!
//! ```sh
//! cargo run -p lintl-core --example custom_rule
//! ```

use lintl_core::{builtin_rules, compile_with, Code, Diagnostic, Options, ValidationRule};

/// Policy rule: `<img>` requires an `alt` attribute. Reuses the closest
/// stable diagnostic class (`LNL0103` invalid attribute); see the
/// `ValidationRule` trait docs for the closed-code-set decision.
struct RequireAlt;

impl ValidationRule for RequireAlt {
    fn check(
        &self,
        node: &lintl_core::ast::Node,
        _context: &lintl_core::scan::ScanContext,
        out: &mut Vec<Diagnostic>,
    ) {
        let lintl_core::ast::Node::Element(element) = node else {
            return;
        };
        if element.name != "img" {
            return;
        }
        let has_alt = element
            .attrs
            .iter()
            .any(|attr| matches!(attr, lintl_core::ast::Attr::Str { name, .. } if name == "alt"));
        if !has_alt {
            out.push(
                Diagnostic::new(
                    Code::BadAttr,
                    "`<img>` is missing its `alt` attribute",
                    element.offset,
                )
                .with_help("describe the image for screen readers: `<img alt=\"…\">`"),
            );
        }
    }
}

const SOURCE: &str = r#"<?php
return <img src="/logo.png" />;
return <img src="/chart.png" alt="Q3 revenue" />;
// lintl-ignore-next: decorative divider, alt tracked in CMS-42
return <img src="/divider.svg" />;
"#;

fn main() {
    let rules: Vec<&dyn ValidationRule> = builtin_rules()
        .iter()
        .copied()
        .chain([&RequireAlt as &dyn ValidationRule])
        .collect();
    match compile_with(SOURCE, &Options::default(), &rules) {
        Ok(out) => {
            println!("compiled ({} warning(s)):\n", out.warnings.len());
            println!("{SOURCE}");
        }
        Err(problems) => {
            println!("{} violation(s) found:\n", problems.len());
            for problem in &problems {
                println!("{}", problem.render(SOURCE));
            }
        }
    }
}
