//! The T-21 LSP foundation contracts: diagnostics publish on open with
//! the compiler's byte offsets mapped to 0-based UTF-16 positions, hover
//! over a `<ComponentName>` renders the `ScanContext` signature, unknown
//! requests answer method-not-found, and `shutdown`/`exit` end the
//! session. A mock client drives `lintl_lsp::run` over in-memory
//! buffers — the same framing a stdio editor client speaks.

use std::io::Cursor;

/// Frame one JSON value the way a client does.
fn framed(body: &str) -> Vec<u8> {
    format!("Content-Length: {}\r\n\r\n{body}", body.len()).into_bytes()
}

/// A client message serialized with `serde_json`: the client speaks real
/// `JSON`, so its messages are never hand-escaped. (M0 triage: the three
/// previously "load-flaky" failures were the hand-rolled escaping —
/// `COMPONENT_SOURCE`'s `\L`/`\A` backslashes and an embedded literal
/// newline are invalid JSON escapes, so every such didOpen died as
/// -32700 and no document ever opened. Deterministic, not load-related.)
fn message(value: &serde_json::Value) -> String {
    value.to_string()
}

/// Run the server over the given client messages, returning everything it
/// wrote, split into (framed header, body) chunks.
fn session(messages: &[String]) -> String {
    let mut input: Vec<u8> = Vec::new();
    for message in messages {
        input.extend_from_slice(&framed(message));
    }
    let mut output: Vec<u8> = Vec::new();
    let mut reader = Cursor::new(input);
    let code = lintl_lsp::run(&mut reader, &mut output);
    assert_eq!(code, 0, "the session ends cleanly on exit");
    String::from_utf8(output).expect("server output is utf8")
}

/// Collect the JSON bodies of every framed server message.
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

// Component-shaped names only: the inventory records invocations with a
// capitalized first letter or a `\` path (docs/component-inventory.md), so
// a lowercase `<salute>` is an HTML element and hover rightly misses it.
const COMPONENT_SOURCE: &str = concat!(
    "<?php\n\nnamespace App\\Lsp;\n\nuse Lintl\\Html;\n\n",
    "function Salute(string $name, ?Html $children = null): Html\n{\n",
    "    return <div class=\"salute\">{$name}{$children}</div>;\n}\n\n",
    "echo <Salute name=\"LSP\" />;\n"
);

/// The `didOpen` message for `text` at `uri`.
fn open_message(uri: &str, text: &str) -> String {
    message(&serde_json::json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": uri,
                "languageId": "php",
                "version": 1,
                "text": text,
            },
        },
    }))
}

/// The 0-based UTF-16 position of `needle`'s first byte in `text`, the
/// way a real client computes it from its own line model (newline-counted
/// line, byte-delta character on the ASCII fixtures).
fn position_of(text: &str, needle: &str) -> (u32, u32) {
    let offset = text.find(needle).expect("the needle exists in the source");
    let line = u32::try_from(text[..offset].matches('\n').count()).unwrap_or(0);
    let line_start = text[..offset].rfind('\n').map_or(0, |index| index + 1);
    (line, u32::try_from(offset - line_start).unwrap_or(0))
}

/// The request envelope for one document-scoped LSP query.
fn request(id: i32, method: &str, uri: &str, line: u32, character: u32) -> String {
    message(&serde_json::json!({
        "jsonrpc": "2.0", "id": id, "method": method,
        "params": {
            "textDocument": {"uri": uri},
            "position": {"line": line, "character": character},
        },
    }))
}

const EXIT: &str = "{\"jsonrpc\":\"2.0\",\"method\":\"exit\"}";

#[test]
fn initialize_advertises_diagnostics_and_hover() {
    let session = session(&[message(&serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {},
    }))]);
    let bodies = bodies(&session);
    let init = &bodies[0];
    assert_eq!(init["id"], 1);
    assert_eq!(
        init["result"]["capabilities"]["textDocumentSync"], 1,
        "full-text sync"
    );
    assert_eq!(init["result"]["capabilities"]["hoverProvider"], true);
    assert_eq!(init["result"]["serverInfo"]["name"], "lintl-lsp");
}

#[test]
fn did_open_publishes_mapped_diagnostics() {
    // A clean source publishes an empty list (the clear contract); a
    // warning rides publishDiagnostics with a mapped range; a parse error
    // publishes an error-severity diagnostic.
    let session = session(&[
        open_message("file:///proj/Page.lintl.php", COMPONENT_SOURCE),
        EXIT.to_string(),
    ]);
    let bodies = bodies(&session);
    let published = bodies
        .iter()
        .find(|body| body["method"] == "textDocument/publishDiagnostics")
        .expect("diagnostics publish on open");
    assert_eq!(
        published["params"]["uri"].as_str(),
        Some("file:///proj/Page.lintl.php")
    );
    assert_eq!(
        published["params"]["diagnostics"].as_array().map(Vec::len),
        Some(0)
    );
}

#[test]
fn did_open_publishes_error_and_warning_severities() {
    let bad = "<?php\nreturn <button onclick=\"track('cta')\">go</button>;\n";
    let session = session(&[
        open_message("file:///proj/Bad.lintl.php", bad),
        EXIT.to_string(),
    ]);
    let bodies = bodies(&session);
    let published = bodies
        .iter()
        .find(|body| body["method"] == "textDocument/publishDiagnostics")
        .expect("diagnostics publish on open");
    let diagnostics = published["params"]["diagnostics"].as_array().expect("list");
    assert!(!diagnostics.is_empty(), "{published}");
    let first = &diagnostics[0];
    assert_eq!(first["source"], "lintl");
    assert!(
        first["code"].as_str().expect("code").starts_with("LNL"),
        "the stable LNLxxxx id rides along: {first}"
    );
    // LSP positions are 0-based: the diagnostic anchors on line 1 (the
    // source's second line).
    assert_eq!(first["range"]["start"]["line"], 1, "{first}");
}

#[test]
fn hover_renders_the_declared_signature() {
    // `function Salute(string $name, ?Html $children = null): Html` is
    // declared above; hovering over `<Salute` shows its parameters. The
    // tag sits after `echo <` on the last source line.
    let (line, character) = position_of(COMPONENT_SOURCE, "<Salute");
    let session = session(&[
        open_message("file:///proj/Salute.lintl.php", COMPONENT_SOURCE),
        request(
            2,
            "textDocument/hover",
            "file:///proj/Salute.lintl.php",
            line,
            character + 1,
        ),
        EXIT.to_string(),
    ]);
    let bodies = bodies(&session);
    let response = bodies
        .iter()
        .find(|body| body.get("id") == Some(&serde_json::json!(2)))
        .expect("the hover response arrives");
    let hover = &response["result"];
    assert_eq!(
        hover["contents"]["kind"], "markdown",
        "the signature renders as markdown: {hover}"
    );
    let value = hover["contents"]["value"].as_str().expect("value");
    assert!(value.contains("Salute"), "{value}");
    assert!(value.contains("`name!`"), "required params marked: {value}");
    assert!(value.contains("`children`"), "{value}");
}

#[test]
fn hover_off_a_component_is_null() {
    let session = session(&[
        open_message("file:///proj/S.lintl.php", COMPONENT_SOURCE),
        request(3, "textDocument/hover", "file:///proj/S.lintl.php", 0, 0),
        EXIT.to_string(),
    ]);
    let bodies = bodies(&session);
    let response = bodies
        .iter()
        .find(|body| body.get("id") == Some(&serde_json::json!(3)))
        .expect("the hover response arrives");
    assert!(response["result"].is_null(), "{response}");
}

#[test]
fn unknown_requests_answer_method_not_found_and_shutdown_exits() {
    let session = session(&[
        message(&serde_json::json!({
            "jsonrpc": "2.0", "id": 9, "method": "workspace/executeCommand", "params": {},
        })),
        message(&serde_json::json!({
            "jsonrpc": "2.0", "id": 10, "method": "shutdown",
        })),
        EXIT.to_string(),
    ]);
    let bodies = bodies(&session);
    let not_found = bodies
        .iter()
        .find(|body| body.get("id") == Some(&serde_json::json!(9)))
        .expect("the error response arrives");
    assert_eq!(not_found["error"]["code"], -32601);
    let shutdown_response = bodies
        .iter()
        .find(|body| body.get("id") == Some(&serde_json::json!(10)))
        .expect("shutdown answers null");
    assert!(shutdown_response["result"].is_null());
}

// ---------------------------------------------------------------------------
// M1 (§17.1 Q3 LSP card): completion, go-to-definition and signature help.
// Every scenario drives the same mock-client loop; every request position
// is computed client-side and every response position must round-trip
// through the line-preservation guarantee.
// ---------------------------------------------------------------------------

/// The M1 fixture: a declared component with typed and required params, a
/// component invocation, and an HTML element.
const M1_SOURCE: &str = concat!(
    "<?php\n\nnamespace App\\Lsp;\n\nuse Lintl\\Html;\n\n",
    "function Card(string $title, int $count, ?Html $children = null): Html\n{\n",
    "    return <div class=\"card\">{$title}{$children}</div>;\n}\n\n",
    "echo <Card title=\"Hi\" />;\n",
    "echo <a href=\"https://example.com\">go</a>;\n"
);

/// A component declared and invoked in one file — the fixtures for the
/// tag-position scenarios (a prop/signature lookup needs the same-file
/// declaration the scanner records).
const CARD_SOURCE: &str = concat!(
    "<?php\n\nuse Lintl\\Html;\n\n",
    "function Card(string $title, int $count, ?Html $children = null): Html\n{\n",
    "    return <div class=\"card\">{$title}{$children}</div>;\n}\n\n",
    "echo <Card title=\"Hi\" />;\n"
);

/// The cursor position of `needle`'s first byte, offset by `into` bytes
/// into the needle (the way a client places the caret inside a token).
fn position_at(text: &str, needle: &str, into: usize) -> (u32, u32) {
    let (line, character) = position_of(text, needle);
    (line, character + u32::try_from(into).unwrap_or(0))
}

/// The cursor position of an exact byte offset — needle-free, for
/// positions a needle search cannot pin down (two `(`s in one file).
fn position_of_offset(text: &str, offset: usize) -> (u32, u32) {
    let line = u32::try_from(text[..offset].matches('\n').count()).unwrap_or(0);
    let line_start = text[..offset].rfind('\n').map_or(0, |index| index + 1);
    (line, u32::try_from(offset - line_start).unwrap_or(0))
}

/// A client-side UTF-16 position: the 0-based line and the UTF-16 code
/// units from the line start — the LSP contract, verified against the
/// server's own mapping on multibyte fixtures.
fn utf16_position(text: &str, needle: &str) -> (u32, u32) {
    let offset = text.find(needle).expect("the needle exists in the source");
    let line = u32::try_from(text[..offset].matches('\n').count()).unwrap_or(0);
    let line_start = text[..offset].rfind('\n').map_or(0, |index| index + 1);
    let utf16: usize = text[line_start..offset].chars().map(char::len_utf16).sum();
    (line, u32::try_from(utf16).unwrap_or(0))
}

fn m1_session(id: i32, method: &str, line: u32, character: u32) -> (String, serde_json::Value) {
    let session = session(&[
        open_message("file:///proj/M1.lintl.php", M1_SOURCE),
        request(id, method, "file:///proj/M1.lintl.php", line, character),
        EXIT.to_string(),
    ]);
    let bodies = bodies(&session);
    let response = bodies
        .iter()
        .find(|body| body.get("id") == Some(&serde_json::json!(id)))
        .expect("the response arrives")
        .clone();
    (session, response)
}

#[test]
fn initialize_advertises_completion_definition_and_signature_help() {
    let session = session(&[message(&serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {},
    }))]);
    let bodies = bodies(&session);
    let capabilities = &bodies[0]["result"]["capabilities"];
    assert_eq!(capabilities["definitionProvider"], true);
    let triggers = capabilities["completionProvider"]["triggerCharacters"]
        .as_array()
        .expect("completion triggers");
    assert!(
        triggers.iter().any(|t| t.as_str() == Some("<")),
        "completion triggers on `<`: {triggers:?}"
    );
    let signature_triggers = capabilities["signatureHelpProvider"]["triggerCharacters"]
        .as_array()
        .expect("signature triggers");
    assert!(
        signature_triggers.iter().any(|t| t.as_str() == Some("("))
            && signature_triggers.iter().any(|t| t.as_str() == Some(",")),
        "signature help triggers on `(` and `,`: {signature_triggers:?}"
    );
}

#[test]
fn completion_after_lt_offers_elements_and_components() {
    // Mid-edit: the tag is not closed yet — completion still works.
    let source = "<?php\n\nfunction Card(): Html { return <div />; }\n\necho <";
    let (line, character) = position_at(source, "<", 1);
    let session = session(&[
        open_message("file:///proj/M2.lintl.php", source),
        request(
            20,
            "textDocument/completion",
            "file:///proj/M2.lintl.php",
            line,
            character,
        ),
        EXIT.to_string(),
    ]);
    let bodies = bodies(&session);
    let response = bodies
        .iter()
        .find(|body| body.get("id") == Some(&serde_json::json!(20)))
        .expect("the completion response arrives");
    let items = response["result"].as_array().expect("a completion list");
    assert!(
        items
            .iter()
            .any(|item| item["label"].as_str() == Some("Card")),
        "declared components are offered: {items:?}"
    );
    assert!(
        items
            .iter()
            .any(|item| item["label"].as_str() == Some("div")),
        "HTML elements are offered: {items:?}"
    );
}

#[test]
fn completion_filters_by_the_partial_name() {
    let source = "<?php\n\nfunction Card(): Html { return <div />; }\n\necho <Ca";
    let (line, character) = position_at(source, "<Ca", 3);
    let session = session(&[
        open_message("file:///proj/M2.lintl.php", source),
        request(
            21,
            "textDocument/completion",
            "file:///proj/M2.lintl.php",
            line,
            character,
        ),
        EXIT.to_string(),
    ]);
    let bodies = bodies(&session);
    let response = bodies
        .iter()
        .find(|body| body.get("id") == Some(&serde_json::json!(21)))
        .expect("the completion response arrives");
    let items = response["result"].as_array().expect("a completion list");
    assert!(
        items
            .iter()
            .any(|item| item["label"].as_str() == Some("Card")),
        "the partial name filters to matching labels: {items:?}"
    );
    assert!(
        items
            .iter()
            .all(|item| item["label"].as_str() != Some("div")),
        "non-matching labels are filtered: {items:?}"
    );
}

#[test]
fn completion_on_a_component_tag_space_offers_props() {
    // After the tag's space: prop names from the scanned signature, with
    // already-passed props filtered out.
    let source = CARD_SOURCE;
    let (line, character) = position_at(source, "<Card title", 6);
    let session = session(&[
        open_message("file:///proj/M2.lintl.php", source),
        request(
            22,
            "textDocument/completion",
            "file:///proj/M2.lintl.php",
            line,
            character,
        ),
        EXIT.to_string(),
    ]);
    let bodies = bodies(&session);
    let response = bodies
        .iter()
        .find(|body| body.get("id") == Some(&serde_json::json!(22)))
        .expect("the completion response arrives");
    let items = response["result"].as_array().expect("a completion list");
    assert!(
        items
            .iter()
            .any(|item| item["label"].as_str() == Some("count")),
        "the remaining required prop is offered: {items:?}"
    );
    assert!(
        items
            .iter()
            .all(|item| item["label"].as_str() != Some("title")),
        "already-passed props are filtered: {items:?}"
    );
    assert!(
        items
            .iter()
            .all(|item| item["label"].as_str() != Some("children")),
        "`children` is JSX children, not an attribute: {items:?}"
    );
}

#[test]
fn completion_on_an_html_element_offers_its_attributes() {
    let source = "echo <a href=\"https://example.com\">go</a>;";
    // Cursor after the space following `<a`, before `href`.
    let (line, character) = position_at(source, "href", 0);
    let session = session(&[
        open_message("file:///proj/M2.lintl.php", source),
        request(
            23,
            "textDocument/completion",
            "file:///proj/M2.lintl.php",
            line,
            character,
        ),
        EXIT.to_string(),
    ]);
    let bodies = bodies(&session);
    let response = bodies
        .iter()
        .find(|body| body.get("id") == Some(&serde_json::json!(23)))
        .expect("the completion response arrives");
    let items = response["result"].as_array().expect("a completion list");
    assert!(
        items
            .iter()
            .any(|item| item["label"].as_str() == Some("target")),
        "element-specific attributes are offered: {items:?}"
    );
    assert!(
        items
            .iter()
            .any(|item| item["label"].as_str() == Some("id")),
        "global attributes are offered: {items:?}"
    );
    assert!(
        items
            .iter()
            .all(|item| item["label"].as_str() != Some("colspan")),
        "other elements' attributes are not offered: {items:?}"
    );
}

#[test]
fn completion_inside_a_quoted_value_is_empty() {
    let source = CARD_SOURCE;
    // Cursor inside the quoted value `Hi` of the invocation.
    let (line, character) = position_at(source, "\"Hi\"", 1);
    let session = session(&[
        open_message("file:///proj/M2.lintl.php", source),
        request(
            24,
            "textDocument/completion",
            "file:///proj/M2.lintl.php",
            line,
            character,
        ),
        EXIT.to_string(),
    ]);
    let bodies = bodies(&session);
    let response = bodies
        .iter()
        .find(|body| body.get("id") == Some(&serde_json::json!(24)))
        .expect("the completion response arrives");
    assert!(
        response["result"].as_array().map_or(true, Vec::is_empty),
        "no attribute completion inside a quoted value: {response}"
    );
}

#[test]
fn definition_on_a_component_tag_jumps_to_the_declaration() {
    let (line, character) = position_at(M1_SOURCE, "<Card", 2);
    let (_session, response) = m1_session(30, "textDocument/definition", line, character);
    let location = &response["result"];
    assert!(location.is_object(), "a Location comes back: {response}");
    let declared = M1_SOURCE
        .find("function Card")
        .expect("the declaration exists");
    let name_offset = declared + "function ".len();
    let expected = utf16_position(M1_SOURCE, "function Card");
    let expected = (
        expected.0,
        expected.1 + u32::try_from("function ".len()).unwrap_or(0),
    );
    let _ = name_offset;
    assert_eq!(location["uri"].as_str(), Some("file:///proj/M1.lintl.php"));
    assert_eq!(location["range"]["start"]["line"], expected.0, "{location}");
    assert_eq!(
        location["range"]["start"]["character"], expected.1,
        "the definition anchors on the declared name: {location}"
    );
}

#[test]
fn definition_on_a_prop_jumps_to_the_parameter() {
    // Cursor on the `title` prop of `<Card title="Hi" />`.
    let (line, character) = position_at(M1_SOURCE, "title=\"Hi\"", 0);
    let (_session, response) = m1_session(31, "textDocument/definition", line, character);
    let location = &response["result"];
    assert!(location.is_object(), "a Location comes back: {response}");
    let expected = utf16_position(M1_SOURCE, "$title");
    assert_eq!(location["range"]["start"]["line"], expected.0, "{location}");
    assert_eq!(
        location["range"]["start"]["character"], expected.1,
        "the prop resolves to the parameter it names: {location}"
    );
}

#[test]
fn definition_off_a_component_is_null() {
    let (line, character) = position_at(M1_SOURCE, "echo", 0);
    let (_session, response) = m1_session(32, "textDocument/definition", line, character);
    assert!(
        response["result"].is_null(),
        "nothing to resolve off a component: {response}"
    );
}

#[test]
fn signature_help_inside_a_component_tag_marks_required_params() {
    // Cursor after the already-passed `title` attribute: the help shows
    // the typed parameter list, `title!`-style required marks, and the
    // active parameter advances past the passed attribute.
    let source = CARD_SOURCE;
    let (line, character) = position_at(source, "\"Hi\" />", 0);
    let session = session(&[
        open_message("file:///proj/M2.lintl.php", source),
        request(
            33,
            "textDocument/signatureHelp",
            "file:///proj/M2.lintl.php",
            line,
            character,
        ),
        EXIT.to_string(),
    ]);
    let bodies = bodies(&session);
    let response = bodies
        .iter()
        .find(|body| body.get("id") == Some(&serde_json::json!(33)))
        .expect("the signature help response arrives");
    let help = &response["result"];
    let signature = &help["signatures"][0];
    let label = signature["label"].as_str().expect("a signature label");
    assert!(label.contains("Card"), "{label}");
    assert!(
        label.contains("title!"),
        "required params keep their ! mark: {label}"
    );
    assert!(label.contains("children"), "{label}");
    assert_eq!(
        help["activeParameter"], 1,
        "the passed title activates count: {help}"
    );
}

#[test]
fn signature_help_on_a_php_call_paren_and_comma() {
    // The call form inside a `{expr}` child: `(` after the component name
    // and `,` between arguments both show the typed list, with the active
    // parameter advancing per comma.
    let source = concat!(
        "<?php\n\nuse Lintl\\Html;\n\n",
        "function Card(string $title, int $count, ?Html $children = null): Html\n{\n",
        "    return <div>{$title}{$children}</div>;\n}\n\n",
        "echo <div>{Card(\"Hi\", 2)}</div>;\n"
    );
    let open_offset = source.find("Card(").expect("the call exists") + "Card".len();
    let comma_offset = source.find(',').expect("the comma exists");
    // Exact offsets: the file's first `(` is the declaration's, not the
    // call's, so a needle search cannot pin the trigger positions.
    for (id, offset, expected_active) in
        [(40, open_offset + 1, 0_u32), (41, comma_offset + 1, 1_u32)]
    {
        let (line, character) = position_of_offset(source, offset);
        let session = session(&[
            open_message("file:///proj/M3.lintl.php", source),
            request(
                id,
                "textDocument/signatureHelp",
                "file:///proj/M3.lintl.php",
                line,
                character,
            ),
            EXIT.to_string(),
        ]);
        let bodies = bodies(&session);
        let response = bodies
            .iter()
            .find(|body| body.get("id") == Some(&serde_json::json!(id)))
            .expect("the signature help response arrives");
        let help = &response["result"];
        let label = help["signatures"][0]["label"].as_str().expect("a label");
        assert!(label.contains("Card"), "{label}");
        assert_eq!(
            help["activeParameter"], expected_active,
            "the call's argument position activates the matching parameter: {help}"
        );
        let _ = offset;
    }
}

#[test]
fn signature_help_off_a_call_is_null() {
    let (line, character) = position_at(M1_SOURCE, "echo", 0);
    let (_session, response) = m1_session(42, "textDocument/signatureHelp", line, character);
    assert!(
        response["result"].is_null(),
        "no signature help away from any call: {response}"
    );
}

#[test]
fn definition_positions_round_trip_through_utf16() {
    // Multibyte text before both the invocation and the declaration: the
    // request position is computed in UTF-16 units (the client's unit),
    // and the returned Location must land on the declaration in the same
    // units — the line-preservation guarantee end to end.
    let source = concat!(
        "<?php\n\nnamespace App\\Lsp;\n\nuse Lintl\\Html;\n\n",
        "// 卡片 é-component\n",
        "function Card(string $title): Html { return <div>{$title}</div>; }\n\n",
        "echo <Card title=\"你好\" />;\n"
    );
    let (line, character) = utf16_position(source, "<Card");
    let session = session(&[
        open_message("file:///proj/M4.lintl.php", source),
        request(
            50,
            "textDocument/definition",
            "file:///proj/M4.lintl.php",
            line,
            character + 1,
        ),
        EXIT.to_string(),
    ]);
    let bodies = bodies(&session);
    let response = bodies
        .iter()
        .find(|body| body.get("id") == Some(&serde_json::json!(50)))
        .expect("the definition response arrives");
    let location = &response["result"];
    assert!(
        location.is_object(),
        "resolves through multibyte text: {response}"
    );
    let expected = utf16_position(source, "function Card");
    let expected = (
        expected.0,
        expected.1 + u32::try_from("function ".len()).unwrap_or(0),
    );
    assert_eq!(location["range"]["start"]["line"], expected.0, "{location}");
    assert_eq!(
        location["range"]["start"]["character"], expected.1,
        "the declaration position is a UTF-16 position: {location}"
    );
}

// ---------------------------------------------------------------------------
// M2: code actions + formatting (§17.1 Q3+ / Year-2 formatter lane)
// ---------------------------------------------------------------------------

/// The codeAction request for `range` on `uri`.
fn code_action_request(id: i32, uri: &str, start: (u32, u32), end: (u32, u32)) -> String {
    message(&serde_json::json!({
        "jsonrpc": "2.0", "id": id, "method": "textDocument/codeAction",
        "params": {
            "textDocument": {"uri": uri},
            "range": {"start": {"line": start.0, "character": start.1},
                      "end": {"line": end.0, "character": end.1}},
            "context": {"diagnostics": []},
        },
    }))
}

/// The formatting request on `uri` (the options the client always sends).
fn formatting_request(id: i32, uri: &str) -> String {
    message(&serde_json::json!({
        "jsonrpc": "2.0", "id": id, "method": "textDocument/formatting",
        "params": {
            "textDocument": {"uri": uri},
            "options": {"tabSize": 4, "insertSpaces": true},
        },
    }))
}

const EXTRACT_SOURCE: &str = concat!(
    "<?php\n\n",
    "function Page(string $u): Html\n{\n",
    "    return (\n",
    "        <section>\n",
    "            <b class=\"hi\">{$u}</b>\n",
    "        </section>\n",
    "    );\n}\n",
);

#[test]
fn initialize_advertises_code_actions_and_formatting() {
    let session = session(&[message(&serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {},
    }))]);
    let init = &bodies(&session)[0];
    assert_eq!(init["result"]["capabilities"]["codeActionProvider"], true);
    assert_eq!(
        init["result"]["capabilities"]["documentFormattingProvider"],
        true
    );
}

#[test]
fn code_action_extracts_a_selection_to_a_component() {
    let uri = "file:///proj/Extract.lintl.php";
    // The selection covers `<b class="hi">{$u}</b>` — a complete subtree.
    let start = position_of(EXTRACT_SOURCE, "<b class=\"hi\">");
    let end = position_of(EXTRACT_SOURCE, "</b>");
    let end = (end.0, end.1 + u32::try_from("</b>".len()).unwrap_or(0));
    let session = session(&[
        open_message(uri, EXTRACT_SOURCE),
        code_action_request(60, uri, start, end),
        EXIT.to_string(),
    ]);
    let response = bodies(&session)
        .into_iter()
        .find(|body| body.get("id") == Some(&serde_json::json!(60)))
        .expect("the codeAction response arrives");
    let actions = response["result"].as_array().expect("a list of actions");
    let extract = actions
        .iter()
        .find(|action| action["title"] == "Extract to Lintl component")
        .expect("the extract action is offered");
    assert_eq!(extract["kind"], "refactor.extract");

    // The edit creates the sibling file, fills it with the component, and
    // replaces the selection with the invocation.
    let ops = &extract["edit"]["documentChanges"];
    assert_eq!(ops.as_array().map(Vec::len), Some(3), "{extract}");
    assert_eq!(ops[0]["kind"], "create");
    assert_eq!(ops[0]["uri"], "file:///proj/Extracted.lintl.php");

    let body = ops[1]["edits"][0]["newText"].as_str().expect("body text");
    assert!(
        body.contains("function Extracted($u)"),
        "the free $u becomes the component's parameter: {body}"
    );
    assert!(
        body.contains("return <b class=\"hi\">{$u}</b>;"),
        "the selection is the component body: {body}"
    );

    let replace = ops[2]["edits"][0]["newText"].as_str().expect("invocation");
    let replacement = ops[2].clone();
    assert_eq!(replace, "<Extracted u={$u} />", "{replacement}");
}

#[test]
fn code_action_extract_skips_php_and_broken_selections() {
    let uri = "file:///proj/NoExtract.lintl.php";
    // A selection with inline PHP never extracts (the body must compile).
    let start = position_of(EXTRACT_SOURCE, "<?php");
    let end = position_of(EXTRACT_SOURCE, "function Page");
    // An unclosed element is rejected by the same compile gate: the
    // selection opens <section> and stops before </section>.
    let broken_start = position_of(EXTRACT_SOURCE, "<section>");
    let broken_end = position_of(EXTRACT_SOURCE, "</b>");
    let broken_end = (
        broken_end.0,
        broken_end.1 + u32::try_from("</b>".len()).unwrap_or(0),
    );

    let session = session(&[
        open_message(uri, EXTRACT_SOURCE),
        code_action_request(61, uri, start, end),
        code_action_request(62, uri, broken_start, broken_end),
        EXIT.to_string(),
    ]);
    let bodies = bodies(&session);
    for id in [61, 62] {
        let response = bodies
            .iter()
            .find(|body| body.get("id") == Some(&serde_json::json!(id)))
            .expect("the codeAction response arrives");
        let actions = response["result"].as_array().expect("a list");
        assert!(
            actions
                .iter()
                .all(|action| action["title"] != "Extract to Lintl component"),
            "id {id}: no extract for a PHP/broken selection: {response}"
        );
    }
}

const REQUIRED_SOURCE: &str = concat!(
    "<?php\n\n",
    "function Card(string $name, ?Html $children = null): Html\n{\n",
    "    return <div class=\"card\">{$name}{$children}</div>;\n}\n\n",
    "echo <Card />;\n"
);

#[test]
fn code_action_adds_missing_required_props() {
    let uri = "file:///proj/Props.lintl.php";
    let at = position_of(REQUIRED_SOURCE, "<Card");
    let session = session(&[
        open_message(uri, REQUIRED_SOURCE),
        code_action_request(63, uri, at, at),
        EXIT.to_string(),
    ]);
    let response = bodies(&session)
        .into_iter()
        .find(|body| body.get("id") == Some(&serde_json::json!(63)))
        .expect("the codeAction response arrives");
    let actions = response["result"].as_array().expect("a list of actions");
    let fix = actions
        .iter()
        .find(|action| action["title"] == "Add missing required props: name")
        .expect("the quickfix is offered");
    assert_eq!(fix["kind"], "quickfix");

    let edits = &fix["edit"]["changes"][uri];
    assert_eq!(edits.as_array().map(Vec::len), Some(1));
    assert_eq!(edits[0]["newText"], " name=\"\"", "{fix}");
    // The insertion sits right before the self-closing `/>`.
    let tag_line = REQUIRED_SOURCE
        .lines()
        .position(|line| line.contains("<Card />"))
        .unwrap_or(0);
    assert_eq!(edits[0]["range"]["start"]["line"], tag_line as u64);
}

#[test]
fn code_action_offers_nothing_when_required_props_are_passed() {
    let uri = "file:///proj/PropsDone.lintl.php";
    let complete = REQUIRED_SOURCE.replace("<Card />", "<Card name=\"x\" />");
    let at = position_of(&complete, "<Card");
    let session = session(&[
        open_message(uri, &complete),
        code_action_request(64, uri, at, at),
        EXIT.to_string(),
    ]);
    let response = bodies(&session)
        .into_iter()
        .find(|body| body.get("id") == Some(&serde_json::json!(64)))
        .expect("the codeAction response arrives");
    let actions = response["result"].as_array().expect("a list");
    assert!(
        actions.iter().all(|action| action["kind"] != "quickfix"),
        "every required prop is passed: {response}"
    );
}

const MESSY_SOURCE: &str = concat!(
    "<?php\n\n",
    "function Page(): Html\n{\n",
    "    return (\n",
    "      <div>\n",
    "   <b>bold text stays</b>\n",
    "      </div>\n",
    "    );\n}\n",
);

#[test]
fn formatting_normalizes_tag_indentation_and_keeps_php_and_text() {
    let uri = "file:///proj/Fmt.lintl.php";
    let session = session(&[
        open_message(uri, MESSY_SOURCE),
        formatting_request(65, uri),
        EXIT.to_string(),
    ]);
    let response = bodies(&session)
        .into_iter()
        .find(|body| body.get("id") == Some(&serde_json::json!(65)))
        .expect("the formatting response arrives");
    let edits = response["result"].as_array().expect("a list of edits");

    // Apply the edits the way a client would and check every line.
    let mut lines: Vec<String> = MESSY_SOURCE.lines().map(str::to_string).collect();
    for edit in edits {
        let line =
            usize::try_from(edit["range"]["start"]["line"].as_u64().unwrap_or(0)).unwrap_or(0);
        let line_value = lines.get(line).expect("the edit targets a line");
        let first = line_value.len() - line_value.trim_start().len();
        let indent = edit["newText"].as_str().expect("indent");
        let updated = format!("{indent}{}", &line_value[first..]);
        lines[line] = updated;
    }

    // PHP lines and the JSX text keep their exact bytes.
    assert_eq!(lines[0], "<?php");
    assert_eq!(lines[2], "function Page(): Html");
    assert_eq!(lines[3], "{");
    assert_eq!(lines[4], "    return (");
    assert_eq!(
        lines[5], "    <div>",
        "the root tag aligns with the PHP base: {lines:?}"
    );
    assert_eq!(
        lines[6], "        <b>bold text stays</b>",
        "the text after <b> is data — the tag line re-indents: {lines:?}"
    );
    assert_eq!(lines[7], "    </div>", "{lines:?}");
    assert_eq!(lines[8], "    );", "PHP tail untouched");
    assert_eq!(lines[9], "}");
}

// ---------------------------------------------------------------------------
// Phase 8: rename / find references / workspace symbols (§17.1 Q3)
// ---------------------------------------------------------------------------

/// The declaring document: `Salute` declared and once invoked paired.
const RENAME_DECLARE_SOURCE: &str = concat!(
    "<?php\n\n",
    "function Salute(string $name, int $age): Html\n{\n",
    "    return <div>{$name}</div>;\n}\n\n",
    "echo <Salute name=\"a\" age={1}>inner</Salute>;\n"
);

/// The invoking document: `Salute` resolved cross-file.
const RENAME_INVOKE_SOURCE: &str = concat!(
    "<?php\n\n",
    "function Page(): Html\n{\n",
    "    return <Salute name=\"LSP\" age={2} />;\n}\n"
);

const DECLARE_URI: &str = "file:///ws/Salute.lintl.php";
const INVOKE_URI: &str = "file:///ws/Page.lintl.php";

/// Open both rename-fixture documents.
fn rename_session(extra: &[String]) -> String {
    let mut messages = vec![
        open_message(DECLARE_URI, RENAME_DECLARE_SOURCE),
        open_message(INVOKE_URI, RENAME_INVOKE_SOURCE),
    ];
    messages.extend(extra.iter().cloned());
    session(&messages)
}

/// The rename request for `position` on `uri`.
fn rename_request(id: i32, uri: &str, line: u32, character: u32, new_name: &str) -> String {
    message(&serde_json::json!({
        "jsonrpc": "2.0", "id": id, "method": "textDocument/rename",
        "params": {
            "textDocument": {"uri": uri},
            "position": {"line": line, "character": character},
            "newName": new_name,
        },
    }))
}

/// The references request for `position` on `uri`.
fn references_request(
    id: i32,
    uri: &str,
    line: u32,
    character: u32,
    include_declaration: bool,
) -> String {
    message(&serde_json::json!({
        "jsonrpc": "2.0", "id": id, "method": "textDocument/references",
        "params": {
            "textDocument": {"uri": uri},
            "position": {"line": line, "character": character},
            "context": {"includeDeclaration": include_declaration},
        },
    }))
}

/// The workspace/symbol request for `query`.
fn symbol_request(id: i32, query: &str) -> String {
    message(&serde_json::json!({
        "jsonrpc": "2.0", "id": id, "method": "workspace/symbol",
        "params": {"query": query},
    }))
}

/// The last response body carrying `id`.
fn response_of(session_text: &str, id: i32) -> serde_json::Value {
    bodies(session_text)
        .into_iter()
        .find(|body| body["id"] == id)
        .unwrap_or_else(|| panic!("no response for id {id}"))
}

/// Apply the `WorkspaceEdit.changes` edits for `uri` the way a client
/// would (offsets descending within the file) and return the new text.
fn apply_edits(session_text: &str, id: i32, uri: &str, original: &str) -> String {
    let response = response_of(session_text, id);
    let edits = &response["result"]["changes"][uri];
    assert!(
        edits.is_array() && !edits.as_array().expect("edits").is_empty(),
        "the edit set for {uri} is non-empty: {response}"
    );
    let mut ranges: Vec<(usize, usize, &str)> = edits
        .as_array()
        .expect("edits")
        .iter()
        .map(|edit| {
            let start = edit["range"]["start"].clone();
            let end = edit["range"]["end"].clone();
            (
                offset_at(
                    original,
                    u32::try_from(start["line"].as_u64().unwrap_or(0)).unwrap_or(0),
                    u32::try_from(start["character"].as_u64().unwrap_or(0)).unwrap_or(0),
                ),
                offset_at(
                    original,
                    u32::try_from(end["line"].as_u64().unwrap_or(0)).unwrap_or(0),
                    u32::try_from(end["character"].as_u64().unwrap_or(0)).unwrap_or(0),
                ),
                edit["newText"].as_str().expect("newText is a string"),
            )
        })
        .collect();
    ranges.sort_unstable_by_key(|(offset, _end, _text)| std::cmp::Reverse(*offset));
    let mut text = original.to_string();
    for (start, end, new_text) in ranges {
        text.replace_range(start..end, new_text);
    }
    text
}

/// An LSP 0-based (line, UTF-16) position → byte offset (ASCII fixtures:
/// characters are bytes, so the UTF-16 column is the byte column).
fn offset_at(text: &str, line: u32, character: u32) -> usize {
    let mut offset = 0usize;
    for (index, line_text) in text.lines().enumerate() {
        if index == usize::try_from(line).unwrap_or(usize::MAX) {
            return offset + character as usize;
        }
        offset += line_text.len() + 1;
    }
    text.len()
}

#[test]
fn initialize_advertises_rename_references_and_workspace_symbols() {
    let session_text = session(&[message(&serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {},
    }))]);
    let init = &bodies(&session_text)[0];
    assert_eq!(init["result"]["capabilities"]["renameProvider"], true);
    assert_eq!(init["result"]["capabilities"]["referencesProvider"], true);
    assert_eq!(
        init["result"]["capabilities"]["workspaceSymbolProvider"],
        true
    );
}

#[test]
fn rename_from_the_declaration_updates_cross_file_invocations() {
    let (line, character) = position_of(RENAME_DECLARE_SOURCE, "function Salute");
    let character = character + u32::try_from("function ".len()).unwrap_or(0);
    let session_text = rename_session(&[rename_request(10, DECLARE_URI, line, character, "Greet")]);
    let response = response_of(&session_text, 10);
    let changes = &response["result"]["changes"];
    assert!(
        changes[DECLARE_URI].is_array() && changes[INVOKE_URI].is_array(),
        "both files are edited: {response}"
    );
    // The declaring file: the declaration identifier, the open tag name
    // and the close tag name.
    let declare_edits = changes[DECLARE_URI].as_array().expect("edits");
    assert_eq!(
        declare_edits.len(),
        3,
        "decl + open tag + close tag: {response}"
    );
    let invoked = apply_edits(&session_text, 10, INVOKE_URI, RENAME_INVOKE_SOURCE);
    assert!(
        invoked.contains("<Greet name=\"LSP\" age={2} />"),
        "the cross-file invocation renames: {invoked}"
    );
    assert!(
        !invoked.contains("Salute"),
        "no stale name remains: {invoked}"
    );
    let declared = apply_edits(&session_text, 10, DECLARE_URI, RENAME_DECLARE_SOURCE);
    assert!(
        declared.contains("function Greet(")
            && declared.contains("<Greet name=\"a\" age={1}>inner</Greet>"),
        "the declaration and both tags rename: {declared}"
    );
}

#[test]
fn rename_from_an_invocation_reaches_the_declaration() {
    let (line, character) = position_of(RENAME_INVOKE_SOURCE, "<Salute");
    let character = character + 1; // on the tag name's first byte
    let session_text = rename_session(&[rename_request(11, INVOKE_URI, line, character, "Cheer")]);
    let declared = apply_edits(&session_text, 11, DECLARE_URI, RENAME_DECLARE_SOURCE);
    assert!(
        declared.contains("function Cheer("),
        "the declaration renames from an invocation cursor: {declared}"
    );
    let invoked = apply_edits(&session_text, 11, INVOKE_URI, RENAME_INVOKE_SOURCE);
    assert!(invoked.contains("<Cheer name=\"LSP\""), "{invoked}");
}

#[test]
fn rename_a_prop_updates_the_parameter_and_the_attribute_usages() {
    let (line, character) = position_of(RENAME_DECLARE_SOURCE, "$name");
    let session_text = rename_session(&[rename_request(12, DECLARE_URI, line, character, "title")]);
    let declared = apply_edits(&session_text, 12, DECLARE_URI, RENAME_DECLARE_SOURCE);
    assert!(
        declared.contains("function Salute(string $title, int $age)"),
        "the parameter renames: {declared}"
    );
    assert!(
        declared.contains("<Salute title=\"a\" age={1}>"),
        "the same-file attribute renames: {declared}"
    );
    // The BODY child `{$name}` is a child expression, not a prop usage —
    // it must keep its name.
    assert!(
        declared.contains("<div>{$name}</div>"),
        "the body child is not an attribute usage: {declared}"
    );
    let invoked = apply_edits(&session_text, 12, INVOKE_URI, RENAME_INVOKE_SOURCE);
    assert!(
        invoked.contains("<Salute title=\"LSP\" age={2} />"),
        "the cross-file attribute renames: {invoked}"
    );
}

#[test]
fn rename_refuses_invalid_names_and_undeclared_components() {
    // A lowercase new name is not a component (§4.7) — the empty answer.
    let (line, character) = position_of(RENAME_DECLARE_SOURCE, "function Salute");
    let character = character + u32::try_from("function ".len()).unwrap_or(0);
    let session_text = rename_session(&[rename_request(
        13,
        DECLARE_URI,
        line,
        character,
        "greet_lower",
    )]);
    let response = response_of(&session_text, 13);
    assert!(
        response["result"].is_null(),
        "an invalid component name renames nothing: {response}"
    );

    // Same name: no-op rename answers null.
    let session_text =
        rename_session(&[rename_request(14, DECLARE_URI, line, character, "Salute")]);
    let response = response_of(&session_text, 14);
    assert!(response["result"].is_null(), "a no-op rename: {response}");

    // An HTML element is not a component — nothing to rename.
    let (line, character) = position_of(RENAME_DECLARE_SOURCE, "<div>");
    let session_text =
        rename_session(&[rename_request(15, DECLARE_URI, line, character + 1, "Pane")]);
    let response = response_of(&session_text, 15);
    assert!(response["result"].is_null(), "an element tag: {response}");

    // An undeclared component invocation cannot rename (the declaration
    // is invisible — a partial rename would break the workspace).
    let undeclared = concat!(
        "<?php\n\n",
        "function Ghost(): Html\n{\n",
        "    return <Nowhere />;\n}\n"
    );
    let session_text = rename_session(&[rename_request(
        16,
        DECLARE_URI,
        position_of(undeclared, "<Nowhere").0,
        position_of(undeclared, "<Nowhere").1 + 1,
        "There",
    )]);
    let response = response_of(&session_text, 16);
    assert!(
        response["result"].is_null(),
        "an undeclared component: {response}"
    );
}

#[test]
fn references_list_cross_file_usages_with_and_without_the_declaration() {
    let (line, character) = position_of(RENAME_DECLARE_SOURCE, "function Salute");
    let character = character + u32::try_from("function ".len()).unwrap_or(0);
    // With the declaration: decl + open tag + close tag (declaring file)
    // + the invocation tag (invoking file).
    let session_text =
        rename_session(&[references_request(20, DECLARE_URI, line, character, true)]);
    let response_20 = response_of(&session_text, 20);
    let locations = response_20["result"].as_array().expect("locations");
    assert_eq!(
        locations.len(),
        4,
        "decl + 2 tags + cross-file: {locations:?}"
    );
    let from_declare = locations
        .iter()
        .filter(|location| location["uri"] == DECLARE_URI)
        .count();
    let from_invoke = locations
        .iter()
        .filter(|location| location["uri"] == INVOKE_URI)
        .count();
    assert_eq!(from_declare, 3, "{locations:?}");
    assert_eq!(from_invoke, 1, "{locations:?}");

    // Without the declaration: the three usage tags only.
    let session_text =
        rename_session(&[references_request(21, DECLARE_URI, line, character, false)]);
    let response_21 = response_of(&session_text, 21);
    let locations = response_21["result"].as_array().expect("locations");
    assert_eq!(locations.len(), 3, "usage tags only: {locations:?}");
}

#[test]
fn references_on_a_prop_list_the_attribute_usages() {
    let (line, character) = position_of(RENAME_INVOKE_SOURCE, "name=");
    let session_text = rename_session(&[references_request(22, INVOKE_URI, line, character, true)]);
    let response_22 = response_of(&session_text, 22);
    let locations = response_22["result"].as_array().expect("locations");
    // The `$name` declaration + the two written attributes.
    assert_eq!(locations.len(), 3, "{locations:?}");
    let declared = locations
        .iter()
        .find(|location| location["uri"] == DECLARE_URI)
        .expect("the declaration location");
    let (decl_line, decl_char) = position_of(RENAME_DECLARE_SOURCE, "$name");
    assert_eq!(
        declared["range"]["start"]["line"], decl_line,
        "{locations:?}"
    );
    assert_eq!(
        declared["range"]["start"]["character"], decl_char,
        "{locations:?}"
    );
}

#[test]
fn workspace_symbols_fuzzy_search_declared_components() {
    // The subsequence `slt` matches Salute but not Page.
    let session_text = rename_session(&[symbol_request(30, "slt")]);
    let response_30 = response_of(&session_text, 30);
    let symbols = response_30["result"].as_array().expect("symbols");
    assert_eq!(symbols.len(), 1, "one fuzzy hit: {symbols:?}");
    assert_eq!(symbols[0]["name"], "Salute", "{symbols:?}");
    assert_eq!(symbols[0]["kind"], 12, "FUNCTION kind: {symbols:?}");
    assert_eq!(symbols[0]["location"]["uri"], DECLARE_URI, "{symbols:?}");
    let (line, character) = position_of(RENAME_DECLARE_SOURCE, "function Salute");
    let character = character + u32::try_from("function ".len()).unwrap_or(0);
    assert_eq!(symbols[0]["location"]["range"]["start"]["line"], line);
    assert_eq!(
        symbols[0]["location"]["range"]["start"]["character"],
        character
    );

    // The empty query lists every declared component (Salute and Page).
    let session_text = rename_session(&[symbol_request(31, "")]);
    let response_31 = response_of(&session_text, 31);
    let names: Vec<String> = response_31["result"]
        .as_array()
        .expect("symbols")
        .iter()
        .filter_map(|symbol| symbol["name"].as_str().map(str::to_string))
        .collect();
    assert_eq!(names.len(), 2, "every component lists: {names:?}");

    // No match: the empty list is the spec's answer.
    let session_text = rename_session(&[symbol_request(32, "zzz")]);
    let response_32 = response_of(&session_text, 32);
    let symbols = response_32["result"].as_array().expect("symbols");
    assert!(symbols.is_empty(), "no hit: {symbols:?}");
}

// ---------------------------------------------------------------------------
// Phase 9 M1: the persistent workspace index (§17.1 Q3) — the declaration
// file is CLOSED (on disk, never didOpen'd); only the invocation file is
// open. Every scenario runs the real `initialize` walk over a temp
// workspace root, so the index, its on-disk cache and its invalidation are
// under test end to end.
// ---------------------------------------------------------------------------

use std::path::PathBuf;

/// A temp workspace holding ONE closed declaration file (the Phase 8
/// rename fixture's declaring document, written to disk and never
/// opened). Returns `(root, declaration uri, invocation uri)`.
fn disk_workspace(name: &str) -> (PathBuf, String, String) {
    let root = std::env::temp_dir().join(format!("lintl-lsp-ws-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("creates the workspace");
    let declare = root.join("Salute.lintl.php");
    std::fs::write(&declare, RENAME_DECLARE_SOURCE).expect("writes the closed declaration file");
    let uri =
        |path: &std::path::Path| format!("file://{}", path.to_str().expect("utf8 workspace path"));
    (
        root.clone(),
        uri(&declare),
        uri(&root.join("Page.lintl.php")),
    )
}

/// The `initialize` request naming `root` via `workspaceFolders`.
fn initialize_with_root(id: i32, root: &std::path::Path) -> String {
    message(&serde_json::json!({
        "jsonrpc": "2.0", "id": id, "method": "initialize",
        "params": {
            "capabilities": {},
            "workspaceFolders": [
                {"uri": format!("file://{}", root.to_str().expect("utf8 path")), "name": "ws"},
            ],
        },
    }))
}

/// The full-sync `didChange` message for `text` at `uri`.
fn change_message(uri: &str, text: &str) -> String {
    message(&serde_json::json!({
        "jsonrpc": "2.0", "method": "textDocument/didChange",
        "params": {"textDocument": {"uri": uri}, "contentChanges": [{"text": text}]},
    }))
}

/// The `didSave` message carrying `text` at `uri`.
fn save_message(uri: &str, text: &str) -> String {
    message(&serde_json::json!({
        "jsonrpc": "2.0", "method": "textDocument/didSave",
        "params": {"textDocument": {"uri": uri}, "text": text},
    }))
}

/// Clean a disk workspace up (best-effort — a leaked temp dir must never
/// fail a test).
fn remove_workspace(root: &std::path::Path) {
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn definition_resolves_into_a_closed_file_through_the_index() {
    let (root, declare_uri, invoke_uri) = disk_workspace("definition");
    let (line, character) = position_of(RENAME_INVOKE_SOURCE, "<Salute");
    let session_text = session(&[
        initialize_with_root(1, &root),
        open_message(&invoke_uri, RENAME_INVOKE_SOURCE),
        request(
            2,
            "textDocument/definition",
            &invoke_uri,
            line,
            character + 1,
        ),
        EXIT.to_string(),
    ]);
    let response = response_of(&session_text, 2);
    assert_eq!(
        response["result"]["uri"], declare_uri,
        "the declaration lives in the closed file: {response}"
    );
    let expected = position_of(RENAME_DECLARE_SOURCE, "Salute");
    assert_eq!(
        [
            response["result"]["range"]["start"]["line"].clone(),
            response["result"]["range"]["start"]["character"].clone(),
        ],
        [serde_json::json!(expected.0), serde_json::json!(expected.1)],
        "the declaration's line+col resolve in the closed file: {response}"
    );
    remove_workspace(&root);
}

#[test]
fn initialize_advertises_the_definition_provider() {
    // The card's capability bullet: `definitionProvider` is advertised
    // (the index makes it true cross-file, not only same-file).
    let session_text = session(&[message(&serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {},
    }))]);
    let init = &bodies(&session_text)[0];
    assert_eq!(init["result"]["capabilities"]["definitionProvider"], true);
}

#[test]
fn rename_of_a_closed_declaration_edits_the_disk_file() {
    let (root, declare_uri, invoke_uri) = disk_workspace("rename");
    let (line, character) = position_of(RENAME_INVOKE_SOURCE, "<Salute");
    let session_text = session(&[
        initialize_with_root(1, &root),
        open_message(&invoke_uri, RENAME_INVOKE_SOURCE),
        rename_request(2, &invoke_uri, line, character + 1, "Cheer"),
        EXIT.to_string(),
    ]);
    // The closed file's edits (declaration + open + close tags) ride the
    // WorkspaceEdit, computed against the disk bytes the index validated.
    let declared = apply_edits(&session_text, 2, &declare_uri, RENAME_DECLARE_SOURCE);
    assert!(
        declared.contains("function Cheer(")
            && declared.contains("<Cheer name=\"a\" age={1}>inner</Cheer>"),
        "the closed declaration file renames: {declared}"
    );
    let invoked = apply_edits(&session_text, 2, &invoke_uri, RENAME_INVOKE_SOURCE);
    assert!(
        invoked.contains("<Cheer name=\"LSP\" age={2} />"),
        "the open invocation renames: {invoked}"
    );
    remove_workspace(&root);
}

#[test]
fn references_reach_usages_in_closed_files() {
    let (root, declare_uri, invoke_uri) = disk_workspace("references");
    let (line, character) = position_of(RENAME_INVOKE_SOURCE, "<Salute");
    let session_text = session(&[
        initialize_with_root(1, &root),
        open_message(&invoke_uri, RENAME_INVOKE_SOURCE),
        references_request(2, &invoke_uri, line, character + 1, true),
        EXIT.to_string(),
    ]);
    let response = response_of(&session_text, 2);
    let locations = response["result"].as_array().expect("locations");
    let uris: Vec<&str> = locations
        .iter()
        .filter_map(|location| location["uri"].as_str())
        .collect();
    assert_eq!(
        uris.iter().filter(|uri| **uri == declare_uri).count(),
        3,
        "declaration + open tag + close tag in the closed file: {uris:?}"
    );
    assert_eq!(
        uris.iter().filter(|uri| **uri == invoke_uri).count(),
        1,
        "the open file's invocation: {uris:?}"
    );
    remove_workspace(&root);
}

#[test]
fn workspace_symbols_list_closed_file_declarations() {
    let (root, declare_uri, _invoke_uri) = disk_workspace("symbols");
    let session_text = session(&[
        initialize_with_root(1, &root),
        symbol_request(2, "sal"),
        EXIT.to_string(),
    ]);
    let response = response_of(&session_text, 2);
    let symbols = response["result"].as_array().expect("symbols");
    assert_eq!(symbols.len(), 1, "only Salute matches: {symbols:?}");
    assert_eq!(symbols[0]["name"], "Salute");
    assert_eq!(symbols[0]["location"]["uri"], declare_uri);
    remove_workspace(&root);
}

#[test]
fn index_invalidation_rides_the_client_sync() {
    let (root, declare_uri, invoke_uri) = disk_workspace("invalidation");
    // The declaration file is OPEN here: its buffer is the truth, and a
    // didChange full sync (then a didSave) must re-parse its declarations
    // into the index — the cross-file queries see the renamed component
    // immediately.
    let edited = RENAME_DECLARE_SOURCE.replace("Salute", "Salutation");
    let invoke_edited = RENAME_INVOKE_SOURCE.replace("Salute", "Salutation");
    let (line, character) = position_of(&invoke_edited, "<Salutation");
    let session_text = session(&[
        initialize_with_root(1, &root),
        open_message(&declare_uri, RENAME_DECLARE_SOURCE),
        open_message(&invoke_uri, &invoke_edited),
        change_message(&declare_uri, &edited),
        save_message(&declare_uri, &edited),
        request(
            2,
            "textDocument/definition",
            &invoke_uri,
            line,
            character + 1,
        ),
        symbol_request(3, "salutation"),
        EXIT.to_string(),
    ]);
    let response = response_of(&session_text, 2);
    assert_eq!(
        response["result"]["uri"], declare_uri,
        "definition resolves into the synced buffer: {response}"
    );
    let expected = position_of(&edited, "Salutation");
    assert_eq!(
        [
            response["result"]["range"]["start"]["line"].clone(),
            response["result"]["range"]["start"]["character"].clone(),
        ],
        [serde_json::json!(expected.0), serde_json::json!(expected.1)],
        "the re-parsed declaration position: {response}"
    );
    let symbols = response_of(&session_text, 3)["result"]
        .as_array()
        .expect("symbols")
        .iter()
        .filter_map(|symbol| symbol["name"].as_str().map(str::to_string))
        .collect::<Vec<_>>();
    assert!(
        symbols.iter().any(|name| name == "Salutation")
            && !symbols.iter().any(|name| name == "Salute"),
        "the index re-parsed the synced text: {symbols:?}"
    );
    remove_workspace(&root);
}

#[test]
fn the_on_disk_cache_round_trips_and_evicts_across_sessions() {
    let (root, declare_uri, _invoke_uri) = disk_workspace("cache-sessions");
    // Session 1: the walk persists the cache beside the workspace.
    let session_text = session(&[initialize_with_root(1, &root), EXIT.to_string()]);
    assert!(
        root.join(".lintl-cache/lsp-index.json").is_file(),
        "the cache persists into .lintl-cache: {session_text}"
    );

    // Session 2 over the unchanged workspace: the cache round-trips — the
    // closed declaration still answers.
    let session_text = session(&[
        initialize_with_root(1, &root),
        symbol_request(2, "sal"),
        EXIT.to_string(),
    ]);
    let response = response_of(&session_text, 2);
    let symbols = response["result"].as_array().expect("symbols");
    assert_eq!(symbols.len(), 1, "the cache-hit entry answers: {response}");
    assert_eq!(symbols[0]["location"]["uri"], declare_uri);

    // Session 3 after the disk file changed: mtime+size eviction — the
    // stale cache entry is gone and the fresh declarations answer.
    std::fs::write(
        root.join("Salute.lintl.php"),
        "<?php\nfunction Later(string $name): Html\n{\n    return <u>{$name}</u>;\n}\n",
    )
    .expect("rewrites the closed file");
    let session_text = session(&[
        initialize_with_root(1, &root),
        symbol_request(2, "later"),
        symbol_request(3, "sal"),
        EXIT.to_string(),
    ]);
    let later = response_of(&session_text, 2);
    assert_eq!(
        later["result"].as_array().expect("symbols").len(),
        1,
        "the fresh declaration is indexed: {later}"
    );
    let salute = response_of(&session_text, 3);
    assert!(
        salute["result"].as_array().expect("symbols").is_empty(),
        "the stale entry was evicted across sessions: {salute}"
    );
    remove_workspace(&root);
}

// ---------------------------------------------------------------------------
// Phase 10 M3: the monorepo `lsp.include` config (the Phase 9 §f backlog).
// A monorepo workspace — real sources in packages/{ui,app}/src, vendored
// component sources in vendor/, a stray root source — configures
// `lintl.json` `{"lsp": {"include": [patterns]}}` and the walk indexes
// ONLY the configured patterns: vendor and the stray file stay out even
// though the default walk (or the `**` in the patterns themselves) would
// take them. Runs the real `initialize` over the workspace root and reads
// the indexed set back through `workspace/symbol`.
// ---------------------------------------------------------------------------

/// The declaration source the closed-file fixtures use.
fn monorepo_workspace(name: &str, config: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("lintl-lsp-monorepo-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("packages/ui/src")).expect("creates ui package");
    std::fs::create_dir_all(root.join("packages/app/src/deep")).expect("creates app package");
    std::fs::create_dir_all(root.join("vendor/acme")).expect("creates the vendor tree");
    std::fs::write(
        root.join("packages/ui/src/Badge.lintl.php"),
        RENAME_DECLARE_SOURCE,
    )
    .expect("writes the ui source");
    std::fs::write(
        root.join("packages/app/src/deep/Page.lintl.php"),
        RENAME_DECLARE_SOURCE,
    )
    .expect("writes the app source");
    std::fs::write(
        root.join("vendor/acme/Vendored.lintl.php"),
        RENAME_DECLARE_SOURCE,
    )
    .expect("writes the vendored source");
    std::fs::write(root.join("Stray.lintl.php"), RENAME_DECLARE_SOURCE)
        .expect("writes the stray source");
    if !config.is_empty() {
        std::fs::write(root.join("lintl.json"), config).expect("writes the config");
    }
    root
}

/// The indexed component names the server reports for `query` after an
/// initialize over `root`.
fn indexed_symbols(root: &std::path::Path, id: i32, query: &str) -> Vec<String> {
    let session_text = session(&[
        initialize_with_root(1, root),
        symbol_request(id, query),
        EXIT.to_string(),
    ]);
    response_of(&session_text, id)["result"]
        .as_array()
        .expect("symbols")
        .iter()
        .filter_map(|symbol| symbol["name"].as_str().map(str::to_string))
        .collect()
}

#[test]
fn the_monorepo_include_list_indexes_only_the_configured_patterns() {
    let root = monorepo_workspace(
        "include",
        r#"{"lsp": {"include": ["packages/*/src/**/*.lintl.php"]}}"#,
    );
    let names = indexed_symbols(&root, 2, "");
    assert_eq!(
        names.len(),
        2,
        "only the configured patterns walk in: {names:?}"
    );
    // Every listed component comes from a packages/ source; the vendored
    // tree and the stray root file stay out.
    assert!(
        names.iter().all(|name| name == "Salute"),
        "the two package sources declare Salute: {names:?}"
    );
    remove_workspace(&root);
}

#[test]
fn a_monorepo_without_the_include_config_keeps_the_default_walk() {
    // The control: no config (and no vendored-skip ambiguity) — the
    // default walk indexes the packages AND the stray root source, and
    // still skips vendor/.
    let root = monorepo_workspace("default", "");
    let names = indexed_symbols(&root, 2, "");
    assert_eq!(
        names.len(),
        3,
        "the default walk takes every non-vendored source: {names:?}"
    );
    remove_workspace(&root);
}
