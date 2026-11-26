//! Consumer example: component signature validation (architecture doc
//! §17.1). The scanner records declared `function Name(...)` parameter
//! lists, and the validation pass rejects props that are not parameters
//! (`LNL0203`), children passed to a signature without a `children`
//! parameter (`LNL0204`) and invocations that omit a parameter declared
//! without a default value (`LNL0205`) — all guaranteed PHP runtime fatals.
//!
//! Run:
//!
//! ```sh
//! cargo run -p lintl-core --example signature_lint
//! ```

use lintl_core::{compile, Options};

const SOURCE: &str = r#"<?php
function Card(string $heading, iterable $children = []): Html {
    return <b>{$heading}</b>;
}
function Panel(string $title): Html {
    return <section>{$title}</section>;
}
function Field(string $label, string $name): Html {
    return <label>{$label}</label>;
}
return <Card tittel="Hi">Body</Card>;
return <Panel title="Go">Body</Panel>;
return <Field name="email" />;
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
