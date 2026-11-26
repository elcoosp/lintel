//! Consumer example: the file-level `lintl-ignore-file` directive (see
//! `docs/ignore-directives.md`). One PHP line comment anywhere in the file —
//! `// lintl-ignore-file` (or `# …`, with an optional `: reason`) — drops
//! every validation diagnostic in the file, errors and warnings alike,
//! while parse errors (`LNL01xx`) stay fatal. Unlike the one-line
//! `lintl-ignore-next` window the flag is position independent and the
//! validation pass short-circuits before running any rule.
//!
//! This demo compiles the same legacy page twice: once bare (the builtin
//! shadow lint `LNL0201` fails the compile) and once with the directive
//! (clean compile, zero warnings).
//!
//! Run:
//!
//! ```sh
//! cargo run -p lintl-core --example ignore_file
//! ```

use lintl_core::{compile, Options};

const LEGACY: &str = r#"<?php
function Badge(string $label): Html {
    return <b class="badge">{$label}</b>;
}
return <Header title="Archive" />;
return <Badge label="old" />;
return <button onclick="track('cta')">Legacy CTA</button>;
"#;

const SUPPRESSED: &str = r#"<?php
// lintl-ignore-file: generated archive page, audited in CMS-77
function Badge(string $label): Html {
    return <b class="badge">{$label}</b>;
}
return <Header title="Archive" />;
return <Badge label="old" />;
return <button onclick="track('cta')">Legacy CTA</button>;
"#;

fn main() {
    println!("── without the directive ───────────────────────────────");
    match compile(LEGACY, &Options::default()) {
        Ok(out) => println!("compiled with {} warning(s)", out.warnings.len()),
        Err(problems) => {
            println!("{} violation(s) found:\n", problems.len());
            for problem in &problems {
                println!("{}", problem.render(LEGACY));
            }
        }
    }

    println!("\n── with // lintl-ignore-file ──────────────────────────");
    match compile(SUPPRESSED, &Options::default()) {
        Ok(out) => {
            println!(
                "compiled clean ({} warning(s)) — every validation diagnostic dropped",
                out.warnings.len()
            );
            println!("first lines of output:\n");
            for line in out.php.lines().take(3) {
                println!("{line}");
            }
        }
        Err(problems) => {
            println!("{} violation(s) found:\n", problems.len());
            for problem in &problems {
                println!("{}", problem.render(SUPPRESSED));
            }
        }
    }
}
