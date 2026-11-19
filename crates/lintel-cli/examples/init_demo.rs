//! Consumer example: `lintel init` as a library (task-card T-10
//! acceptance: "`lintel init` yields a runnable demo in one command").
//! [`lintel_cli::init`] scaffolds the demo `hello.lintel.php` source into a
//! target directory — refusing to clobber — and compiles it once to
//! `hello.php` with the same engine `lintel build` uses, so one command
//! leaves a runnable page. Embedders (installers, editors, test harnesses)
//! call the same function the binary runs.
//!
//! Run:
//!
//! ```sh
//! cargo run -p lintel-cli --example init_demo
//! ```

use std::{fs, io::Write as _};

fn main() {
    let dir = std::env::temp_dir().join(format!("lintel-init-example-{}", std::process::id()));
    let report = lintel_cli::init(&dir).expect("scaffolds the demo");
    let php = fs::read_to_string(&report.compiled).expect("the demo compiled in the same call");

    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "scaffolded: {}", report.source.display());
    let _ = writeln!(out, "compiled:   {}", report.compiled.display());
    let _ = writeln!(out, "lines:      {}", report.lines);
    let _ = writeln!(out, "── compiled hello.php ──────────────────────");
    let _ = writeln!(out, "{php}");
    let _ = fs::remove_dir_all(&dir);
}
