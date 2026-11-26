//! The cross-file signature-aggregation contracts
//! (`docs/cross-file-signatures.md`): [`ScanContext::merged_with`] unions
//! two files' declaration tables so a file's component invocations can be
//! validated against components declared in *other* files — the same-file
//! boundary `LNL0203`/`LNL0204`/`LNL0205` documented so far. Suppression
//! state deliberately does not merge: byte windows and the file-level flag
//! are per-source facts.

use lintl_core::scan::{scan, ScanContext};
use lintl_core::validate::validate_with;
use lintl_core::{builtin_rules, Code};

/// The foreign file: declares `Card` with one `string` parameter.
const FOREIGN_CARD: &str =
    "<?php\nfunction Card(string $title): Html {\n    return <b>{$title}</b>;\n}\n";

/// The base file: invokes `Card` with one matching and one unknown prop.
const BASE_CARD: &str = "<?php\nreturn <Card title=\"hi\" oops=\"bad\" />;\n";

#[test]
fn cross_file_prop_check_fires_only_with_the_merged_context() {
    let foreign = scan(FOREIGN_CARD);
    let base = scan(BASE_CARD);
    assert!(foreign.diagnostics.is_empty() && base.diagnostics.is_empty());

    // Same-file boundary: the base file declares no `Card`, so no signature
    // check fires.
    let plain = validate_with(&base.roots, &base.context, builtin_rules());
    assert!(
        plain.is_empty(),
        "without the merge the invocation is unchecked: {plain:?}"
    );

    // Merged: the foreign signature resolves, and the unknown prop reports.
    let merged = base.context.merged_with(&foreign.context);
    let found = validate_with(&base.roots, &merged, builtin_rules());
    assert_eq!(found.len(), 1, "one LNL0203: {found:?}");
    assert_eq!(found[0].code, Code::UnknownProp);
    assert!(
        found[0]
            .message
            .contains("`Card` declares no parameter `$oops`"),
        "{}",
        found[0].message
    );
    assert!(
        found[0]
            .help
            .as_deref()
            .is_some_and(|help| help.contains("$title")),
        "the help names the foreign parameter list: {:?}",
        found[0].help
    );
}

#[test]
fn cross_file_required_and_children_checks_resolve() {
    // `Badge(string $label)` declared abroad, invoked without it: LNL0205.
    let foreign =
        scan("<?php\nfunction Badge(string $label): Html {\n    return <em>{$label}</em>;\n}\n");
    let base = scan("<?php\nreturn <Badge />;\n");
    let merged = base.context.merged_with(&foreign.context);
    let found = validate_with(&base.roots, &merged, builtin_rules());
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].code, Code::MissingRequiredProp);

    // A foreign `children` parameter accepts JSX children (no LNL0204)…
    let accepting = scan(
        "<?php\nfunction Wrap($children = null): Html {\n    return <div>{$children}</div>;\n}\n",
    );
    let invoking = scan("<?php\nreturn <Wrap>text</Wrap>;\n");
    let merged_accepting = invoking.context.merged_with(&accepting.context);
    assert!(
        validate_with(&invoking.roots, &merged_accepting, builtin_rules()).is_empty(),
        "a foreign `children` parameter accepts JSX children"
    );

    // …and a foreign signature without one rejects them (LNL0204).
    let rejecting = scan("<?php\nfunction Wrap(): Html {\n    return <div />;\n}\n");
    let merged_rejecting = invoking.context.merged_with(&rejecting.context);
    let found = validate_with(&invoking.roots, &merged_rejecting, builtin_rules());
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].code, Code::UnexpectedChildren);
}

#[test]
fn declared_functions_union_across_files() {
    let foreign = scan("<?php\nfunction header(): void {}\n");
    let base = scan("<?php\nreturn <Header />;\n");
    let plain = validate_with(&base.roots, &base.context, builtin_rules());
    assert_eq!(plain.len(), 1, "`header()` is a builtin shadow: {plain:?}");
    assert_eq!(plain[0].code, Code::BuiltinShadow);

    let merged = base.context.merged_with(&foreign.context);
    assert!(
        validate_with(&base.roots, &merged, builtin_rules()).is_empty(),
        "the foreign `header` declaration resolves the shadow"
    );
    assert!(merged.declares_function("header"));
}

#[test]
fn in_file_declarations_win_the_collision() {
    // Both files declare `Card` with different parameter lists; the context
    // the validation runs against (the base) keeps its own signature.
    let base = scan("<?php\nfunction Card(string $title): Html {\n    return <b>{$title}</b>;\n}\nreturn <Card title=\"ok\" />;\n");
    let foreign =
        scan("<?php\nfunction Card(int $count): Html {\n    return <em>{$count}</em>;\n}\n");
    let merged = base.context.merged_with(&foreign.context);
    let params: Vec<&str> = merged
        .signature_of("Card")
        .expect("the collision resolves")
        .params
        .iter()
        .map(String::as_str)
        .collect();
    assert_eq!(params, vec!["title"], "the base's signature wins");
    assert!(
        validate_with(&base.roots, &merged, builtin_rules()).is_empty(),
        "`title` validates against the base's own signature"
    );

    // Reversed, the foreign file is the base and wins instead.
    let reversed = foreign.context.merged_with(&base.context);
    let params: Vec<&str> = reversed
        .signature_of("Card")
        .expect("the collision resolves")
        .params
        .iter()
        .map(String::as_str)
        .collect();
    assert_eq!(params, vec!["count"], "merge is documented non-commutative");
}

#[test]
fn merged_declaration_order_is_base_then_foreign() {
    let base = scan("<?php\nfunction Page(): Html {\n    return <div />;\n}\n");
    let foreign = scan(FOREIGN_CARD);
    let merged = base.context.merged_with(&foreign.context);
    let names: Vec<&str> = merged.declared_signatures().map(|(name, _)| name).collect();
    assert_eq!(names, vec!["Page", "Card"], "base first, foreign appended");
    let reversed = foreign.context.merged_with(&base.context);
    let names: Vec<&str> = reversed
        .declared_signatures()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(names, vec!["Card", "Page"]);
}

#[test]
fn suppression_state_does_not_merge() {
    // The foreign file silences itself; that must not silence the base.
    let foreign = scan("<?php\n// lintl-ignore-file\nfunction Card(string $title): Html {\n    return <b>{$title}</b>;\n}\n");
    assert!(foreign.context.file_suppressed(), "the directive latched");
    let base = scan(BASE_CARD);
    let merged = base.context.merged_with(&foreign.context);
    assert!(
        !merged.file_suppressed(),
        "the file-level flag is per-source"
    );
    let found = validate_with(&base.roots, &merged, builtin_rules());
    assert_eq!(
        found.len(),
        1,
        "the base's LNL0203 still reports: {found:?}"
    );
    assert_eq!(found[0].code, Code::UnknownProp);

    // Suppression windows are byte ranges of *their own* source; a merge
    // must not carry them into another file's coordinate space.
    let windowed = scan("<?php\n// lintl-ignore-next\nreturn <Header />;\n");
    assert_ne!(
        windowed.context.suppressions(),
        [],
        "the directive recorded its window"
    );
    let joined = base.context.merged_with(&windowed.context);
    assert!(
        joined.suppressions().is_empty(),
        "no foreign windows leak into the merged context"
    );
    assert!(!joined.file_suppressed());
}

#[test]
fn merging_leaves_both_contexts_unchanged() {
    let foreign = scan(FOREIGN_CARD);
    let base = scan(BASE_CARD);
    let foreign_suppressions = foreign.context.suppressions().len();
    let _merged = base.context.merged_with(&foreign.context);
    assert!(
        base.context.signature_of("Card").is_none(),
        "the base context gained nothing"
    );
    assert!(
        foreign.context.signature_of("Card").is_some(),
        "the foreign context keeps its own table"
    );
    assert_eq!(
        foreign.context.suppressions().len(),
        foreign_suppressions,
        "the foreign context keeps its own windows"
    );
}

#[test]
fn merging_an_empty_context_changes_nothing() {
    let base = scan(BASE_CARD);
    let merged = base.context.merged_with(&ScanContext::default());
    assert!(merged.declared_signatures().next().is_none());
    assert!(!merged.file_suppressed());
    assert_eq!(
        merged.suppressions(),
        [],
        "an empty foreign table carries no windows"
    );
    assert!(
        validate_with(&base.roots, &merged, builtin_rules()).is_empty(),
        "an empty foreign table validates nothing new"
    );
}
