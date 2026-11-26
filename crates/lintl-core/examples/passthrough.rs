//! Consumer example: the two structural guarantees every lowering target
//! shares — byte-identical verbatim pass-through (README guarantee 1) and
//! line preservation (guarantee 2, doc 13.4). The PHP parts of a file are
//! never rewritten, and the compiled output keeps exactly the source's line
//! count under `string`, `stream` and `rfc` targets alike.
//!
//! Run:
//!
//! ```sh
//! cargo run -p lintl-core --example passthrough
//! ```

use lintl_core::{compile, Options, Target};

const SOURCE: &str = r#"<?php
declare(strict_types=1);

// PHP without JSX passes through byte-for-byte, comments and all.
$items = ['a', 'b'];
$template = "<li>{$items[0]}</li>";  // strings/heredocs are never JSX
if (count($items) < 3 && $items !== []) {
    echo "less-than is the operator here, not a tag\n";
}

return <ul class="list">
    <li>{implode(', ', $items)}</li>
</ul>;
"#;

fn main() {
    let baseline = match compile(SOURCE, &Options::default()) {
        Ok(out) => out,
        Err(problems) => {
            for problem in &problems {
                println!("{}", problem.render(SOURCE));
            }
            std::process::exit(1);
        }
    };

    println!("── string target (default) ───────────────");
    println!("{}", baseline.php);
    println!("lines in = lines out = {}", baseline.php.lines().count());

    for target in [Target::Stream, Target::Rfc] {
        let opts = Options { target };
        match compile(SOURCE, &opts) {
            Ok(out) => {
                println!("── {target:?} target ─────────────────────────────");
                println!("{}", out.php);
                assert_eq!(out.php.lines().count(), SOURCE.lines().count());
                assert_eq!(out.php.lines().count(), baseline.php.lines().count());
            }
            Err(problems) => {
                for problem in &problems {
                    println!("{}", problem.render(SOURCE));
                }
                std::process::exit(1);
            }
        }
    }
    println!("all three targets preserve the source line count");
}
