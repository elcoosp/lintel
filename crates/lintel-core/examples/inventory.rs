//! Consumer example: the component inventory (architecture doc §21.1,
//! `docs/component-inventory.md`). `collect_inventory` turns one scanned
//! file into a shape report — every declared component with its parameter
//! list, every invocation site with its source position and prop names —
//! the data an LSP outline, a metrics collector or a doc generator wants,
//! derived from the compiler's own tree and signature table instead of a
//! re-implementation.
//!
//! Run:
//!
//! ```sh
//! cargo run -p lintel-core --example inventory
//! ```

use lintel_core::scan::scan;
use lintel_core::{collect_inventory, SourceMap};

const SOURCE: &str = r#"<?php
function Card(string $title, int $count = 0): Html {
    return <div class="card">{$title}</div>;
}
function Badge(string $label): Html {
    return <b>{$label}</b>;
}
echo <Card title="Hello" count={2}><b>bold</b></Card>;
echo <Badge label="shipped" />;
echo <Grid rows={$rows} {...$rest} />;
"#;

fn main() {
    let parsed = scan(SOURCE);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let inventory = collect_inventory(&parsed.roots, &parsed.context);
    let map = SourceMap::new(SOURCE);

    println!("declared components (declaration order):");
    for component in &inventory.declared {
        let signature = &component.signature;
        let params: Vec<String> = signature
            .params
            .iter()
            .map(|param| {
                if signature.required.contains(param) {
                    format!("{param}!")
                } else {
                    param.clone()
                }
            })
            .collect();
        println!(
            "  {}({}){}",
            component.name,
            params.join(", "),
            if signature.variadic { ", variadic" } else { "" }
        );
    }

    println!("invocation sites (source order):");
    for site in &inventory.invocations {
        let (line, col) = map.line_col(site.offset);
        let mut flags: Vec<&str> = vec![];
        if site.children {
            flags.push("children");
        }
        if site.spread {
            flags.push("spread");
        }
        if !site.declared_in_file {
            flags.push("undeclared");
        }
        let flags = if flags.is_empty() {
            String::new()
        } else {
            format!(" ({})", flags.join(", "))
        };
        println!(
            "  <{}> at {line}:{col} props: {}{flags}",
            site.name,
            if site.props.is_empty() {
                "none".to_string()
            } else {
                site.props.join(", ")
            }
        );
    }

    println!("-----");
    println!("machine-readable (one JSON line):");
    println!("{}", inventory.to_json(SOURCE));
}
