//! Consumer example: the `lintl stats` shape report as a library. The
//! `stats` subcommand runs the scanner over each source and prints one
//! section per file — the declared components with their parameter lists
//! and every invocation site with its position and props — without
//! compiling PHP or running validation lints (`docs/component-inventory.md`).
//! Like every engine command, the exact code path runs through
//! [`lintl_cli::run`] with the sinks injected.
//!
//! Run:
//!
//! ```sh
//! cargo run -p lintl-cli --example stats_library
//! ```

use std::{fs, io::Write as _};

fn main() {
    let dir = std::env::temp_dir().join(format!("lintl-stats-example-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("creates the scratch dir");
    let source = dir.join("Page.lintl.php");
    fs::write(
        &source,
        "<?php\n\
         function Card(string $title, int $count = 0): Html {\n\
         \x20   return <div class=\"card\">{$title}</div>;\n\
         }\n\
         echo <Card title=\"Hello\">bold text</Card>;\n\
         echo <Grid rows={$rows} {...$rest} />;\n",
    )
    .expect("writes the source");

    let args: Vec<String> = ["stats", source.to_str().expect("utf8 path")]
        .iter()
        .copied()
        .map(String::from)
        .collect();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = lintl_cli::run(&args, &mut stdout, &mut stderr).expect("arguments parse");
    std::io::stdout().write_all(&stdout).expect("flushes");

    let json_args: Vec<String> = [
        "stats",
        "--format=json",
        source.to_str().expect("utf8 path"),
    ]
    .iter()
    .copied()
    .map(String::from)
    .collect();
    let mut json_out = Vec::new();
    let mut json_err = Vec::new();
    let json_code =
        lintl_cli::run(&json_args, &mut json_out, &mut json_err).expect("arguments parse");
    println!("-----");
    std::io::stdout().write_all(&json_out).expect("flushes");

    assert_eq!(code, 0, "the report runs cleanly");
    assert_eq!(json_code, 0);
    assert!(json_err.is_empty(), "no diagnostics: {json_err:?}");
    let _ = fs::remove_dir_all(&dir);
}
