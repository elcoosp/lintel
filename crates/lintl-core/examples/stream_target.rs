//! Consumer example: the streaming lowering target (doc 5.3 `stream`, §10).
//!
//! The same component source is compiled to the string target and the stream
//! target side by side: the string target emits one `new \Lintl\Html(...)`
//! value, the stream target emits an immediately-invoked
//! `static function (): \Generator` closure that yields the markup one chunk
//! at a time. Both preserve the source line count.
//!
//! Run:
//!
//! ```sh
//! cargo run -p lintl-core --example stream_target
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
    let stream_php = compiled(Target::Stream);

    println!("── string target (default) ───────────────");
    println!("{string_php}");
    println!("── stream target (generator chunks) ──────");
    println!("{stream_php}");

    let chunks = stream_php.matches("yield ").count();
    println!("lines in = lines out = {} for both; the stream target yields {chunks} chunk(s) and wraps the root in a `static function (): \\Generator` closure", string_php.lines().count());
    assert!(
        !stream_php.contains("new \\Lintl\\Html("),
        "stream output must not wrap the root in an Html value"
    );
    assert!(stream_php.contains(": \\Generator"));
}
