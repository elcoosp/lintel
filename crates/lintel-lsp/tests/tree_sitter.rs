//! The Phase 11 tree-sitter-lane acceptance (architecture doc §17.1 Q3:
//! "tree-sitter grammar", the Phase 11 LSP-parsing card): a monorepo
//! fixture drives the real server over mock-client sessions and asserts
//! that
//!
//! 1. **definition resolves across files through the tree-sitter lane** —
//!    a `<Component />` tag in an OPEN buffer jumps to a declaration in a
//!    CLOSED file the server only knows through the persistent walk;
//! 2. **rename works on tree-sitter-parsed components** — the
//!    `WorkspaceEdit` spans both files (the closed declaration and the
//!    open invocation), through the merged declaration view;
//! 3. **the fallback keeps malformed input alive** — a file whose
//!    declaration header is beyond tree-sitter recovery (`(***` params)
//!    still declares its component: the lane falls back to the
//!    lintel-core scanner, the cache row records `scanner-fallback`, and
//!    rename/definition answer as usual.
//!
//! The fixture mirrors a monorepo (`packages/*/src` with an `lsp.include`
//! config): the declaration and the malformed legacy file stay closed
//! for the whole session — every fact about them arrives through the
//! index, never through an open buffer.

use std::fs;
use std::io::Cursor;
use std::path::PathBuf;

const BADGE: &str = concat!(
    "<?php\n\nnamespace Ui;\n\nuse Lintel\\Html;\n\n",
    "function Badge(string $label): Html\n{\n",
    "    return <b class=\"pill\">{$label}</b>;\n}\n"
);

const PAGE: &str = concat!(
    "<?php\n\nnamespace App;\n\nuse Lintel\\Html;\n\nuse Ui\\Badge;\n\n",
    "function Page(string $title): Html\n{\n",
    "    return <html><body>\n",
    "        <Badge label={$title} />\n",
    "        <Broken note=\"legacy\" />\n",
    "    </body></html>;\n}\n"
);

/// The malformed legacy source: the declaration header (`(***`) is
/// beyond tree-sitter error recovery — the lane sees no usable
/// declaration structure and falls back to the scanner, which tolerates
/// the malformed parameter list.
const BROKEN: &str = concat!(
    "<?php\n\nnamespace Legacy;\n\nuse Lintel\\Html;\n\n",
    "function Broken(***): Html\n{\n",
    "    return <i>broken but declared</i>;\n}\n"
);

const CONFIG: &str = r#"{"lsp": {"include": ["packages/*/src/**/*.lintel.php"]}}"#;

/// A fresh monorepo fixture: `packages/ui/src/Badge.lintel.php` (closed),
/// `packages/legacy/src/Broken.lintel.php` (closed, malformed), and
/// `packages/app/src/Page.lintel.php` (the consumer the client opens).
fn monorepo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lintel-lsp-ts-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("packages/ui/src")).expect("dirs");
    fs::create_dir_all(dir.join("packages/app/src")).expect("dirs");
    fs::create_dir_all(dir.join("packages/legacy/src")).expect("dirs");
    fs::write(dir.join("packages/ui/src/Badge.lintel.php"), BADGE).expect("writes Badge");
    fs::write(dir.join("packages/app/src/Page.lintel.php"), PAGE).expect("writes Page");
    fs::write(dir.join("packages/legacy/src/Broken.lintel.php"), BROKEN).expect("writes Broken");
    fs::write(dir.join("lintel.json"), CONFIG).expect("writes config");
    dir
}

/// The `file://` URI of a fixture path (minimal encoding, like the
/// server's own `path_to_uri`).
fn uri_of(path: &std::path::Path) -> String {
    format!("file://{}", path.to_string_lossy())
}

/// Frame one JSON value the way a client does.
fn framed(body: &str) -> Vec<u8> {
    format!("Content-Length: {}\r\n\r\n{body}", body.len()).into_bytes()
}

/// Run the server over the given client messages, returning everything it
/// wrote.
fn session(messages: &[String]) -> String {
    let mut input: Vec<u8> = Vec::new();
    for message in messages {
        input.extend_from_slice(&framed(message));
    }
    let mut output: Vec<u8> = Vec::new();
    let code = lintel_lsp::run(&mut Cursor::new(input), &mut output);
    assert_eq!(code, 0, "the session ends cleanly on exit");
    String::from_utf8(output).expect("server output is utf8")
}

/// The JSON bodies of every framed server message.
fn bodies(session: &str) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    for chunk in session.split("Content-Length: ") {
        if chunk.is_empty() {
            continue;
        }
        let Some((_header, body)) = chunk.split_once("\r\n\r\n") else {
            continue;
        };
        out.push(serde_json::from_str(body.trim()).expect("valid JSON body"));
    }
    out
}

/// The response body for the request with the given id.
fn response_for(session: &str, id: i64) -> serde_json::Value {
    bodies(session)
        .into_iter()
        .find(|body| body.get("id") == Some(&serde_json::json!(id)))
        .unwrap_or_else(|| panic!("a response for id {id}"))
}

/// The 0-based UTF-16 position of `needle`'s first byte in `text`.
fn position_of(text: &str, needle: &str) -> serde_json::Value {
    let offset = text.find(needle).expect("needle in text");
    let (line, column) = text[..offset]
        .chars()
        .fold((0u32, 0u32), |(line, column), c| {
            if c == '\n' {
                (line + 1, 0)
            } else {
                (line, column + 1)
            }
        });
    serde_json::json!({"line": line, "character": column})
}

/// The client's message for `method` with `params`.
fn request(id: i64, method: &str, params: &serde_json::Value) -> String {
    let id_value = serde_json::Value::from(id);
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id_value,
        "method": method,
        "params": params,
    })
    .to_string()
}

/// `initialize` + `didOpen Page` — the fixture's only open buffer.
fn open_page(root: &std::path::Path) -> Vec<String> {
    let page_uri = uri_of(&root.join("packages/app/src/Page.lintel.php"));
    vec![
        request(
            0,
            "initialize",
            &serde_json::json!({
                "rootUri": uri_of(root),
                "capabilities": {},
            }),
        ),
        serde_json::json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": page_uri,
                    "languageId": "php",
                    "version": 1,
                    "text": PAGE,
                },
            },
        })
        .to_string(),
    ]
}

#[test]
fn definition_resolves_across_files_through_the_tree_sitter_lane() {
    let root = monorepo("definition");
    let badge_path = root.join("packages/ui/src/Badge.lintel.php");
    let badge_uri = uri_of(&badge_path);
    let page_uri = uri_of(&root.join("packages/app/src/Page.lintel.php"));

    let mut messages = open_page(&root);
    messages.push(request(
        1,
        "textDocument/definition",
        &serde_json::json!({
            "textDocument": {"uri": page_uri},
            "position": position_of(PAGE, "<Badge"),
        }),
    ));
    let response = response_for(&session(&messages), 1);

    let location = response
        .get("result")
        .expect("definition resolves")
        .as_object()
        .expect("a Location object");
    let target = location
        .get("uri")
        .and_then(serde_json::Value::as_str)
        .expect("uri");
    assert_eq!(
        target, badge_uri,
        "the tag in the open buffer jumps to the CLOSED declaration file"
    );
    // The range lands on the declaration identifier: `function Badge` in
    // the fixture's line table.
    let expected = position_of(BADGE, "Badge");
    let range = location.get("range").expect("a range");
    assert_eq!(range.get("start"), Some(&expected), "the identifier span");
}

#[test]
fn rename_edits_the_tree_sitter_parsed_declaration_and_the_invocation() {
    let root = monorepo("rename");
    let badge_path = root.join("packages/ui/src/Badge.lintel.php");
    let badge_uri = uri_of(&badge_path);
    let page_uri = uri_of(&root.join("packages/app/src/Page.lintel.php"));

    let mut messages = open_page(&root);
    messages.push(request(
        1,
        "textDocument/rename",
        &serde_json::json!({
            "textDocument": {"uri": page_uri},
            "position": position_of(PAGE, "<Badge"),
            "newName": "Label",
        }),
    ));
    let response = response_for(&session(&messages), 1);
    let changes = response
        .get("result")
        .expect("rename resolves")
        .get("changes")
        .expect("per-URI edits")
        .as_object()
        .expect("an edit map");

    // The closed declaration file: its declaration identifier is edited —
    // the component was parsed by the tree-sitter lane (the file never
    // opened). One edit: the identifier.
    let badge_edits = changes
        .get(&badge_uri)
        .expect("the closed declaration file is edited")
        .as_array()
        .expect("edits");
    assert_eq!(badge_edits.len(), 1, "the declaration identifier edit");
    // The open invocation file: the self-closing tag is edited — one edit.
    let page_edits = changes
        .get(&page_uri)
        .expect("the invocation file is edited")
        .as_array()
        .expect("edits");
    assert_eq!(page_edits.len(), 1, "the invocation tag edit");
}

#[test]
fn references_list_the_tree_sitter_parsed_declaration_across_files() {
    let root = monorepo("references");
    let badge_uri = uri_of(&root.join("packages/ui/src/Badge.lintel.php"));
    let page_uri = uri_of(&root.join("packages/app/src/Page.lintel.php"));

    let mut messages = open_page(&root);
    messages.push(request(
        1,
        "textDocument/references",
        &serde_json::json!({
            "textDocument": {"uri": page_uri},
            "position": position_of(PAGE, "<Badge"),
            "context": {"includeDeclaration": true},
        }),
    ));
    let response = response_for(&session(&messages), 1);
    let locations = response
        .get("result")
        .expect("references resolve")
        .as_array()
        .expect("a location list");
    let uris: Vec<&str> = locations
        .iter()
        .filter_map(|location| location.get("uri").and_then(serde_json::Value::as_str))
        .collect();
    assert!(
        uris.contains(&badge_uri.as_str()),
        "the closed declaration file lists: {uris:?}"
    );
    assert!(
        uris.contains(&page_uri.as_str()),
        "the open invocation file lists: {uris:?}"
    );
}

#[test]
fn a_malformed_file_falls_back_to_the_scanner_and_stays_declared() {
    let root = monorepo("fallback");
    let broken_uri = uri_of(&root.join("packages/legacy/src/Broken.lintel.php"));
    let page_uri = uri_of(&root.join("packages/app/src/Page.lintel.php"));

    // Rename `<Broken />` in the open buffer: the component is declared
    // ONLY through the fallback lane (the closed malformed file), and
    // the rename still spans both files.
    let mut messages = open_page(&root);
    messages.push(request(
        1,
        "textDocument/rename",
        &serde_json::json!({
            "textDocument": {"uri": page_uri},
            "position": position_of(PAGE, "<Broken"),
            "newName": "Fixed",
        }),
    ));
    let response = response_for(&session(&messages), 1);
    let changes = response
        .get("result")
        .expect("the fallback lane declares Broken")
        .get("changes")
        .expect("per-URI edits")
        .as_object()
        .expect("an edit map");
    assert!(
        changes.get(&broken_uri).is_some(),
        "the closed malformed file's declaration is edited: {changes:?}"
    );

    // Definition from the open buffer lands on the malformed file's
    // declaration identifier.
    let mut messages = open_page(&root);
    messages.push(request(
        1,
        "textDocument/definition",
        &serde_json::json!({
            "textDocument": {"uri": page_uri},
            "position": position_of(PAGE, "<Broken"),
        }),
    ));
    let response = response_for(&session(&messages), 1);
    let target = response
        .get("result")
        .expect("definition resolves through the fallback")
        .get("uri")
        .and_then(serde_json::Value::as_str)
        .expect("a uri");
    assert_eq!(target, broken_uri);

    // The cache row records the lane contract.
    let cache =
        fs::read_to_string(root.join(".lintel-cache/lsp-index.json")).expect("the index persists");
    assert!(
        cache.contains("\"lane\":\"scanner-fallback\""),
        "the malformed file rides the fallback lane"
    );
    assert!(
        cache.contains("\"lane\":\"tree-sitter\""),
        "the healthy files ride the tree-sitter lane"
    );
}

#[test]
fn the_tree_sitter_lane_survives_a_session_boundary_through_the_cache() {
    let root = monorepo("session");
    let badge_uri = uri_of(&root.join("packages/ui/src/Badge.lintel.php"));
    let page_uri = uri_of(&root.join("packages/app/src/Page.lintel.php"));

    // Session 1: walks, parses, persists.
    let mut messages = open_page(&root);
    messages.push(request(
        1,
        "textDocument/definition",
        &serde_json::json!({
            "textDocument": {"uri": page_uri},
            "position": position_of(PAGE, "<Badge"),
        }),
    ));
    let _ = response_for(&session(&messages), 1);
    assert!(root.join(".lintel-cache/lsp-index.json").is_file());

    // Session 2: the cache round-trips — definition still resolves to the
    // closed declaration file without a re-walk.
    let mut messages = open_page(&root);
    messages.push(request(
        1,
        "textDocument/definition",
        &serde_json::json!({
            "textDocument": {"uri": page_uri},
            "position": position_of(PAGE, "<Badge"),
        }),
    ));
    let response = response_for(&session(&messages), 1);
    let target = response
        .get("result")
        .expect("definition resolves across the session boundary")
        .get("uri")
        .and_then(serde_json::Value::as_str)
        .expect("a uri");
    assert_eq!(target, badge_uri);
}
