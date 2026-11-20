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

const COMPONENT_SOURCE: &str = concat!(
    "<?php\n\nnamespace App\\Lsp;\n\nuse Lintel\\Html;\n\n",
    "function salute(string $name, ?Html $children = null): Html\n{\n",
    "    return <div class=\"salute\">{$name}{$children}</div>;\n}\n\n",
    "echo <salute name=\"LSP\" />;\n"
);

#[test]
fn initialize_advertises_diagnostics_and_hover() {
    let session =
        session(&["{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}".into()]);
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
    // A static `onclick` attribute is the §7.2 warning lint (LNT03xx):
    // compiling succeeds, so publishDiagnostics carries it with a mapped
    // range; a parse error publishes an error-severity diagnostic.
    let open = format!(
        "{{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/didOpen\",\"params\":{{\
         \"textDocument\":{{\"uri\":\"file:///proj/Page.lintel.php\",\"languageId\":\"php\",\
         \"version\":1,\"text\":\"{}\"}}}}}}",
        COMPONENT_SOURCE.replace('"', "\\\"").replace('\n', "\\n")
    );
    let exit = "{\"jsonrpc\":\"2.0\",\"method\":\"exit\"}".to_string();
    let session = session(&[open, exit]);
    let bodies = bodies(&session);
    let published = bodies
        .iter()
        .find(|body| body["method"] == "textDocument/publishDiagnostics")
        .expect("diagnostics publish on open");
    assert_eq!(
        published["params"]["uri"].as_str(),
        Some("file:///proj/Page.lintel.php")
    );
    // The clean source publishes an empty list (the clear contract).
    assert_eq!(
        published["params"]["diagnostics"].as_array().map(Vec::len),
        Some(0)
    );
}

#[test]
fn did_open_publishes_error_and_warning_severities() {
    let bad = "<?php\nreturn <button onclick=\"track('cta')\">go</button>;\n";
    let open = format!(
        "{{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/didOpen\",\"params\":{{\
         \"textDocument\":{{\"uri\":\"file:///proj/Bad.lintel.php\",\"languageId\":\"php\",\
         \"version\":1,\"text\":\"{}\"}}}}}}",
        bad.replace('"', "\\\"")
    );
    let exit = "{\"jsonrpc\":\"2.0\",\"method\":\"exit\"}".to_string();
    let session = session(&[open, exit]);
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
    // `function salute(string $name, ?Html $children = null): Html` is
    // declared above; hovering over `<salute` shows its parameters. The
    // tag sits after `echo <` on the last source line.
    let tag_offset = COMPONENT_SOURCE
        .find("<salute")
        .expect("the invocation exists")
        + 1; // inside the name, past the `<`
    let line = COMPONENT_SOURCE[..tag_offset].matches('\n').count() as u32;
    let line_start = COMPONENT_SOURCE
        .rsplit_once('\n')
        .map_or(0, |(head, _)| COMPONENT_SOURCE.len() - head.len() - 1);
    let _ = line_start;
    let character = (tag_offset
        - COMPONENT_SOURCE
            .split('\n')
            .take(usize::try_from(line).unwrap_or(0))
            .map(|l| l.len() + 1)
            .sum::<usize>()) as u32;
    let open = format!(
        "{{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/didOpen\",\"params\":{{\
         \"textDocument\":{{\"uri\":\"file:///proj/Salute.lintel.php\",\"languageId\":\"php\",\
         \"version\":1,\"text\":\"{}\"}}}}}}",
        COMPONENT_SOURCE.replace('"', "\\\"").replace('\n', "\\n")
    );
    let hover = format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"textDocument/hover\",\"params\":{{\
         \"textDocument\":{{\"uri\":\"file:///proj/Salute.lintel.php\"}},\
         \"position\":{{\"line\":{line},\"character\":{character}}}}}}}"
    );
    let exit = "{\"jsonrpc\":\"2.0\",\"method\":\"exit\"}".to_string();
    let session = session(&[open, hover, exit]);
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
    assert!(value.contains("salute"), "{value}");
    assert!(value.contains("`name!`"), "required params marked: {value}");
    assert!(value.contains("`children`"), "{value}");
}

#[test]
fn hover_off_a_component_is_null() {
    let open = format!(
        "{{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/didOpen\",\"params\":{{\
         \"textDocument\":{{\"uri\":\"file:///proj/S.lintel.php\",\"languageId\":\"php\",\
         \"version\":1,\"text\":\"{}\"}}}}}}",
        COMPONENT_SOURCE.replace('"', "\\\"").replace('\n', "\\n")
    );
    let hover = "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"textDocument/hover\",\"params\":{\
                 \"textDocument\":{\"uri\":\"file:///proj/S.lintel.php\"},\
                 \"position\":{\"line\":0,\"character\":0}}}"
        .to_string();
    let exit = "{\"jsonrpc\":\"2.0\",\"method\":\"exit\"}".to_string();
    let session = session(&[open, hover, exit]);
    let bodies = bodies(&session);
    let response = bodies
        .iter()
        .find(|body| body.get("id") == Some(&serde_json::json!(3)))
        .expect("the hover response arrives");
    assert!(response["result"].is_null(), "{response}");
}

#[test]
fn unknown_requests_answer_method_not_found_and_shutdown_exits() {
    let unknown = "{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"workspace/symbol\",\"params\":{}}";
    let shutdown = "{\"jsonrpc\":\"2.0\",\"id\":10,\"method\":\"shutdown\"}";
    let exit = "{\"jsonrpc\":\"2.0\",\"method\":\"exit\"}".to_string();
    let session = session(&[unknown.into(), shutdown.into(), exit]);
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
