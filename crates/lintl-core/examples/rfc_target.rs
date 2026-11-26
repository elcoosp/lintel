//! Consumer example: the RFC-parity lowering target (doc 5.3 `rfc`).
//!
//! The same component source is compiled to both targets side by side: the
//! default string target emits `new \Lintl\Html(...)` with runtime helper
//! calls, the `rfc` target emits `new \Markup\Element(...)` constructor
//! calls / named-argument component calls with no `\Lintl\` helpers.
//!
//! Run:
//!
//! ```sh
//! cargo run -p lintl-core --example rfc_target
//! ```

use lintl_core::{compile, Options, Target};

const SOURCE: &str = r#"<?php
function Badge(string $label, bool $verified = false): Html {
    return (
        <span class="badge">
            {$label}
            {$verified && <i class="icon-check" />}
        </span>
    );
}
"#;

fn compiled(target: Target) -> String {
    let opts = Options { target };
    match compile(SOURCE, &opts) {
        Ok(out) => out.php,
        Err(problems) => {
            for problem in &problems {
                println!("{}", problem.render(SOURCE));
            }
            std::process::exit(1);
        }
    }
}

fn main() {
    let string_php = compiled(Target::String);
    let rfc_php = compiled(Target::Rfc);

    println!("── string target (default) ───────────────");
    println!("{string_php}");
    println!("── rfc target (\\Markup\\Element parity) ────");
    println!("{rfc_php}");

    let helpers = rfc_php.contains("\\Lintl\\");
    println!(
        "lines in = lines out = {} for both; rfc output references \\Lintl\\ helpers: {helpers}",
        string_php.lines().count()
    );
}
