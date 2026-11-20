//! §7.3-Y2 type-driven escape elision (`lintel analyze` path): the analyze
//! pass resolves child-expression types through the `PHPStan` bridge, and the
//! lowering drops the `\Lintel\c()` wrapper ONLY where the type is provably
//! `int|float|bool|numeric-string|literal`. These goldens pin the emitted
//! PHP per proven kind, the conservative fallbacks, the reconciliation with
//! the v0.1 folds, and the elision-rate stats (`docs/type-elision.md`).

use lintel_core::{compile, compile_analyzed, ChildHint, ChildKind, Options, Target, TypeHints};

/// `function Page(int $id)` + `{$id}` on source line 3, the hint keyed by
/// the ABSOLUTE source line (the artifact is line-preserved, so the
/// bridge's per-line report and the source agree 1:1).
fn int_page_src() -> &'static str {
    "<?php\nfunction Page(int $id) {\n  return <p>{$id}</p>;\n}\n"
}

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

#[test]
fn int_typed_child_elides_the_helper() {
    let out = compile_analyzed(
        int_page_src(),
        &Options::default(),
        &hints(vec![hint(3, "$id", ChildKind::Int)]),
        lintel_core::builtin_rules(),
    )
    .expect("compiles");
    assert_eq!(
        out.output.php,
        "<?php\nfunction Page(int $id) {\n  return new \\Lintel\\Html('<p>' . $id . '</p>');\n}\n",
        "the proven-int child renders as the raw expression, no `c()`",
    );
    assert_eq!(out.elided, 1);
}

#[test]
fn float_typed_child_elides_the_helper() {
    let src = "<?php\nfunction Price($v) {\n  return <span>{$v}</span>;\n}\n";
    let out = compile_analyzed(
        src,
        &Options::default(),
        &hints(vec![hint(3, "$v", ChildKind::Float)]),
        lintel_core::builtin_rules(),
    )
    .expect("compiles");
    assert_eq!(
        out.output.php,
        "<?php\nfunction Price($v) {\n  return new \\Lintel\\Html('<span>' . $v . '</span>');\n}\n",
    );
}

#[test]
fn numeric_string_typed_child_elides_the_helper() {
    // `$code` is untyped (the scanner proves only `string`, so the stub
    // declares it mixed); the analyze pass proved `numeric-string` —
    // `is_numeric` strings carry none of `<>&"'`, so the raw expression
    // renders byte-identical to the escaped helper call.
    let src = "<?php\nfunction Row($code) {\n  return <td>{$code}</td>;\n}\n";
    let out = compile_analyzed(
        src,
        &Options::default(),
        &hints(vec![hint(3, "$code", ChildKind::NumericString)]),
        lintel_core::builtin_rules(),
    )
    .expect("compiles");
    assert_eq!(
        out.output.php,
        "<?php\nfunction Row($code) {\n  return new \\Lintel\\Html('<td>' . $code . '</td>');\n}\n",
    );
}

#[test]
fn literal_typed_child_inlines_the_escaped_value() {
    // A constant-valued expression folds like the v0.1 single-quoted fold:
    // the known value, escaped — byte-identical to what `c()` renders for
    // that value, and attacker-uncontrollable (it is fixed at analysis
    // time, baked into the artifact).
    let src = "<?php\nreturn <b>{MODE}</b>;\n";
    let out = compile_analyzed(
        src,
        &Options::default(),
        &hints(vec![hint(2, "MODE", ChildKind::Literal("row".into()))]),
        lintel_core::builtin_rules(),
    )
    .expect("compiles");
    assert_eq!(
        out.output.php,
        "<?php\nreturn new \\Lintel\\Html('<b>row</b>');\n"
    );
}

#[test]
fn literal_value_with_markup_inlines_escaped() {
    let src = "<?php\nreturn <b>{MODE}</b>;\n";
    let out = compile_analyzed(
        src,
        &Options::default(),
        &hints(vec![hint(2, "MODE", ChildKind::Literal("<i>&x".into()))]),
        lintel_core::builtin_rules(),
    )
    .expect("compiles");
    assert!(
        out.output.php.contains("&lt;i&gt;&amp;x"),
        "{}",
        out.output.php
    );
    assert!(
        !out.output.php.contains("<i>"),
        "no raw markup reaches the output"
    );
}

#[test]
fn unproven_children_keep_the_helper() {
    // A `string`-typed hint is not elidable (§7.3: only int|float|bool|
    // numeric-string|literal), and no hint at all is the default — both
    // keep `\Lintel\c()`.
    let src = "<?php\nfunction Row($label) {\n  return <p>{$label}</p>;\n}\n";
    let no_hints = compile_analyzed(
        src,
        &Options::default(),
        &TypeHints::default(),
        lintel_core::builtin_rules(),
    )
    .expect("compiles");
    assert!(
        no_hints.output.php.contains("\\Lintel\\c($label)"),
        "{}",
        no_hints.output.php
    );
    // A hint keyed to the WRONG line must not elide either (absolute-line
    // pinning): the child sits on line 3, the stale hint names line 2.
    let stale = compile_analyzed(
        src,
        &Options::default(),
        &hints(vec![hint(2, "$label", ChildKind::Int)]),
        lintel_core::builtin_rules(),
    )
    .expect("compiles");
    assert!(
        stale.output.php.contains("\\Lintel\\c($label)"),
        "{}",
        stale.output.php
    );
}

#[test]
fn hints_do_not_override_the_v01_folds() {
    // Reconciliation (single source of truth): the typed-parameter
    // direct-escape and the `(int)`/`count()` folds keep their exact
    // behavior under the analyze pass — the hints extend, never replace.
    let src = "<?php\nfunction Row(int $n, string $name) {\n  return <p>{(int)$n} {$name} {count($n)}</p>;\n}\n";
    let entries = vec![
        hint(3, "(int)$n", ChildKind::Int),
        hint(3, "$name", ChildKind::NumericString),
        hint(3, "count($n)", ChildKind::Int),
    ];
    let out = compile_analyzed(
        src,
        &Options::default(),
        &hints(entries),
        lintel_core::builtin_rules(),
    )
    .expect("compiles");
    assert!(
        out.output.php.contains("\\htmlspecialchars($name"),
        "the string param still folds: {}",
        out.output.php
    );
    assert!(
        out.output.php.contains("' . (int)$n . '"),
        "the cast still folds raw: {}",
        out.output.php
    );
    assert!(
        out.output.php.contains("' . count($n) . '"),
        "count() still folds raw: {}",
        out.output.php
    );
    assert!(
        !out.output.php.contains("\\Lintel\\c("),
        "{}",
        out.output.php
    );
}

#[test]
fn attribute_expressions_keep_their_helpers() {
    // Only child expressions elide (`docs/escape-elision.md` rule 5): the
    // URL helper's scheme allowlist is a context contract, not just
    // escaping, so an int hint on the same line never touches `href`.
    let src = "<?php\nfunction Page(int $id) {\n  return <a href={$id}>x</a>;\n}\n";
    let out = compile_analyzed(
        src,
        &Options::default(),
        &hints(vec![hint(3, "$id", ChildKind::Int)]),
        lintel_core::builtin_rules(),
    )
    .expect("compiles");
    assert!(
        out.output.php.contains("\\Lintel\\u('href', $id)"),
        "{}",
        out.output.php
    );
}

#[test]
fn stream_target_elides_too() {
    let out = compile_analyzed(
        int_page_src(),
        &Options {
            target: Target::Stream,
        },
        &hints(vec![hint(3, "$id", ChildKind::Int)]),
        lintel_core::builtin_rules(),
    )
    .expect("compiles");
    assert!(out.output.php.contains("yield $id;"), "{}", out.output.php);
    assert!(
        !out.output.php.contains("\\Lintel\\c($id)"),
        "{}",
        out.output.php
    );
}

#[test]
fn empty_hints_compile_byte_identical_to_compile() {
    for target in [Target::String, Target::Stream, Target::Rfc] {
        let src = int_page_src();
        let plain = compile(src, &Options { target }).expect("compiles");
        let analyzed = compile_analyzed(
            src,
            &Options { target },
            &TypeHints::default(),
            lintel_core::builtin_rules(),
        )
        .expect("compiles");
        assert_eq!(
            plain.php, analyzed.output.php,
            "target {target:?}: no hints, no change"
        );
    }
}

#[test]
fn conflicting_hints_for_one_child_elide_nothing() {
    // Two entries naming the same child with different proven kinds mean
    // the analysis is ambiguous — the conservative verdict keeps the
    // helper (the types file is untrusted input).
    let out = compile_analyzed(
        int_page_src(),
        &Options::default(),
        &hints(vec![
            hint(3, "$id", ChildKind::Int),
            hint(3, "$id", ChildKind::Float),
        ]),
        lintel_core::builtin_rules(),
    )
    .expect("compiles");
    assert!(
        out.output.php.contains("\\Lintel\\c($id)"),
        "{}",
        out.output.php
    );
}

#[test]
fn literal_with_line_count_mismatch_keeps_the_helper() {
    // Inlining the value must preserve line numbers: a multi-line
    // expression whose value is single-line stays wrapped.
    let src = "<?php\nreturn <b>{MODE\n}</b>;\n";
    let out = compile_analyzed(
        src,
        &Options::default(),
        &hints(vec![hint(2, "MODE", ChildKind::Literal("row".into()))]),
        lintel_core::builtin_rules(),
    )
    .expect("compiles");
    assert!(
        out.output.php.contains("\\Lintel\\c(MODE"),
        "{}",
        out.output.php
    );
    assert_eq!(out.output.php.lines().count(), src.lines().count());
}

#[test]
fn elision_stats_report_the_rate() {
    // Two dynamic children in one fragment root: one proven int (elided),
    // one unproven (kept). (Two sibling roots at statement level are not
    // a supported shape — see the backlog note in `docs/type-elision.md`.)
    let src = "<?php\nfunction Page(int $id, $label) {\n  return <><p>{$id}</p><span>{$label}</span></>;\n}\n";
    let out = compile_analyzed(
        src,
        &Options::default(),
        &hints(vec![hint(3, "$id", ChildKind::Int)]),
        lintel_core::builtin_rules(),
    )
    .expect("compiles");
    assert_eq!(out.dynamic_children, 2);
    assert_eq!(out.considered, 2);
    assert_eq!(out.elided, 1);
    assert_eq!(out.elision_rate(), Some(0.5));
}

#[test]
fn line_preservation_holds_under_elision() {
    let src = "<?php\nfunction Page(int $id) {\n  return\n    <p>\n      {$id}\n    </p>;\n}\n";
    let out = compile_analyzed(
        src,
        &Options::default(),
        &hints(vec![hint(5, "$id", ChildKind::Int)]),
        lintel_core::builtin_rules(),
    )
    .expect("compiles");
    assert_eq!(out.output.php.lines().count(), src.lines().count());
    // The separator rides after the newline (lines start with ` . `), so
    // the elided child surfaces as its own ` . $id` segment.
    assert!(out.output.php.contains(" . $id\n"), "{}", out.output.php);
    assert!(!out.output.php.contains("\\Lintel\\c($id)"));
}

#[test]
fn compile_errors_are_unchanged_under_analyze() {
    let src = "<?php\nreturn <p><span></p>;\n";
    let plain = compile(src, &Options::default());
    let analyzed = compile_analyzed(
        src,
        &Options::default(),
        &hints(vec![hint(2, "x", ChildKind::Int)]),
        lintel_core::builtin_rules(),
    );
    assert!(plain.is_err());
    assert!(analyzed.is_err());
}
