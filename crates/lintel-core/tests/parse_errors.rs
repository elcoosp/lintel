//! Parse-error diagnostics carry stable `LNT01xx` codes and byte offsets.

use lintel_core::{compile, Code, Options};
use lintel_diag::Diagnostic;

fn expect_code(src: &str, code: Code) -> Vec<Diagnostic> {
    match compile(src, &Options::default()) {
        Ok(out) => panic!("expected {code:?}, got output: {}", out.php),
        Err(problems) => {
            assert!(
                problems.iter().any(|p| p.code == code),
                "expected {code:?} in {problems:?}"
            );
            problems
        }
    }
}

#[test]
fn unclosed_tag_reports_unterminated_tag() {
    // EOF inside the attribute list of an unterminated tag.
    expect_code("<?php\nreturn <div\n", Code::UnterminatedTag);
}

#[test]
fn stray_punctuation_in_attrs_reports_bad_attr() {
    expect_code("<?php\nreturn <div;\n", Code::BadAttr);
}

#[test]
fn unclosed_element_reports_unterminated_tag() {
    expect_code("<?php\nreturn <div>\n", Code::UnterminatedTag);
}

#[test]
fn wrong_close_tag_reports_mismatched_close() {
    expect_code("<?php\nreturn <span>text</div>;\n", Code::MismatchedClose);
}

#[test]
fn bare_value_reports_bad_attr() {
    expect_code("<?php\nreturn <div foo=3 />;\n", Code::BadAttr);
}

#[test]
fn malformed_spread_reports_bad_spread() {
    expect_code("<?php\nreturn <div {3} />;\n", Code::BadSpread);
}

#[test]
fn malformed_close_tag_reports_expected() {
    // `</` must be followed by a name and `>`; `<//>` has neither.
    expect_code("<?php\nreturn <div><//>;\n", Code::Expected);
}

#[test]
fn error_offset_points_into_the_source() {
    let src = "<?php\nreturn <div foo=3 />;\n";
    let problems = expect_code(src, Code::BadAttr);
    let problem = &problems[0];
    assert!(
        problem.offset < src.len(),
        "offset {} out of range",
        problem.offset
    );
    assert!(problem.render(src).contains("input:2:"));
}

#[test]
fn clean_sources_report_no_diagnostics() {
    for src in ["<?php\nreturn <div>ok</div>;\n", "<?php echo 1;\n"] {
        match compile(src, &Options::default()) {
            Ok(_) => {}
            Err(problems) => panic!("unexpected diagnostics: {problems:?}"),
        }
    }
}
