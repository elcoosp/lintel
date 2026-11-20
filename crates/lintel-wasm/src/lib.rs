//! WASM entry crate for the browser playground (task card T-13).
//!
//! The compiler core stays dependency-free: this crate is the card-named
//! freeze exception that wraps [`lintel_core::compile`] in one
//! `wasm-bindgen` export and returns a self-describing JSON envelope the
//! playground JS can consume without any additional glue:
//!
//! ```json
//! {"ok": true, "php": "…", "diagnostics": [], "error": null}
//! ```
//!
//! `diagnostics` carries both error-severity entries (the compile failed,
//! so `php` is null) and warning-severity entries (the file compiled, the
//! advice rides along), distinguishable by their `severity` field. Each
//! entry carries the machine fields of the `Diagnostic::to_json` contract
//! (`docs/diagnostics-json.md`) plus a rendered, source-annotated form for
//! the playground's diagnostics panel. The target words match the CLI's
//! `--target` exactly: `string`, `stream` and `rfc`.
//!
//! The size budget (acceptance: "loads <1 MB gz") is asserted by
//! `tests/size_budget.rs` over the built-and-bound `wasm32-unknown-unknown`
//! artifact; the committed playground bundle is produced by
//! `make playground`.

use lintel_core::{compile, json_escape, Diagnostic, Options, Target};
use wasm_bindgen::prelude::wasm_bindgen;

/// The compiler version the playground footer displays.
#[wasm_bindgen]
#[must_use]
pub fn lintel_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Compile `source` on the named lowering target and return the JSON
/// envelope the playground renders.
///
/// `target` accepts the CLI's exact words — `string` (the default when
/// empty), `stream`, `rfc`; anything else returns `ok: false` with an
/// `error` message and no diagnostics. The envelope is always valid JSON:
/// the compiler reports source problems as diagnostics instead of panicking,
/// so the export itself cannot fail.
#[wasm_bindgen]
#[must_use]
pub fn compile_source(source: &str, target: &str) -> String {
    let parsed = match target {
        "" | "string" => Some(Target::String),
        "stream" => Some(Target::Stream),
        "rfc" => Some(Target::Rfc),
        _ => None,
    };
    let Some(target) = parsed else {
        return envelope(
            None,
            &[],
            &format!("unknown target `{target}`: expected string, stream or rfc"),
        );
    };
    match compile(source, &Options { target }) {
        Ok(out) => envelope(
            Some(&out.php),
            &diagnostic_entries(source, &out.warnings),
            "",
        ),
        Err(problems) => envelope(None, &diagnostic_entries(source, &problems), ""),
    }
}

/// Assemble the envelope: `{ok, php, diagnostics, error}` in a stable field
/// order. `php` present means the compile succeeded; `error` (a top-level
/// API misuse such as an unknown target word) is set only when nothing
/// compiled at all.
fn envelope(php: Option<&str>, diagnostics: &[String], error: &str) -> String {
    let mut out = String::with_capacity(128);
    out.push_str("{\"ok\":");
    out.push_str(if php.is_some() { "true" } else { "false" });
    out.push_str(",\"php\":");
    match php {
        // json_escape writes the escaped content without its quotes.
        Some(php) => {
            out.push('"');
            json_escape(php, &mut out);
            out.push('"');
        }
        None => out.push_str("null"),
    }
    out.push_str(",\"diagnostics\":");
    push_array(&mut out, diagnostics);
    out.push_str(",\"error\":");
    if error.is_empty() {
        out.push_str("null");
    } else {
        out.push('"');
        json_escape(error, &mut out);
        out.push('"');
    }
    out.push('}');
    out
}

/// Error- and warning-severity diagnostics share one shape; the compile
/// outcome differs only in whether `php` rides along.
fn diagnostic_entries(src: &str, problems: &[Diagnostic]) -> Vec<String> {
    problems
        .iter()
        .map(|problem| diagnostic_json(src, &problem.render(src), problem))
        .collect()
}

/// One diagnostic object: the machine fields of the `Diagnostic::to_json`
/// contract (code, severity, message, offset, line, col, help — the stable
/// field order of `docs/diagnostics-json.md`) plus a `rendered` form for
/// the source-annotated panel.
fn diagnostic_json(src: &str, rendered: &str, diagnostic: &Diagnostic) -> String {
    let machine = diagnostic.to_json(src);
    let mut out = String::with_capacity(machine.len() + rendered.len() + 16);
    out.push_str(&machine[..machine.len() - 1]); // splice into the object
    out.push_str(",\"rendered\":\"");
    json_escape(rendered, &mut out);
    out.push_str("\"}");
    out
}

/// A JSON array literal over pre-serialized object strings.
fn push_array(out: &mut String, items: &[String]) {
    out.push('[');
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(item);
    }
    out.push(']');
}

#[cfg(test)]
mod tests {
    use super::compile_source;

    #[test]
    fn valid_source_compiles_to_an_ok_envelope() {
        let json = compile_source("<?php\nreturn <b>hi</b>;\n", "string");
        assert!(json.starts_with("{\"ok\":true,\"php\":\""));
        assert!(json.contains("\"diagnostics\":[]"));
        assert!(json.contains("\"error\":null"));
        assert!(json.contains("new \\\\Lintel\\\\Html("), "{json}");
    }

    #[test]
    fn broken_source_returns_diagnostics_with_rendered_form() {
        let json = compile_source("<?php\nreturn <b></i>;\n", "string");
        assert!(json.starts_with("{\"ok\":false,\"php\":null"));
        assert!(json.contains("\"code\":\"LNT0102\""), "{json}");
        assert!(json.contains("\"rendered\":\""), "{json}");
        assert!(json.contains("expected </b>"), "{json}");
    }

    #[test]
    fn warnings_ride_along_with_the_php() {
        let json = compile_source(
            "<?php\nreturn <button onclick=\"track('cta')\">go</button>;\n",
            "string",
        );
        assert!(json.starts_with("{\"ok\":true"), "{json}");
        assert!(json.contains("\"code\":\"LNT0302\""), "{json}");
    }

    #[test]
    fn unknown_target_words_return_the_api_error() {
        let json = compile_source("<?php\nreturn <b>hi</b>;\n", "nope");
        assert!(json.starts_with("{\"ok\":false,\"php\":null"), "{json}");
        assert!(json.contains("unknown target `nope`"), "{json}");
        assert!(json.contains("\"diagnostics\":[]"));
    }

    #[test]
    fn stream_target_reaches_the_generator_emission() {
        let json = compile_source("<?php\nreturn <b>{$name}</b>;\n", "stream");
        assert!(
            json.contains("static function () use ($name): \\\\Generator"),
            "{json}"
        );
    }

    #[test]
    fn empty_target_word_defaults_to_string() {
        let json = compile_source("<?php\nreturn <b>hi</b>;\n", "");
        assert!(json.starts_with("{\"ok\":true"), "{json}");
        assert!(json.contains("new \\\\Lintel\\\\Html("), "{json}");
    }
}
