//! Consumer example: escape elision (architecture doc §7.3, "escape
//! elision (honest version)"). A dynamic child whose expression is
//! provably a non-string scalar — an `(int)`/`(float)` cast spanning the
//! whole expression, or a `count()` call — folds to the raw PHP
//! expression instead of the `\Lintel\c()` escape helper: an int/float
//! interpolates as digits and cannot carry markup. Everything else keeps
//! the helper, conservative by design.
//!
//! Run:
//!
//! ```sh
//! cargo run -p lintel-core --example elision
//! ```

use lintel_core::{compile, Options};

const SOURCE: &str = r#"<?php
return <p class="stats">
    {(int)$n} of {count($rows)} rows — but {$note} and {(string)$note} stay escaped.
</p>;
"#;

fn main() {
    let out = compile(SOURCE, &Options::default()).expect("compiles");
    println!("compiled PHP:\n{}", out.php);
    println!("-----");
    println!("elided (no helper):   `(int)$n`, `count($rows)` — provably scalar");
    println!("kept the `c()` helper: `{{$note}}` (unknown type), `{{(string)$note}}` (a string can carry markup)");
}
