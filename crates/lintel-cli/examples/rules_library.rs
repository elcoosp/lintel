//! Consumer example: the `lintel rules` registry listing as a library. The
//! `rules` subcommand prints the built-in validation registry — one
//! `<id> <LNTxxxx codes> <summary>` line per rule, or one JSON object per
//! rule with `--format=json` — without scanning any source
//! (`docs/rule-registry.md`). Like every engine command, the exact code
//! path runs through [`lintel_cli::run`] with the sinks injected.
//!
//! Run:
//!
//! ```sh
//! cargo run -p lintel-cli --example rules_library
//! ```

use std::process;

fn run_rules(args: &[&str]) -> (i32, String, String) {
    let invocation: Vec<String> = args.iter().copied().map(String::from).collect();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = lintel_cli::run(&invocation, &mut stdout, &mut stderr).expect("arguments parse");
    (
        code,
        String::from_utf8(stdout).expect("stdout is utf8"),
        String::from_utf8(stderr).expect("stderr is utf8"),
    )
}

fn main() {
    let (code, text, stderr) = run_rules(&["rules"]);
    println!("text ({} rule lines):", text.lines().count());
    print!("{text}");

    let (json_code, json, json_stderr) = run_rules(&["rules", "--format=json"]);
    println!("-----");
    print!("{json}");

    assert_eq!(code, 0, "the registry listing never fails: {stderr}");
    assert_eq!(json_code, 0, "{json_stderr}");
    assert!(json_stderr.is_empty(), "no diagnostics: {json_stderr:?}");
    assert_eq!(
        text.lines().count(),
        json.lines().count(),
        "both formats list the same registry"
    );
    assert!(
        json.starts_with("{\"id\":\"builtin-shadow\",\"codes\":[\"LNT0201\"],"),
        "stable field order, registry order: {json}"
    );
    process::exit(code);
}
