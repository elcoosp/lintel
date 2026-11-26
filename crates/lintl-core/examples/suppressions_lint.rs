//! Consumer example: `lintl-ignore-next` suppression directives (see
//! `docs/ignore-directives.md`). A PHP line comment above a line silences
//! the validation diagnostics anchored there — error and warning severity
//! alike — while the same rule keeps firing everywhere else. Parse errors
//! (`LNL01xx`) are never suppressible.
//!
//! Run:
//!
//! ```sh
//! cargo run -p lintl-core --example suppressions_lint
//! ```

use lintl_core::{compile, Options};

const SOURCE: &str = r#"<?php
// lintl-ignore-next: legacy header, kept byte-identical until CMS-123 lands
return <Header title="Ignored shadow" />;
return <button onclick="track('cta')">Legacy CTA</button>;
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
            println!("{} violation(s) found:\n", problems.len());
            for problem in &problems {
                println!("{}", problem.render(SOURCE));
            }
        }
    }
}
