//! Consumer example: warning-level diagnostics. A static `onclick="…"`
//! string is allowed (doc 4.4) but carries an `LNT0302` warning that rides
//! along on [`lintel_core::Output::warnings`] instead of failing.
//!
//! Run:
//!
//! ```sh
//! cargo run -p lintel-core --example warnings_lint
//! ```

use lintel_core::{compile, Options};

const SOURCE: &str = r#"<?php
return <nav>
    <button onclick="track('cta')">Save</button>
    <a href="/docs">Docs</a>
</nav>;
"#;

fn main() {
    match compile(SOURCE, &Options::default()) {
        Ok(out) => {
            println!("compiled ({} warning(s)):\n", out.warnings.len());
            println!("{}", out.php);
            println!("── warnings ──────────────────────────────");
            for warning in &out.warnings {
                println!("{}", warning.render(SOURCE));
            }
        }
        Err(problems) => {
            for problem in &problems {
                println!("{}", problem.render(SOURCE));
            }
            std::process::exit(1);
        }
    }
}
