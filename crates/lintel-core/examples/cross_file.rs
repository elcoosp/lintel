//! Consumer example: the cross-file signature-aggregation seam.
//! [`ScanContext::merged_with`] unions two files' declaration tables, so a
//! file's component invocations validate against components declared in
//! *other* files — `LNT0201`/`LNT0203`/`LNT0204`/`LNT0205` become
//! project-wide checks when a consumer validates against the merged
//! context. The merge is read-only on both inputs, keeps the base file's
//! signature on name collisions, and deliberately does not carry
//! suppression state across files (`docs/cross-file-signatures.md`).
//!
//! This demo compiles the same base file twice: once against its own
//! (empty) declaration table, where the cross-file invocation is unchecked,
//! and once against the merged table, where the foreign `Card` signature
//! reports the unknown prop.
//!
//! Run:
//!
//! ```sh
//! cargo run -p lintel-core --example cross_file
//! ```

use lintel_core::builtin_rules;
use lintel_core::scan::scan;
use lintel_core::validate::validate_with;
use lintel_core::Severity;

/// The file being validated: invokes a `Card` component it does not
/// declare, with one unknown prop.
const BASE: &str = "<?php\n$page = <Card title=\"Hello\" oops=\"bad\" />;\n";

/// The foreign file: declares `Card` with one `string` parameter.
const FOREIGN: &str =
    "<?php\nfunction Card(string $title): Html {\n    return <b>{$title}</b>;\n}\n";

fn main() {
    let base = scan(BASE);
    let foreign = scan(FOREIGN);
    assert!(
        base.diagnostics.is_empty() && foreign.diagnostics.is_empty(),
        "both files parse cleanly"
    );

    let plain = validate_with(&base.roots, &base.context, builtin_rules());
    println!(
        "own context:  {} diagnostic(s) — `Card` is declared abroad, prop checks skip",
        plain.len()
    );

    let merged = base.context.merged_with(&foreign.context);
    let found = validate_with(&base.roots, &merged, builtin_rules());
    println!(
        "merged context: {} diagnostic(s) — the foreign signature resolves:",
        found.len()
    );
    for problem in &found {
        let map = lintel_core::SourceMap::new(BASE);
        let (line, col) = map.line_col(problem.offset);
        println!(
            "  {line}:{col} {}[{}] {}",
            problem.severity.prefix(),
            problem.code.lnt(),
            problem.message
        );
        if let Some(help) = &problem.help {
            println!("    = help: {help}");
        }
    }

    assert!(plain.is_empty(), "the own context checks nothing");
    assert_eq!(found.len(), 1, "the merged context reports LNT0203");
    assert_eq!(found[0].severity, Severity::Error);
    assert!(
        found[0]
            .help
            .as_deref()
            .is_some_and(|help| help.contains("$title")),
        "the help names the foreign parameter list"
    );
}
