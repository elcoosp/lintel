//! `lintel-ignore-next` suppression directives (architecture doc §17): a
//! PHP line comment `// lintel-ignore-next` (or `# lintel-ignore-next`) tells
//! the validation pass to drop diagnostics anchored on the following source
//! line. The file-level sibling `// lintel-ignore-file` drops every
//! validation diagnostic in the file without suppressing parse errors. The
//! exact contracts are recorded in `docs/ignore-directives.md`.
//!
//! The seam mirrors the signature-scanning one: the scanner records
//! suppression windows into [`lintel_core::scan::ScanContext`] while copying
//! line comments verbatim, and the pure validation pass consults the context.
//! Parse diagnostics (`LNT01xx`) are never suppressible — they abort the
//! compile before validation runs, so a suppression can never hide a broken
//! file behind a clean exit.

use lintel_core::scan::ScanContext;
use lintel_core::{compile, Code, Options};

fn compile_src(src: &str) -> Result<String, Vec<lintel_core::Diagnostic>> {
    compile(src, &Options::default()).map(|out| out.php)
}

/// The static `on*` handler warning (`LNT0302`) that a file either carries
/// or does not carry after suppression.
fn warnings_of(src: &str) -> Vec<lintel_core::Diagnostic> {
    compile(src, &Options::default())
        .expect("static handler is a warning, never a failure")
        .warnings
}

#[test]
fn directive_suppresses_the_warning_on_the_next_line() {
    let src =
        "<?php\n// lintel-ignore-next\nreturn <button onclick=\"track('cta')\">Go</button>;\n";
    assert!(warnings_of(src).is_empty(), "warning must be suppressed");
    assert!(compile_src(src).is_ok());
}

#[test]
fn the_same_source_warns_without_the_directive() {
    let src = "<?php\nreturn <button onclick=\"track('cta')\">Go</button>;\n";
    let warnings = warnings_of(src);
    assert_eq!(warnings.len(), 1, "control: warning fires");
    assert_eq!(warnings[0].code, Code::StaticHandler);
}

#[test]
fn directive_suppresses_error_severity_validation_rules() {
    // Builtin shadow (LNT0201) is a hard error without the directive.
    let src = "<?php\n// lintel-ignore-next\nreturn <Header title=\"Hi\" />;\n";
    compile_src(src).expect("LNT0201 suppressed on the next line");
}

#[test]
fn only_the_next_line_is_suppressed() {
    // The directive line, then a clean line, then the offending one.
    let src =
        "<?php\n// lintel-ignore-next\n$a = 1;\nreturn <button onclick=\"x()\">Go</button>;\n";
    let warnings = warnings_of(src);
    assert_eq!(
        warnings.len(),
        1,
        "diagnostic past the next line still fires"
    );
    assert_eq!(warnings[0].code, Code::StaticHandler);
}

#[test]
fn attribute_diagnostics_anchored_on_the_directive_line_are_suppressed() {
    // Hyphenated props (LNT0202) anchor at the attribute name, on the same
    // line as the component tag.
    let src = "<?php\n// lintel-ignore-next\nreturn <Card data-x=\"1\" />;\n";
    compile_src(src).expect("same-line attribute diagnostic suppressed");
}

#[test]
fn a_trailing_reason_is_allowed() {
    let src = "<?php\n// lintel-ignore-next: legacy markup, tracked in CMS-123\nreturn <button onclick=\"track()\">Go</button>;\n";
    assert!(warnings_of(src).is_empty(), "reason text is ignored");
}

#[test]
fn hash_line_comments_work_too() {
    let src = "<?php\n# lintel-ignore-next\nreturn <button onclick=\"track()\">Go</button>;\n";
    assert!(
        warnings_of(src).is_empty(),
        "# is a PHP line comment and carries the directive"
    );
}

#[test]
fn the_directive_must_be_a_line_comment_not_a_string() {
    let src = "<?php\n$note = \"// lintel-ignore-next\";\nreturn <button onclick=\"track()\">Go</button>;\n";
    let warnings = warnings_of(src);
    assert_eq!(
        warnings.len(),
        1,
        "a string containing it must not suppress"
    );
    assert_eq!(warnings[0].code, Code::StaticHandler);
}

#[test]
fn block_comments_do_not_carry_the_directive() {
    let src = "<?php\n/* lintel-ignore-next */\nreturn <button onclick=\"track()\">Go</button>;\n";
    let warnings = warnings_of(src);
    assert_eq!(warnings.len(), 1, "block comments are not directives");
    assert_eq!(warnings[0].code, Code::StaticHandler);
}

#[test]
fn near_miss_directive_names_do_not_suppress() {
    let src = "<?php\n// lintel-ignore-nextline\nreturn <button onclick=\"track()\">Go</button>;\n";
    assert_eq!(warnings_of(src).len(), 1, "directive word must be exact");
}

#[test]
fn parse_errors_are_never_suppressed() {
    // Safety property: LNT01xx diagnostics abort the compile before
    // validation, so a directive cannot hide a misparse.
    let src = "<?php\n// lintel-ignore-next\nreturn <div>unclosed</span>;\n";
    match compile_src(src) {
        Ok(php) => panic!("parse errors are not suppressible: {php}"),
        Err(problems) => assert_eq!(problems[0].code, Code::MismatchedClose),
    }
}

#[test]
fn two_directives_in_a_row_cover_the_first_code_line_after_them() {
    let src = "<?php\n// lintel-ignore-next\n// lintel-ignore-next\nreturn <button onclick=\"track()\">Go</button>;\n";
    assert!(
        warnings_of(src).is_empty(),
        "the window follows the last directive line"
    );
}

#[test]
fn suppression_windows_survive_the_compound_comment_copy() {
    // A directive on the last PHP line with nothing after it suppresses
    // nothing and must not panic.
    let src = "<?php\nreturn <b>ok</b>;\n// lintel-ignore-next";
    compile_src(src).expect("trailing directive without a following line is inert");
}

#[test]
fn suppression_windows_survive_a_close_tag_terminated_comment() {
    // `?>` ends the comment mid-line; the window still lands on the
    // following physical line.
    let src = "<?php\n// lintel-ignore-next ?> \nreturn <button onclick=\"track()\">Go</button>;\n";
    assert!(
        warnings_of(src).is_empty(),
        "?>-terminated comment still works"
    );
}

#[test]
fn scan_context_suppression_lookup_boundaries() {
    let mut context = ScanContext::default();
    context.add_suppression(10, 20);
    assert!(!context.suppressed_at(9), "start is exclusive below");
    assert!(context.suppressed_at(10), "start is inclusive");
    assert!(context.suppressed_at(19), "end is exclusive above");
    assert!(!context.suppressed_at(20), "end is exclusive");
    assert!(!context.suppressed_at(0), "outside any window");
}

#[test]
fn empty_windows_are_not_recorded() {
    let mut context = ScanContext::default();
    context.add_suppression(10, 10);
    assert!(context.suppressions().is_empty(), "empty range is inert");
}

#[test]
fn the_scanner_records_the_window_for_the_following_line() {
    let src = "<?php\n// lintel-ignore-next\nreturn <Header title=\"x\" />;\n";
    let parsed = lintel_core::scan::scan(src);
    let comment_start = src
        .find("// lintel-ignore-next")
        .expect("directive present");
    let directive_line_end = comment_start
        + src[comment_start..]
            .find('\n')
            .expect("comment ends its line");
    let next_line_start = directive_line_end + 1;
    let next_line_end = src[next_line_start..]
        .find('\n')
        .map_or(src.len(), |rel| next_line_start + rel);
    assert!(
        parsed
            .context
            .suppressions()
            .iter()
            .any(|window| window.start == next_line_start && window.end == next_line_end),
        "window covers the line after the directive: {:?}",
        parsed.context.suppressions()
    );
}

/// The file-level directive: `// lintel-ignore-file` (or `# …`, with an
/// optional `: reason`) drops every validation diagnostic in the file —
/// errors and warnings alike — while parse errors stay fatal. Unlike the
/// one-line `ignore-next` window, the file-level flag is position
/// independent and short-circuits the rule walk entirely.

#[test]
fn file_directive_suppresses_every_validation_diagnostic() {
    // One file, two violations of different severities: the LNT0201 shadow
    // lint (error) and the LNT0302 static handler (warning).
    let src = "<?php\n// lintel-ignore-file: generated archive page, audited in CMS-77\n\
               return <Header title=\"Archive\" />;\n\
               return <button onclick=\"track('cta')\">Go</button>;\n";
    let out = compile(src, &Options::default()).expect("file-level suppression compiles");
    assert!(
        out.warnings.is_empty(),
        "warnings are suppressed too: {:?}",
        out.warnings
    );
}

#[test]
fn the_same_file_fails_without_the_directive() {
    let src = "<?php\nreturn <Header title=\"Archive\" />;\n\
               return <button onclick=\"track('cta')\">Go</button>;\n";
    match compile_src(src) {
        Ok(php) => panic!("control: the shadow lint fires: {php}"),
        Err(problems) => assert_eq!(problems[0].code, Code::BuiltinShadow),
    }
}

#[test]
fn file_directive_works_with_hash_comments() {
    let src = "<?php\n# lintel-ignore-file\nreturn <Header title=\"Hi\" />;\n";
    compile_src(src).expect("# is a PHP line comment and carries the file directive");
}

#[test]
fn the_file_directive_word_must_be_exact() {
    let src = "<?php\n// lintel-ignore-filex\nreturn <Header title=\"Hi\" />;\n";
    match compile_src(src) {
        Ok(php) => panic!("near-miss directive names do not suppress: {php}"),
        Err(problems) => assert_eq!(problems[0].code, Code::BuiltinShadow),
    }
}

#[test]
fn block_comments_and_strings_are_not_file_directives() {
    let blocked = "<?php\n/* lintel-ignore-file */\nreturn <Header title=\"Hi\" />;\n";
    match compile_src(blocked) {
        Ok(php) => panic!("block comments are not directives: {php}"),
        Err(problems) => assert_eq!(problems[0].code, Code::BuiltinShadow),
    }
    let stringed = "<?php\n$note = \"// lintel-ignore-file\";\nreturn <Header title=\"Hi\" />;\n";
    match compile_src(stringed) {
        Ok(php) => panic!("a string containing it must not suppress: {php}"),
        Err(problems) => assert_eq!(problems[0].code, Code::BuiltinShadow),
    }
}

#[test]
fn the_file_directive_is_position_independent() {
    // Unlike the one-line `ignore-next` window, the file-level flag covers
    // the whole file no matter where the comment sits — even on the last
    // line, where a window would be inert.
    let src = "<?php\nreturn <Header title=\"Hi\" />;\n// lintel-ignore-file";
    compile_src(src).expect("a trailing file directive still suppresses");
}

#[test]
fn parse_errors_are_never_file_suppressed() {
    // Safety property, shared with `ignore-next`: LNT01xx diagnostics abort
    // the compile before validation, so a file-level flag cannot hide a
    // misparse behind a clean exit.
    let src = "<?php\n// lintel-ignore-file\nreturn <div>unclosed</span>;\n";
    match compile_src(src) {
        Ok(php) => panic!("parse errors are not suppressible: {php}"),
        Err(problems) => assert_eq!(problems[0].code, Code::MismatchedClose),
    }
}

#[test]
fn file_suppression_short_circuits_the_rule_walk() {
    // At the raw seam: a file-suppressed context yields no diagnostics
    // without running any rule — a rule that flags everything fires zero
    // times, because the walk never happens.
    struct FlagEverything;

    impl lintel_core::ValidationRule for FlagEverything {
        fn check(
            &self,
            node: &lintel_core::ast::Node,
            _context: &ScanContext,
            out: &mut Vec<lintel_core::Diagnostic>,
        ) {
            if let lintel_core::ast::Node::Element(element) = node {
                out.push(lintel_core::Diagnostic::new(
                    Code::BadAttr,
                    format!("flagged `<{}>`", element.name),
                    element.offset,
                ));
            }
        }
    }

    let src = "<?php\n// lintel-ignore-file\nreturn <Header title=\"Hi\" />;\n";
    let parsed = lintel_core::scan::scan(src);
    assert!(parsed.context.file_suppressed(), "the scanner set the flag");
    let problems =
        lintel_core::validate::validate_with(&parsed.roots, &parsed.context, &[&FlagEverything]);
    assert!(problems.is_empty(), "the walk never ran: {problems:?}");

    let unsuppressed = lintel_core::scan::scan("<?php\nreturn <Header title=\"Hi\" />;\n");
    assert!(!unsuppressed.context.file_suppressed(), "control: no flag");
    assert_eq!(
        lintel_core::validate::validate_with(
            &unsuppressed.roots,
            &unsuppressed.context,
            &[&FlagEverything]
        )
        .len(),
        1,
        "without the directive the rule fires"
    );
}

#[test]
fn scan_context_file_flag_round_trip() {
    let mut context = ScanContext::default();
    assert!(!context.file_suppressed(), "default is unsuppressed");
    context.suppress_file();
    assert!(context.file_suppressed(), "suppress_file latches the flag");
}
