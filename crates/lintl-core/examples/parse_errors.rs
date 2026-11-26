//! Consumer example: parse diagnostics (`LNL01xx`) — scanner output, never
//! rule-selected and never suppressible. Every diagnostic carries a stable
//! code, a source offset and an actionable `help:` line; a mismatched close
//! tag that near-misses the open tag in scope additionally gets a
//! "did you mean" suggestion. [`Diagnostic::render`] draws the
//! miette-style snippet with a caret, no dependencies needed.
//!
//! Run:
//!
//! ```sh
//! cargo run -p lintl-core --example parse_errors
//! ```

use lintl_core::{compile, Options};

const SOURCES: [&str; 2] = [
    // `</sapn>` is two edits from `</span>` — the help line names the fix.
    "<?php\nreturn <span>text</sapn>;\n",
    // A tag never closed: the help line says what to write.
    "<?php\nreturn <div>\n    <p>unterminated\n",
];

fn main() {
    for source in SOURCES {
        match compile(source, &Options::default()) {
            Ok(out) => println!("unexpectedly compiled:\n{}", out.php),
            Err(problems) => {
                for problem in &problems {
                    println!("{}", problem.render(source));
                }
            }
        }
        println!();
    }
}
