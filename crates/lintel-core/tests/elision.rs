//! The escape-elision contract (architecture doc §7.3 "escape elision
//! (honest version)", v0.1 subset): a dynamic child `{expr}` whose
//! expression is **provably** a non-string scalar at compile time folds to
//! the raw PHP expression instead of the `\Lintel\c()` escape helper —
//! `(int)`/`(integer)`/`(float)`/`(double)` casts spanning the whole
//! expression, and `count(...)` calls. An int/float cannot carry markup, so
//! the helper call is pure overhead (doc §9.1: "everything folds that can
//! fold").
//!
//! Everything else stays wrapped — conservative by design: a cast must span
//! the *whole* child expression (a top-level binary operator would bind the
//! cast's result as its left operand), unknown functions stay wrapped even
//! when they happen to return ints, and `(string)` casts are strings and
//! never elide. The exact contract is recorded in
//! `docs/escape-elision.md`.
//!
//! Attribute values are untouched: the `\Lintel\a/u/cls/sty` helpers carry
//! bool/null semantics and per-context routing that a scalar fold would
//! drop, and the `rfc` target was already helper-free.

use lintel_core::{compile, Options, Target};

fn php_of(src: &str, target: Target) -> String {
    compile(src, &Options { target })
        .expect("elision fixtures compile cleanly")
        .php
}

#[test]
fn int_cast_child_folds_without_the_c_helper() {
    let out = php_of("<?php\nreturn <b>N{(int)$n}</b>;\n", Target::String);
    assert_eq!(
        out,
        "<?php\nreturn new \\Lintel\\Html('<b>N' . (int)$n . '</b>');\n"
    );
}

#[test]
fn float_cast_and_count_children_elide_too() {
    let out = php_of(
        "<?php\nreturn <p>{(float)$rate}/{count($rows)} rows</p>;\n",
        Target::String,
    );
    assert!(out.contains("(float)$rate . "), "{out}");
    assert!(
        out.contains("count($rows) . ' rows</p>'"),
        "count() elides: {out}"
    );
    assert!(!out.contains("\\Lintel\\c("), "{out}");
}

#[test]
fn integer_and_double_aliases_elide_like_their_casts() {
    let out = php_of(
        "<?php\nreturn <b>{(integer)$a}{(double)$b}</b>;\n",
        Target::String,
    );
    assert!(out.contains("(integer)$a . (double)$b"), "{out}");
    assert!(!out.contains("\\Lintel\\c((integer)"), "{out}");
    assert!(!out.contains("\\Lintel\\c((double)"), "{out}");
}

#[test]
fn case_insensitive_casts_elide_as_written() {
    let out = php_of("<?php\nreturn <b>{(INT)$n}</b>;\n", Target::String);
    assert!(out.contains(" . (INT)$n . "), "{out}");
    assert!(!out.contains("\\Lintel\\c((INT)"), "{out}");
}

#[test]
fn cast_must_span_the_whole_child_expression() {
    // A top-level binary operator binds the cast result as its left operand
    // (`(int)$n . $suffix` is a string concat), so the wrapper stays.
    let out = php_of(
        "<?php\nreturn <b>{(int)$n . $suffix}</b>;\n",
        Target::String,
    );
    assert!(out.contains("\\Lintel\\c((int)$n . $suffix)"), "{out}");
}

#[test]
fn count_call_must_exhaust_the_child_expression() {
    let out = php_of(
        "<?php\nreturn <b>{count($rows) . ' rows'}</b>;\n",
        Target::String,
    );
    assert!(out.contains("\\Lintel\\c(count($rows) . ' rows')"), "{out}");
}

#[test]
fn unknown_functions_and_string_casts_stay_wrapped() {
    let out = php_of(
        "<?php\nreturn <b>{strlen($s)}{(string)$s}</b>;\n",
        Target::String,
    );
    assert!(out.contains("\\Lintel\\c(strlen($s))"), "{out}");
    assert!(out.contains("\\Lintel\\c((string)$s)"), "{out}");
}

#[test]
fn cast_operand_may_be_a_variable_chain_or_group() {
    let out = php_of(
        "<?php\nreturn <b>{(int)$row->count}{(int)($a . $b)}{(int)PHP_INT_MAX}</b>;\n",
        Target::String,
    );
    assert!(out.contains("(int)$row->count"), "{out}");
    assert!(out.contains("(int)($a . $b)"), "{out}");
    assert!(out.contains("(int)PHP_INT_MAX"), "{out}");
    assert!(!out.contains("\\Lintel\\c((int)"), "{out}");
}

#[test]
fn cast_of_cast_elides_through_the_group_or_the_alias() {
    let out = php_of(
        "<?php\nreturn <b>{(int)((float)$x)}{(int)(float)$x}</b>;\n",
        Target::String,
    );
    assert!(out.contains("(int)((float)$x)"), "{out}");
    assert!(out.contains("(int)(float)$x"), "{out}");
    assert!(!out.contains("\\Lintel\\c((int)"), "{out}");
}

#[test]
fn plain_expressions_keep_the_helper() {
    let out = php_of(
        "<?php\nreturn <b>{$n}{$arr[0]}{$obj->prop}</b>;\n",
        Target::String,
    );
    assert!(out.contains("\\Lintel\\c($n)"), "{out}");
    assert!(out.contains("\\Lintel\\c($arr[0])"), "{out}");
    assert!(out.contains("\\Lintel\\c($obj->prop)"), "{out}");
}

#[test]
fn stream_target_elides_in_its_yield_chunks() {
    let out = php_of("<?php\nreturn <b>N{(int)$n}</b>;\n", Target::Stream);
    assert!(out.contains("yield (int)$n;"), "{out}");
    assert!(!out.contains("yield \\Lintel\\c((int)$n);"), "{out}");
}

#[test]
fn rfc_target_stays_helper_free() {
    let out = php_of("<?php\nreturn <b>N{(int)$n}</b>;\n", Target::Rfc);
    assert!(out.contains("['N', (int)$n]"), "{out}");
    assert!(!out.contains("\\Lintel\\c("), "{out}");
}

#[test]
fn attribute_values_keep_their_helpers() {
    let out = php_of(
        "<?php\nreturn <b data-x={(int)$n} class={$c} />;\n",
        Target::String,
    );
    assert!(out.contains("\\Lintel\\a('data-x', (int)$n)"), "{out}");
    assert!(out.contains("\\Lintel\\cls($c)"), "{out}");
}

#[test]
fn elided_children_preserve_line_numbers() {
    let src = "<?php\nfunction Page(int $n): Html {\n    return <div>\n        <span>{(int)$n}</span>\n    </div>;\n}\n";
    let out = php_of(src, Target::String);
    assert_eq!(src.lines().count(), out.lines().count());
    // The elided expression lands on the source line of its `{…}` child.
    let elided_line = out.lines().nth(3).expect("line 4 exists");
    assert!(elided_line.contains("(int)$n"), "{out}");
}
