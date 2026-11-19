//! JSX whitespace normalization (architecture doc 6.4).
//!
//! The rule that everyone gets wrong: lines are trimmed, blank lines are
//! dropped, line breaks between text and an element collapse to a single
//! space, and whitespace on a single line is preserved verbatim. Entities are
//! author-trusted and pass through untouched.

/// JSX text normalization. Returns [`None`] if the text is entirely
/// collapsible whitespace (a newline-separated blank run).
#[must_use]
pub fn normalize_jsx_text(raw: &str) -> Option<String> {
    if !raw.contains('\n') {
        return if raw.is_empty() {
            None
        } else {
            Some(raw.to_string())
        }; // single-line: preserved as-is
    }
    let lines: Vec<&str> = raw.split('\n').collect();
    let last = lines.len() - 1;
    let mut kept: Vec<&str> = vec![];
    for (i, line) in lines.iter().enumerate() {
        let mut t: &str = line.trim_matches('\r');
        if i != 0 {
            t = t.trim_start_matches([' ', '\t']);
        }
        if i != last {
            t = t.trim_end_matches([' ', '\t']);
        }
        if !t.is_empty() {
            kept.push(t);
        }
    }
    if kept.is_empty() {
        None
    } else {
        Some(kept.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_jsx_text as norm;

    #[test]
    fn single_line_is_verbatim() {
        assert_eq!(norm("hello"), Some("hello".into()));
        assert_eq!(norm(""), None);
    }

    #[test]
    fn blank_multiline_is_dropped() {
        assert_eq!(norm("\n  \n\t\n"), None);
    }

    #[test]
    fn breaks_collapse_to_one_space() {
        assert_eq!(norm("a\n  b"), Some("a b".into()));
        assert_eq!(norm("x\n\ny"), Some("x y".into()));
    }
}
