//! Warning-level diagnostics (doc 4.4/§21.1): a [`Severity`] on every
//! diagnostic, the static `on*` handler lint (`LNL0302`) emitted as a
//! warning, and warnings returned on [`lintl_core::Output`] without failing
//! the compilation.

use lintl_core::{compile, Code, Options, Severity};

#[test]
fn static_event_handler_string_warns_but_compiles() {
    let src = "<?php\nreturn <button onclick=\"track('cta')\">Go</button>;\n";
    let out = compile(src, &Options::default()).expect("static handler is a warning, not an error");
    assert_eq!(
        out.warnings.len(),
        1,
        "exactly one warning: {:?}",
        out.warnings
    );
    let warning = &out.warnings[0];
    assert_eq!(warning.code, Code::StaticHandler);
    assert_eq!(warning.severity, Severity::Warning);
    assert!(
        warning.render(src).contains("warning[LNL0302]"),
        "{warning:?}"
    );
    assert!(warning
        .help
        .as_deref()
        .unwrap_or_default()
        .contains("data-"));
}

#[test]
fn dynamic_event_handler_still_fails_with_error_severity() {
    let src = "<?php\nreturn <button onclick={$handler}>Go</button>;\n";
    match compile(src, &Options::default()) {
        Ok(out) => panic!("dynamic handlers must fail: {}", out.php),
        Err(problems) => {
            assert_eq!(problems.len(), 1);
            assert_eq!(problems[0].code, Code::DynamicHandler);
            assert_eq!(problems[0].severity, Severity::Error);
        }
    }
}

#[test]
fn warning_diagnostics_come_from_every_target() {
    let src = "<?php\nreturn <button onclick=\"track()\">Go</button>;\n";
    for target in [
        lintl_core::Target::String,
        lintl_core::Target::Stream,
        lintl_core::Target::Rfc,
    ] {
        let opts = Options { target };
        let out = compile(src, &opts).expect("warns, does not fail");
        assert_eq!(out.warnings.len(), 1, "target {target:?}");
        assert_eq!(out.warnings[0].code, Code::StaticHandler);
    }
}

#[test]
fn clean_output_carries_no_warnings() {
    let out = compile("<?php\nreturn <b>bold</b>;\n", &Options::default()).expect("compiles");
    assert_eq!(out.warnings, Vec::new());
}

#[test]
fn mixed_error_and_warning_reports_only_the_error() {
    let src = "<?php\nreturn <Header onclick=\"track()\" />;\n";
    match compile(src, &Options::default()) {
        Ok(out) => panic!("builtin shadow is an error: {}", out.php),
        Err(problems) => {
            assert_eq!(problems.len(), 1);
            assert_eq!(problems[0].code, Code::BuiltinShadow);
            assert_eq!(problems[0].severity, Severity::Error);
        }
    }
}

#[test]
fn static_handler_check_is_case_insensitive() {
    let src = "<?php\nreturn <a OnClick=\"track()\">y</a>;\n";
    let out = compile(src, &Options::default()).expect("compiles");
    assert_eq!(out.warnings.len(), 1);
    assert_eq!(out.warnings[0].code, Code::StaticHandler);
}

#[test]
fn severity_defaults_to_error_on_plain_diagnostics() {
    let problem = lintl_core::Diagnostic::new(Code::BadAttr, "bad", 0);
    assert_eq!(problem.severity, Severity::Error);
    assert!(
        problem.render("x").contains("error[LNL0103]"),
        "{problem:?}"
    );
}

#[test]
fn diagnostics_can_be_marked_as_warnings_explicitly() {
    let problem = lintl_core::Diagnostic::new(Code::StaticHandler, "static handler", 0)
        .with_help("wire it up in JS")
        .with_severity(Severity::Warning);
    assert_eq!(problem.severity, Severity::Warning);
    assert!(
        problem.render("x").contains("warning[LNL0302]"),
        "{problem:?}"
    );
}

#[test]
fn display_prefixes_warnings() {
    let problem = lintl_core::Diagnostic::new(Code::StaticHandler, "static handler", 4)
        .with_severity(Severity::Warning);
    assert_eq!(
        problem.to_string(),
        "warning[LNL0302]: static handler at offset 4"
    );
}
