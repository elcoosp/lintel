//! Consumer example: typed-parameter escape elision (architecture doc §7.3,
//! the last v0.1 "honest version" bullet). A dynamic child that is exactly
//! a parameter the enclosing function declares as a non-nullable `string`
//! folds to a direct `\htmlspecialchars(...)` call instead of the generic
//! `\Lintel\c()` helper — the type declaration proves the value is a
//! string, so the wrapper's runtime check is pure overhead. The scanner
//! tracks the enclosing function scope, and any construct that can rebind
//! the name (`global`, assignment, `foreach … as`, destructuring) or bind
//! dynamically (`extract`, `$$name`, `include`, `eval`) keeps the helper,
//! conservative by design (`docs/escape-elision.md`).
//!
//! Run:
//!
//! ```sh
//! cargo run -p lintel-core --example typed_elision
//! ```

use lintel_core::{compile, Options};

const TYPED: &str = r#"<?php
function Card(string $title, int $count): Html {
    return <div class="card">
        <h2>{$title}</h2>
        <p>{$count} items — the int is not a string param, and {$note} is undeclared.</p>
    </div>;
}
"#;

const REBOUND: &str = "<?php\n\
function Card(string $title): Html {
    $title = $title . ' (edited)';   // an assignment unproves the name
    return <b>{$title}</b>;
}
";

fn main() {
    let typed = compile(TYPED, &Options::default()).expect("typed source compiles");
    println!("typed parameters elide:\n{}", typed.php);
    println!("-----");
    println!(
        "elided (direct escape):  `{{$title}}` — `string $title` proves the type\n\
         kept the `c()` helper:   `{{$count}}` (typed `int`), `{{$note}}` (undeclared)"
    );
    println!("-----");
    let rebound = compile(REBOUND, &Options::default()).expect("rebound source compiles");
    println!("a rebound name keeps the helper:\n{}", rebound.php);
}
