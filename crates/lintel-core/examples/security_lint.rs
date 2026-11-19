//! Consumer example: the compile-time validation pass turning unsafe markup
//! into stable, rendered LNT diagnostics (builtin shadow, dynamic handler,
//! void children).
//!
//! Run:
//!
//! ```sh
//! cargo run -p lintel-core --example security_lint
//! ```

use lintel_core::{compile, Options};

const SOURCE: &str = r#"<?php
// A component whose name collides with PHP's builtin `header()`.
return <Header title="Admin">
    <button onclick={$handler}>Save</button>
    <br>
        Unexpected content inside a void element
    </br>
</Header>;
"#;

fn main() {
    match compile(SOURCE, &Options::default()) {
        Ok(out) => {
            println!("unexpectedly compiled:\n{}", out.php);
        }
        Err(problems) => {
            println!("{} violation(s) found:\n", problems.len());
            for problem in &problems {
                println!("{}", problem.render(SOURCE));
            }
        }
    }
}
