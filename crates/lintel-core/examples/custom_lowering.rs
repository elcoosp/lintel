//! Consumer example: the `Lowering` emit seam (doc 5.3). `scan_with` takes
//! any `&dyn Lowering` — the same injection point `lowering_for` uses for
//! the built-in `string`/`stream`/`rfc` targets — so a consumer can lower a
//! JSX root however their pipeline needs, without touching the scanner or
//! the AST.
//!
//! This demo composes with the default target instead of replacing it: a
//! `TimedLowering` wraps [`StringLowering`], delegates the root, and times
//! every emission (any observer could sit here — tracing, metrics, a
//! compile cache). The emitted PHP is byte-identical to `compile`'s
//! default-target output; only the seam is exercised.
//!
//! Run:
//!
//! ```sh
//! cargo run -p lintel-core --example custom_lowering
//! ```

use std::sync::Mutex;
use std::time::Instant;

use lintel_core::emit::{Lowering, StringLowering};
use lintel_core::scan::scan_with;

/// Wraps the default string lowering and records one observation per root.
/// `Lowering::emit_root_scoped` takes `&self` (that is what makes the
/// built-in strategies `Sync` and shareable behind `lowering_for`), so an
/// observer uses interior mutability — here a `Mutex`, the same shape a
/// tracing or metrics hook would take. Delegating through the scope-aware
/// method (not plain `emit_root`) keeps the wrapper byte-identical to the
/// default compile, including the scanner's typed-parameter folds
/// (`docs/escape-elision.md`); a wrapper that only implements `emit_root`
/// receives the root-level [`lintel_core::scan::FunctionScope::EMPTY`]
/// context and opts out of them.
struct TimedLowering {
    inner: StringLowering,
    log: Mutex<Vec<(&'static str, u128)>>,
}

impl Lowering for TimedLowering {
    fn emit_root(&self, node: &lintel_core::ast::Node, total_lines: u32) -> String {
        self.emit_root_scoped(node, total_lines, &lintel_core::scan::FunctionScope::EMPTY)
    }

    fn emit_root_scoped(
        &self,
        node: &lintel_core::ast::Node,
        total_lines: u32,
        scope: &lintel_core::scan::FunctionScope,
    ) -> String {
        let start = Instant::now();
        let php = self.inner.emit_root_scoped(node, total_lines, scope);
        self.log
            .lock()
            .expect("observer log mutex")
            .push(("emit_root", start.elapsed().as_nanos()));
        php
    }
}

const SOURCE: &str = r#"<?php
function Badge(string $label): Html {
    return <b class="badge">{$label}</b>;
}
return <Badge label="go" />;
return <i>plain</i>;
"#;

fn main() {
    // The observer log is shared behind the lowering's `&self`.
    let log;
    {
        let lowering = TimedLowering {
            inner: StringLowering,
            log: Mutex::new(Vec::new()),
        };
        let parsed = scan_with(SOURCE, &lowering);
        assert!(
            parsed.diagnostics.is_empty(),
            "demo source must parse cleanly: {:?}",
            parsed.diagnostics
        );
        println!("{} JSX root(s) lowered:\n", parsed.roots.len());
        println!("{SOURCE}");
        println!("── emitted PHP (identical to the default compile) ─────");
        println!("{}", parsed.php);
        log = lowering.log.into_inner().expect("observer log mutex");
    }
    println!("── observer log ────────────────────────────────────────");
    for (event, elapsed) in &log {
        println!("{event}: {elapsed}ns");
    }
}
