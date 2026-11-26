//! The `srcset` per-URL attribute routing contract (architecture doc §4.4
//! URL-attribute table, §7.1 escaping matrix): a dynamic `srcset={expr}`
//! attribute lowers to a dedicated `\Lintl\srcset()` helper call instead of
//! the generic `\Lintl\a()` — the runtime helper splits the candidate list
//! and scheme-checks each URL, which a plain attribute escape cannot do. The
//! exact contract is recorded in `docs/srcset-alignment.md`.
//!
//! Everything else is unchanged: static string values fold verbatim
//! (author-trusted, like every other static attribute), the `stream` target
//! shares the routing, the `rfc` target stays helper-free by contract, and
//! component attributes remain named arguments.

use lintl_core::{compile, Options, Target};

fn php_of(src: &str, target: Target) -> String {
    compile(src, &Options { target })
        .expect("srcset fixtures compile cleanly")
        .php
}

#[test]
fn dynamic_srcset_routes_through_the_srcset_helper() {
    let out = php_of(
        "<?php\nreturn <img src={$src} srcset={$set} />;\n",
        Target::String,
    );
    assert!(
        out.contains("\\Lintl\\srcset('srcset', $set)"),
        "srcset lowers to the dedicated helper: {out}"
    );
    assert!(
        !out.contains("\\Lintl\\a('srcset'"),
        "the generic attribute helper must not handle srcset: {out}"
    );
    // Control: the plain URL attribute keeps its own routing.
    assert!(out.contains("\\Lintl\\u('src', $src)"), "{out}");
}

#[test]
fn static_srcset_values_fold_verbatim() {
    // Author-trusted static strings fold at compile time, exactly like
    // static `href`/`src` values; no runtime helper is emitted.
    let out = php_of(
        "<?php\nreturn <img srcset=\"a.png 1x, b.png 2x\" />;\n",
        Target::String,
    );
    assert!(
        out.contains("srcset=\"a.png 1x, b.png 2x\""),
        "static value folds verbatim: {out}"
    );
    assert!(!out.contains("\\Lintl\\srcset"), "{out}");
}

#[test]
fn stream_target_routes_srcset_the_same_way() {
    let out = php_of("<?php\nreturn <img srcset={$set} />;\n", Target::Stream);
    assert!(
        out.contains("\\Lintl\\srcset('srcset', $set)"),
        "the stream target shares the attribute routing: {out}"
    );
}

#[test]
fn rfc_target_stays_helper_free() {
    let out = php_of("<?php\nreturn <img srcset={$set} />;\n", Target::Rfc);
    assert!(
        out.contains("'srcset' => $set"),
        "rfc attributes stay raw array entries: {out}"
    );
    assert!(!out.contains("\\Lintl\\"), "no runtime helpers: {out}");
}

#[test]
fn component_srcset_stays_a_named_argument() {
    // Helper routing is an HTML-attribute concern; component attributes are
    // named arguments and pass through untouched.
    let out = php_of("<?php\nreturn <Img srcset={$set} />;\n", Target::String);
    assert!(out.contains("srcset: $set"), "{out}");
    assert!(!out.contains("\\Lintl\\srcset"), "{out}");
}

#[test]
fn srcset_routing_ignores_name_case_but_keeps_the_written_name() {
    let out = php_of("<?php\nreturn <img SRCSET={$set} />;\n", Target::String);
    assert!(
        out.contains("\\Lintl\\srcset('SRCSET', $set)"),
        "routing is case-insensitive, the emitted name is as written: {out}"
    );
}

#[test]
fn plain_dynamic_attrs_keep_the_generic_helper() {
    let out = php_of("<?php\nreturn <img alt={$text} />;\n", Target::String);
    assert!(
        out.contains("\\Lintl\\a('alt', $text)"),
        "only srcset is re-routed: {out}"
    );
}

#[test]
fn srcset_preserves_line_count() {
    let src = "<?php\nreturn <img\n    src={$src}\n    srcset={$set}\n/>;\n";
    let out = php_of(src, Target::String);
    assert_eq!(
        out.lines().count(),
        src.lines().count(),
        "line preservation holds for the re-routed attribute: {out}"
    );
}
