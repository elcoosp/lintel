//! Consumer example: component signature validation (architecture doc
//! §17.1). The scanner records declared `function Name(...)` parameter
//! lists, and the validation pass rejects props that are not parameters
//! (`LNT0203`) and children passed to a signature without a `children`
//! parameter (`LNT0204`) — both would fatal at runtime with PHP's
//! `Unknown named parameter`.
//!
//! Run:
//!
//! ```sh
//! cargo run -p lintel-core --example signature_lint
//! ```

use lintel_core::{compile, Options};

const SOURCE: &str = r#"<?php
function Card(string $heading, iterable $children = []): Html {
    return <b>{$heading}</b>;
}
function Panel(string $title): Html {
    return <section>{$title}</section>;
}
return <Card tittel="Hi">Body</Card>;
return <Panel title="Go">Body</Panel>;
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
