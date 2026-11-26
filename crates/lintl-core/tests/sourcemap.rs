//! The `SourceMap` seam: every [`Diagnostic`] carries a byte `offset`, and
//! the renderer computes `input:line:col` from it — but that mapping was
//! private, so CLI/LSP/daemon consumers had to re-implement it. The public
//! [`lintl_diag::SourceMap`] (re-exported at the `lintl-core` root) is the
//! shared offset→position converter, with the same clamp-and-floor
//! semantics the renderer has always used.

use lintl_core::{compile, Code, Options, SourceMap};

const SRC: &str = "<?php\nfunction Card(): Html {\n    return <div foo=3 />;\n}\n";

#[test]
fn line_col_matches_the_render_header_for_a_real_diagnostic() {
    let problem = compile(SRC, &Options::default()).expect_err("foo=3 is not a value form");
    let diagnostic = &problem[0];
    assert_eq!(diagnostic.code, Code::BadAttr);
    let rendered = diagnostic.render(SRC);
    let header = rendered
        .lines()
        .find(|line| line.contains("┌─ input:"))
        .expect("render prints an input header");
    // The renderer and the public seam must agree on line and column.
    let map = SourceMap::new(SRC);
    let (line, col) = map.line_col(diagnostic.offset);
    assert!(
        header.contains(&format!("input:{line}:{col}")),
        "header {header:?} vs sourcemap {line}:{col}"
    );
}

#[test]
fn columns_count_characters_not_bytes() {
    // `é` is two bytes; the renderer's columns have always been char-based,
    // so the seam must be too: the tag after `café` lands at column 12.
    let src = "<?php\n// café\nreturn <img />;\n";
    let map = SourceMap::new(src);
    let (line, col) = map.line_col(src.find("<img").unwrap());
    assert_eq!((line, col), (3, 8));
    assert_eq!(
        src[..src.find("<img").unwrap()].chars().count(),
        src.find("<img").unwrap() - 1
    );
}

#[test]
fn crlf_source_keeps_one_line_per_line_ending() {
    let src = "<?php\r\nreturn <div foo=3 />;\r\n";
    let problem = compile(src, &Options::default()).expect_err("parse error expected");
    let map = SourceMap::new(src);
    let (line, col) = map.line_col(problem[0].offset);
    assert_eq!(line, 2, "{problem:?}");
    assert!(col >= 8);
    // The line text is the physical line: a CRLF source keeps its `\r` in
    // the slice, exactly like the renderer has always printed it.
    assert_eq!(
        map.line_text(problem[0].offset).trim_end_matches('\r'),
        "return <div foo=3 />;"
    );
}

#[test]
fn offsets_past_the_end_clamp_to_the_last_position() {
    let src = "<?php\nreturn <b>x</b>;\n";
    let map = SourceMap::new(src);
    // The source ends with a newline, so the clamped last position is the
    // start of the (empty) third line.
    assert_eq!(map.line_col(10_000), (3, 1));
    assert_eq!(map.line_text(10_000), "");
    // A mid-character offset floors to the char boundary instead of
    // panicking: `é` starts at byte 3 in "café" (5 bytes total) — offset 4
    // is inside it and floors back to 3.
    let accents = "café";
    let mid = SourceMap::new(accents);
    assert_eq!(mid.line_col(9), (1, 5));
    assert_eq!(mid.clamp(9), 5);
    assert_eq!(mid.clamp(4), 3);
    assert_eq!(mid.clamp(1), 1);
}

#[test]
fn empty_and_boundary_sources_stay_total() {
    let empty = SourceMap::new("");
    assert_eq!(empty.line_col(0), (1, 1));
    assert_eq!(empty.line_text(0), "");
    assert_eq!(empty.clamp(42), 0);
    let map = SourceMap::new(SRC);
    assert_eq!(map.source(), SRC);
    assert_eq!(map.line_col(0), (1, 1));
    assert_eq!(map.line_col(SRC.len()), (5, 1));
    assert_eq!(map.line_text(0), "<?php");
}
