//! The `textDocument/formatting` lane (§17.1 Q3+/Year-2 "formatter"):
//! normalizes the JSX indentation of a `.lintel.php` document without
//! touching the PHP.
//!
//! The contract is deliberately narrow, because JSX whitespace is
//! semantic (§6.4's T-04 normalization): only lines that BEGIN a tag,
//! END a tag, or continue a tag's attribute region are re-indented.
//! Text lines keep their whitespace exactly — their leading spaces can
//! reach the rendered output — and lines whose first character opens a
//! PHP region are the PHP code the formatter must not touch. Raw-text
//! bodies (`<script>`, `<style>`) are data and are never re-indented.
//!
//! Depth is the count of enclosing open tags at the line's first
//! non-whitespace byte (a line that starts a close tag drops one, so it
//! aligns with the opening line — the usual editor convention). Each
//! level is 4 spaces, prefixed by the indentation of the nearest
//! preceding PHP-context line, so a component's JSX keeps its
//! PHP-context indentation and only the nesting normalizes.
//!
//! Lines whose computed indentation already matches emit no edit — a
//! well-formatted document formats to zero edits.

use lsp_types::{Range, TextEdit};

/// One indent level.
const INDENT: &str = "    ";

/// The re-indentation edits for `text`, in document order.
#[must_use]
pub fn formatting_edits(text: &str) -> Vec<TextEdit> {
    let spans = crate::tag_spans(text);
    let mut edits = Vec::new();
    let mut base = String::new(); // the nearest preceding PHP-context indent
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let line_start = offset;
        offset += line.len();
        let content = line.trim_end_matches(['\n', '\r']);
        let Some(first) = content.find(|c: char| !c.is_whitespace()) else {
            continue; // a blank line keeps the base, emits nothing
        };
        let first_offset = line_start + first;

        // The line's relationship to the tag regions: inside a tag token
        // (an attribute-region continuation), starting a tag, or starting
        // a close tag.
        let in_tag_token = spans
            .iter()
            .any(|span| span.open <= first_offset && first_offset < span.close);
        let starts_tag = spans.iter().any(|span| span.open == first_offset);
        let starts_close = spans
            .iter()
            .any(|span| span.closing && span.open == first_offset);

        if !in_tag_token && !starts_tag && !starts_close {
            // A text or PHP line: re-indent nothing, and refresh the base
            // only when the line is PHP context (outside every JSX body —
            // a body line has enclosing tags and carries JSX semantics).
            if depth_at(first_offset, &spans) == 0 {
                base = content[..first].to_string();
            }
            continue;
        }
        let mut depth = depth_at(first_offset, &spans);
        if starts_close || content[first..].starts_with("</") {
            // A close-tag line aligns with its element's opening line.
            depth = depth.saturating_sub(1);
        }
        let new_indent = format!("{base}{}", INDENT.repeat(depth as usize));
        if content[..first] != new_indent {
            edits.push(TextEdit {
                range: Range {
                    start: crate::position_of_offset(text, line_start),
                    end: crate::position_of_offset(text, first_offset),
                },
                new_text: new_indent,
            });
        }
    }
    edits
}

/// The count of enclosing, not-yet-closed elements before `offset`:
/// open tags count up (self-closing tags do not nest), close tags count
/// down. Spans arrive in offset order, so the walk stops at `offset`.
fn depth_at(offset: usize, spans: &[crate::TagSpan<'_>]) -> u32 {
    let mut depth = 0_u32;
    for span in spans {
        if span.open >= offset {
            break;
        }
        match span.closing {
            true => depth = depth.saturating_sub(1),
            false if !span.self_closing => depth += 1,
            false => {}
        }
    }
    depth
}
