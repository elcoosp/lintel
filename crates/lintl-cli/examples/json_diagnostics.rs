//! Consumer example: machine-readable diagnostics. `lintl check
//! --format=json` (and the same flag on `build`) emits one
//! [`lintl_core::Diagnostic::to_json`] object per diagnostic on stderr —
//! stable `LNLxxxx` code, severity, message, byte offset, 1-based
//! line/col, optional `help:` line and the docs URL — so CI jobs, editors
//! and LSP-style consumers parse compiler output instead of scraping
//! rendered snippets. Exit codes are identical to text format.
//!
//! Run:
//!
//! ```sh
//! cargo run -p lintl-cli --example json_diagnostics
//! ```

use std::{fs, io::Write as _};

fn main() {
    let dir = std::env::temp_dir().join(format!("lintl-json-example-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("creates the scratch dir");
    let warned = dir.join("Panel.lintl.php");
    // A static `onclick` string compiles but carries an LNL0302 warning.
    fs::write(
        &warned,
        "<?php\nreturn <button onclick=\"track('cta')\">go</button>;\n",
    )
    .expect("writes the warned source");
    let rejected = dir.join("Bad.lintl.php");
    // A dynamic `onclick` is an LNL0301 error: the file fails the gate.
    fs::write(
        &rejected,
        "<?php\nreturn <button onclick={$h}>go</button>;\n",
    )
    .expect("writes the error source");

    let args: Vec<String> = [
        "check",
        warned.to_str().expect("utf8 path"),
        rejected.to_str().expect("utf8 path"),
        "--format=json",
    ]
    .iter()
    .copied()
    .map(String::from)
    .collect();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = lintl_cli::run(&args, &mut stdout, &mut stderr).expect("arguments parse");
    let _ = fs::remove_dir_all(&dir);

    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "exit code: {code} (1: the error file failed the gate)");
    let _ = writeln!(out, "one JSON object per diagnostic:");
    for line in String::from_utf8_lossy(&stderr).lines() {
        let _ = writeln!(out, "  {line}");
    }
}
