//! Consumer example: compile a Lintel component file to plain PHP.
//!
//! Run:
//!
//! ```sh
//! cargo run -p lintel-core --example compile_demo
//! ```

use lintel_core::{compile, Options};

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

fn main() {
    match compile(SOURCE, &Options::default()) {
        Ok(out) => {
            println!("── compiled PHP ──────────────────────────");
            println!("{}", out.php);
            println!("lines in = lines out = {}", out.php.lines().count());
        }
        Err(problems) => {
            for problem in &problems {
                println!("{}", problem.render(SOURCE));
            }
            std::process::exit(1);
        }
    }
}
