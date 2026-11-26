//! Consumer example: the `SourceMap` seam. Every [`Diagnostic`] anchors on a
//! byte offset; editors, CLIs and LSPs speak in 1-based line/column pairs.
//! `SourceMap` is the public offset→position converter the renderer itself
//! uses (`lintl_diag::SourceMap`, re-exported at the `lintl-core` root),
//! with clamp-and-floor semantics: out-of-range and mid-character offsets
//! resolve instead of panicking.
//!
//! Run:
//!
//! ```sh
//! cargo run -p lintl-core --example sourcemap
//! ```

use lintl_core::{compile, Options, SourceMap};

const SOURCE: &str = r#"<?php
// le composant title
function Card(): Html {
    return <b class="badge">{$label}</b>;
}
return <Card tittel="Hi" />;
"#;

fn main() {
    let problems = compile(SOURCE, &Options::default())
        .err()
        .unwrap_or_else(|| panic!("expected the unknown-prop diagnostic"));
    let map = SourceMap::new(SOURCE);
    for problem in &problems {
        let (line, col) = map.line_col(problem.offset);
        println!("{} at byte {}", problem.headline(), problem.offset);
        println!("  editor position: line {line}, column {col} (chars, 1-based)");
        println!(
            "  full line: {}",
            map.line_text(problem.offset).trim_start()
        );
        println!("{}", problem.render(SOURCE));
    }
    // The seam is total: even a nonsense offset resolves to the last
    // position instead of panicking.
    let (line, col) = map.line_col(SOURCE.len() + 1_000);
    println!("offset past EOF clamps to line {line}, column {col}");
}
