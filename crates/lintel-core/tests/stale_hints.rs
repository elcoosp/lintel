//! The analyze seam's stale-hint surface (§7.3-Y2 UX, `docs/type-elision.md`):
//! a report entry that matches no child expression of the CURRENT source is
//! stale — the analysis ran against an older artifact. `stale_hints` is the
//! seam `lintel check --types` and the analyze report surface it with, so a
//! stale report degrades loudly instead of silently compiling without it.

use lintel_core::{compile_analyzed, ChildHint, ChildKind, Options, TypeHints};

fn hints(entries: Vec<ChildHint>) -> TypeHints {
    TypeHints::from_entries(entries)
}

fn hint(line: u32, expr: &str, kind: ChildKind) -> ChildHint {
    ChildHint {
        line,
        expr: expr.to_string(),
        kind,
    }
}

const SRC: &str = "<?php\nfunction Page(int $id, $label) {\n  return <><p>{$id}</p><span>{$label}</span></>;\n}\n";

#[test]
fn a_hint_matching_no_child_is_stale() {
    let table = hints(vec![hint(3, "$id", ChildKind::Int)]);
    let stale = lintel_core::stale_hints(SRC, &table);
    // `$id` matches the line-3 child; the stale list is empty.
    assert!(stale.is_empty(), "{stale:?}");
    let table = hints(vec![hint(2, "$id", ChildKind::Int)]);
    let stale = lintel_core::stale_hints(SRC, &table);
    assert_eq!(stale.len(), 1, "{stale:?}");
    assert_eq!(stale[0].line, 2);
    assert_eq!(stale[0].expr, "$id");
    assert_eq!(stale[0].kind, "int");
}

#[test]
fn every_unmatched_entry_reports_with_its_kind_spelling() {
    let table = hints(vec![
        hint(99, "$ghost", ChildKind::Int),
        hint(98, "MODE", ChildKind::Literal("row".into())),
        hint(97, "$x", ChildKind::NumericString),
        hint(3, "$id", ChildKind::Int), // fresh — matches
    ]);
    let stale = lintel_core::stale_hints(SRC, &table);
    let mut spellings: Vec<&str> = stale.iter().map(|s| s.kind.as_str()).collect();
    spellings.sort_unstable();
    assert_eq!(spellings, vec!["int", "literal", "numeric-string"]);
}

#[test]
fn a_conflicting_entry_that_matched_is_not_stale() {
    // The poisoned key still NAMES a real child; the ambiguity keeps the
    // helper but the hint is not stale — it found its position.
    let table = hints(vec![
        hint(3, "$id", ChildKind::Int),
        hint(3, "$id", ChildKind::Float),
    ]);
    let stale = lintel_core::stale_hints(SRC, &table);
    assert!(stale.is_empty(), "{stale:?}");
}

#[test]
fn an_empty_report_has_no_stale_hints() {
    let stale = lintel_core::stale_hints(SRC, &TypeHints::default());
    assert!(stale.is_empty(), "{stale:?}");
}

#[test]
fn the_elision_still_applies_where_hints_are_fresh() {
    // The surface is advisory: fresh hints elide exactly as before.
    let table = hints(vec![hint(3, "$id", ChildKind::Int)]);
    let out = compile_analyzed(
        SRC,
        &Options::default(),
        &table,
        lintel_core::builtin_rules(),
    )
    .expect("compiles");
    assert_eq!(out.elided, 1);
}
