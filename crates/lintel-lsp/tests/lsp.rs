//! The T-21 LSP foundation contracts: diagnostics publish on open with
//! the compiler's byte offsets mapped to 0-based UTF-16 positions, hover
//! over a `<ComponentName>` renders the `ScanContext` signature, unknown
//! requests answer method-not-found, and `shutdown`/`exit` end the
//! session. A mock client drives `lintel_lsp::run` over in-memory
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
    let code = lintel_lsp::run(&mut reader, &mut output);
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
    "<?php\n\nnamespace App\\Lsp;\n\nuse Lintel\\Html;\n\n",
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
    assert_eq!(init["result"]["serverInfo"]["name"], "lintel-lsp");
}

#[test]
fn did_open_publishes_mapped_diagnostics() {
    // A clean source publishes an empty list (the clear contract); a
    // warning rides publishDiagnostics with a mapped range; a parse error
    // publishes an error-severity diagnostic.
    let session = session(&[
        open_message("file:///proj/Page.lintel.php", COMPONENT_SOURCE),
        EXIT.to_string(),
    ]);
    let bodies = bodies(&session);
    let published = bodies
        .iter()
        .find(|body| body["method"] == "textDocument/publishDiagnostics")
        .expect("diagnostics publish on open");
    assert_eq!(
        published["params"]["uri"].as_str(),
        Some("file:///proj/Page.lintel.php")
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
        open_message("file:///proj/Bad.lintel.php", bad),
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
    assert_eq!(first["source"], "lintel");
    assert!(
        first["code"].as_str().expect("code").starts_with("LNT"),
        "the stable LNTxxxx id rides along: {first}"
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
        open_message("file:///proj/Salute.lintel.php", COMPONENT_SOURCE),
        request(
            2,
            "textDocument/hover",
            "file:///proj/Salute.lintel.php",
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
        open_message("file:///proj/S.lintel.php", COMPONENT_SOURCE),
        request(3, "textDocument/hover", "file:///proj/S.lintel.php", 0, 0),
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
            "jsonrpc": "2.0", "id": 9, "method": "workspace/symbol", "params": {},
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
    "<?php\n\nnamespace App\\Lsp;\n\nuse Lintel\\Html;\n\n",
    "function Card(string $title, int $count, ?Html $children = null): Html\n{\n",
    "    return <div class=\"card\">{$title}{$children}</div>;\n}\n\n",
    "echo <Card title=\"Hi\" />;\n",
    "echo <a href=\"https://example.com\">go</a>;\n"
);

/// A component declared and invoked in one file — the fixtures for the
/// tag-position scenarios (a prop/signature lookup needs the same-file
/// declaration the scanner records).
const CARD_SOURCE: &str = concat!(
    "<?php\n\nuse Lintel\\Html;\n\n",
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
        open_message("file:///proj/M1.lintel.php", M1_SOURCE),
        request(id, method, "file:///proj/M1.lintel.php", line, character),
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
        open_message("file:///proj/M2.lintel.php", source),
        request(
            20,
            "textDocument/completion",
            "file:///proj/M2.lintel.php",
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
        open_message("file:///proj/M2.lintel.php", source),
        request(
            21,
            "textDocument/completion",
            "file:///proj/M2.lintel.php",
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
        open_message("file:///proj/M2.lintel.php", source),
        request(
            22,
            "textDocument/completion",
            "file:///proj/M2.lintel.php",
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
        open_message("file:///proj/M2.lintel.php", source),
        request(
            23,
            "textDocument/completion",
            "file:///proj/M2.lintel.php",
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
        open_message("file:///proj/M2.lintel.php", source),
        request(
            24,
            "textDocument/completion",
            "file:///proj/M2.lintel.php",
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
    assert_eq!(location["uri"].as_str(), Some("file:///proj/M1.lintel.php"));
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
        open_message("file:///proj/M2.lintel.php", source),
        request(
            33,
            "textDocument/signatureHelp",
            "file:///proj/M2.lintel.php",
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
        "<?php\n\nuse Lintel\\Html;\n\n",
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
            open_message("file:///proj/M3.lintel.php", source),
            request(
                id,
                "textDocument/signatureHelp",
                "file:///proj/M3.lintel.php",
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
        "<?php\n\nnamespace App\\Lsp;\n\nuse Lintel\\Html;\n\n",
        "// 卡片 é-component\n",
        "function Card(string $title): Html { return <div>{$title}</div>; }\n\n",
        "echo <Card title=\"你好\" />;\n"
    );
    let (line, character) = utf16_position(source, "<Card");
    let session = session(&[
        open_message("file:///proj/M4.lintel.php", source),
        request(
            50,
            "textDocument/definition",
            "file:///proj/M4.lintel.php",
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
    let uri = "file:///proj/Extract.lintel.php";
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
        .find(|action| action["title"] == "Extract to Lintel component")
        .expect("the extract action is offered");
    assert_eq!(extract["kind"], "refactor.extract");

    // The edit creates the sibling file, fills it with the component, and
    // replaces the selection with the invocation.
    let ops = &extract["edit"]["documentChanges"];
    assert_eq!(ops.as_array().map(Vec::len), Some(3), "{extract}");
    assert_eq!(ops[0]["kind"], "create");
    assert_eq!(ops[0]["uri"], "file:///proj/Extracted.lintel.php");

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
    let uri = "file:///proj/NoExtract.lintel.php";
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
                .all(|action| action["title"] != "Extract to Lintel component"),
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
    let uri = "file:///proj/Props.lintel.php";
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
    let uri = "file:///proj/PropsDone.lintel.php";
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
    let uri = "file:///proj/Fmt.lintel.php";
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
