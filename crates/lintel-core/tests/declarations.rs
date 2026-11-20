//! The [`Declaration`] go-to-definition seam (§17.1 Q3): the scanner
//! records where each declared function's name and parameter names sit in
//! the source — the byte offsets an LSP definition request resolves to —
//! from the same declaration walk that fills the signature table, so
//! definition facts cannot drift from the compiler's own signatures.

use lintel_core::scan::scan;

#[test]
fn declaration_records_name_and_parameter_offsets() {
    let src = concat!(
        "<?php\n\n",
        "function Card(string $title, ?Html $children = null): Html {\n",
        "    return <div>{$title}{$children}</div>;\n",
        "}\n"
    );
    let parsed = scan(src);
    let declaration = parsed
        .context
        .declaration_of("Card")
        .expect("the declared component carries a position record");
    // The declared name's offset: `Card` follows `function ` on line 1.
    let name_offset = src.find("function Card").expect("in source") + "function ".len();
    assert_eq!(declaration.offset, name_offset);
    assert_eq!(declaration.name, "Card");
    // The verbatim span between the parens.
    assert_eq!(
        declaration.params_src,
        "string $title, ?Html $children = null"
    );
    // Each parameter name's `$`, in declaration order — the spans a prop
    // definition request jumps to.
    let params: Vec<(String, usize)> = declaration.params.clone();
    assert_eq!(params.len(), 2);
    assert_eq!(params[0].0, "title");
    assert_eq!(params[0].1, src.find("$title").expect("in source"));
    assert_eq!(params[1].0, "children");
    assert_eq!(params[1].1, src.find("$children").expect("in source"));
    // The declaration table mirrors the signature table's lookup rules.
    assert_eq!(
        parsed.context.declaration_of("card"),
        parsed.context.declaration_of("CARD"),
        "lookups are case-insensitive like PHP function calls"
    );
    assert_eq!(
        parsed
            .context
            .declaration_of("\\App\\Card")
            .map(|declaration| declaration.name.as_str()),
        Some("Card"),
        "qualified lookups strip to the last path segment"
    );
}

#[test]
fn multiline_signatures_keep_absolute_offsets() {
    // A declaration written across lines: every offset stays absolute in
    // the file's coordinate space, so SourceMap maps it to editor
    // positions without re-scanning.
    let src = concat!(
        "<?php\n\n",
        "function Row(\n",
        "    string $label,\n",
        "    int $count,\n",
        "): Html {\n",
        "    return <td>{$label}{$count}</td>;\n",
        "}\n"
    );
    let parsed = scan(src);
    let declaration = parsed.context.declaration_of("Row").expect("declared");
    let expected_label = src.find("$label").expect("in source");
    let expected_count = src.find("$count").expect("in source");
    assert_eq!(declaration.params[0], ("label".to_string(), expected_label));
    assert_eq!(declaration.params[1], ("count".to_string(), expected_count));
    assert!(
        declaration.params_src.contains('$'),
        "the span is the verbatim text"
    );
}

#[test]
fn closures_and_imports_record_no_declaration() {
    let src = concat!(
        "<?php\n\n",
        "use function App\\helper;\n\n",
        "$fn = function (string $x): Html { return <p>{$x}</p>; };\n",
        "$arrow = fn(int $n) => <em>{$n}</em>;\n",
        "echo <div />;\n"
    );
    let parsed = scan(src);
    assert!(
        parsed.context.declaration_of("helper").is_none(),
        "`use function` imports carry no position"
    );
    assert!(
        parsed.context.declaration_of("fn").is_none(),
        "closures register no name to look up"
    );
}

#[test]
fn first_declaration_wins_and_merge_stays_same_file() {
    // Like the signature table, the first declaration of a name wins. And
    // like suppression state, position facts never merge: another file's
    // offsets point into *its* source, so a merged definition would jump
    // into the wrong file's coordinate space.
    let src =
        "<?php\nfunction A(): Html { return <br />; }\nfunction A(): Html { return <hr />; }\n";
    let parsed = scan(src);
    let first = src.find("function A").expect("in source") + "function ".len();
    let second = src.rfind("function A").expect("in source") + "function ".len();
    assert_ne!(first, second);
    let declaration = parsed.context.declaration_of("A").expect("declared");
    assert_eq!(declaration.offset, first, "the first declaration wins");

    let other = scan("<?php\nfunction B(): Html { return <b />; }\n").context;
    let merged = parsed.context.merged_with(&other);
    assert!(
        merged.declaration_of("B").is_none(),
        "declarations are per-source facts and never merge"
    );
}
