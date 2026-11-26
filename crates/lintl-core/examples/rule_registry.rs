//! Consumer example: the rule-registry seam. [`ValidationRule::meta`] gives
//! every rule a stable, machine-readable identity — a kebab-case id, the
//! `LNLxxxx` codes it emits and a one-line summary — and
//! [`registry_metas`]/[`builtin_rule_metas`] list a registry's identities
//! in registry order, the way a docs-site generator, editor integration or
//! CI reporter lists what the compiler enforces without re-documenting it
//! (`docs/rule-registry.md`).
//!
//! This demo prints the built-in registry as text and JSON, then shows a
//! custom registry: one consumer rule that classifies itself, one that
//! does not (the default keeps it valid and unlisted).
//!
//! Run:
//!
//! ```sh
//! cargo run -p lintl-core --example rule_registry
//! ```

use std::fmt::Write as _;

use lintl_core::{
    ast::Node, builtin_rule_metas, registry_metas, scan::ScanContext, Code, Diagnostic, RuleMeta,
    ValidationRule,
};

/// A consumer policy that classifies itself: its identity rides in the
/// registry listing exactly as declared.
struct NoHtmlTables;

impl ValidationRule for NoHtmlTables {
    fn check(&self, _node: &Node, _context: &ScanContext, _out: &mut Vec<Diagnostic>) {}
    fn meta(&self) -> Option<RuleMeta> {
        Some(RuleMeta {
            id: "no-html-tables",
            codes: &[Code::VoidChildren],
            summary: "this demo's stand-in policy: no rule logic, identity only",
        })
    }
}

/// A consumer policy that does not classify itself: the `meta` default
/// keeps it valid — it simply reports no registry entry.
struct UnclassifiedPolicy;

impl ValidationRule for UnclassifiedPolicy {
    fn check(&self, _node: &Node, _context: &ScanContext, _out: &mut Vec<Diagnostic>) {}
}

fn main() {
    println!("built-in registry, text:\n");
    for meta in builtin_rule_metas() {
        let codes = meta
            .codes
            .iter()
            .map(|code| code.lnt())
            .collect::<Vec<_>>()
            .join(",");
        println!("  {} {codes} {}", meta.id, meta.summary);
    }

    println!("\nbuilt-in registry, JSON Lines:\n");
    for meta in builtin_rule_metas() {
        println!("  {}", meta.to_json());
    }

    let registry: [&dyn ValidationRule; 2] = [&UnclassifiedPolicy, &NoHtmlTables];
    let metas = registry_metas(&registry);
    let mut report = String::new();
    for (index, meta) in metas.iter().enumerate() {
        match meta {
            Some(meta) => {
                let _ = writeln!(
                    report,
                    "  [{}] classified: {} ({})",
                    index,
                    meta.id,
                    meta.to_json()
                );
            }
            None => {
                let _ = writeln!(report, "  [{index}] unclassified (default `meta()` = None)");
            }
        }
    }
    println!("\ncustom registry, one entry per rule in registry order:\n{report}");
}
