//! Consumer example: the CLI engine as a library. [`lintel_cli::run`] is
//! the exact code path the `lintel` binary executes — argument parsing,
//! compilation, diagnostic rendering and exit codes — with the sinks
//! injected, so embedders (daemons, test harnesses, editors) reuse it
//! instead of shelling out.
//!
//! Run:
//!
//! ```sh
//! cargo run -p lintel-cli --example build_library
//! ```

use std::{fs, io::Write as _};

fn main() {
    let dir = std::env::temp_dir().join(format!("lintel-cli-example-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("creates the scratch dir");
    let source = dir.join("Page.lintel.php");
    fs::write(&source, "<?php\nreturn <b class=\"x\">{$name}</b>;\n").expect("writes the source");

    let dist = dir.join("dist");
    let args: Vec<String> = [
        "build",
        "--out",
        dist.to_str().expect("utf8 path"),
        source.to_str().expect("utf8 path"),
    ]
    .iter()
    .copied()
    .map(String::from)
    .collect();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = lintel_cli::run(&args, &mut stdout, &mut stderr).expect("arguments parse");

    let compiled = dist.join("Page.php");
    let php = fs::read_to_string(&compiled).expect("compiled file exists");
    let _ = fs::remove_dir_all(&dir);

    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "exit code: {code}");
    let _ = writeln!(out, "stderr: {}", String::from_utf8_lossy(&stderr).trim());
    let _ = writeln!(out, "compiled Page.php: {}", php.trim());
}
