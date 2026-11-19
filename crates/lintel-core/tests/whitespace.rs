//! JSX whitespace normalization (architecture doc task T-04, section 6.4).
//!
//! Rules mirror the JSX/Babel transform: lines are trimmed, blank lines are
//! dropped, line breaks between text and an element collapse, and whitespace
//! on a single line is preserved verbatim.

use lintel_core::text::normalize_jsx_text;

fn norm(raw: &str) -> Option<String> {
    normalize_jsx_text(raw)
}

#[test]
fn single_line_text_is_preserved_exactly() {
    assert_eq!(norm("hello"), Some("hello".into()));
    assert_eq!(norm("  spaced  "), Some("  spaced  ".into()));
    assert_eq!(norm("a &amp; b"), Some("a &amp; b".into()));
}

#[test]
fn empty_and_whitespace_only_without_newline_is_kept() {
    // No newline: the author's single-line spacing is trusted as-is.
    assert_eq!(norm("   "), Some("   ".into()));
    assert_eq!(norm("\t"), Some("\t".into()));
}

#[test]
fn entirely_blank_multiline_text_is_dropped() {
    assert_eq!(norm(""), None);
    assert_eq!(norm("\n"), None);
    assert_eq!(norm("\n  \n\t\n"), None);
}

#[test]
fn interior_line_breaks_collapse_to_one_space() {
    assert_eq!(norm("a\nb"), Some("a b".into()));
    assert_eq!(norm("a\n  b"), Some("a b".into()));
    assert_eq!(norm("a  \nb"), Some("a b".into()));
    // Trailing whitespace on the final line is not newline-adjacent and is
    // kept (Babel behaves the same way).
    assert_eq!(norm("a\n  b  "), Some("a b  ".into()));
    assert_eq!(norm("a\nb\nc"), Some("a b c".into()));
}

#[test]
fn blank_interior_lines_are_skipped() {
    assert_eq!(norm("x\n\ny"), Some("x y".into()));
    assert_eq!(norm("x\n   \n\ty"), Some("x y".into()));
}

#[test]
fn first_line_keeps_leading_and_last_line_keeps_trailing() {
    // Leading indent of line 0 and trailing indent of the final line belong
    // to the markup outside the JSX tree, so they are preserved.
    assert_eq!(norm("  a\nb"), Some("  a b".into()));
    assert_eq!(norm("a\nb  "), Some("a b  ".into()));
    assert_eq!(norm("  a\nb  "), Some("  a b  ".into()));
}

#[test]
fn carriage_returns_are_trimmed() {
    assert_eq!(norm("a\r\nb"), Some("a b".into()));
    assert_eq!(norm("a\nb\r"), Some("a b".into()));
}

#[test]
fn entities_and_words_survive_joining() {
    assert_eq!(
        norm("Hello\n    &amp; welcome\n    home"),
        Some("Hello &amp; welcome home".into())
    );
}

#[test]
fn text_adjacent_to_elements_collapses() {
    // `<div>\n    text\n  </div>`: children text normalizes to `text`.
    assert_eq!(norm("\n    text\n  "), Some("text".into()));
    assert_eq!(norm("\n"), None);
}

#[test]
fn tabs_count_as_indentation() {
    assert_eq!(norm("a\n\t\tb"), Some("a b".into()));
}
