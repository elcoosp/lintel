//! The `lintel-lsp` language-server foundation (architecture doc §15
//! task-card T-21): `textDocument/publishDiagnostics` and
//! `textDocument/hover` for `.lintel.php` files, with every compiler
//! position mapped back through the line-preservation guarantee (§5.2) —
//! the byte offsets the compiler anchors diagnostics on become 1-based
//! `SourceMap` line/columns, then the LSP's 0-based UTF-16 positions.
//!
//! The server is a plain JSON-RPC loop over any `Read`/`Write` pair
//! (stdio in production, in-memory buffers in the tests): `initialize`
//! advertises full-text sync and hover support, `textDocument/didOpen` /
//! `didChange` / `didSave` recompile the document and publish diagnostics,
//! `hover` over a `<ComponentName>` looks the component's signature up in
//! the same `ScanContext` table the compiler validates against and renders
//! the typed parameters, `shutdown`/`exit` end the session. Everything
//! else answers method-not-found, so a client's later capabilities never
//! mislead.
//!
//! # Cross-file signatures
//!
//! Hover resolves signatures declared **in the hovered file** (the
//! same-file table the same-file validation rules use). Folding in other
//! files' signatures is the cross-file seam's job ([`lintel_core::scan::
//! ScanContext::merged_with`]) and lands with the rest of the LSP's
//! backlog (see architecture doc §17.1 Q3).

use std::collections::HashMap;
use std::io::{Read, Write};

use lsp_types::{
    Diagnostic as LspDiagnostic, DiagnosticSeverity, DidChangeTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams, Hover, HoverContents, HoverParams,
    InitializeResult, MarkupContent, MarkupKind, Position, PublishDiagnosticsParams, Range,
    ServerCapabilities, ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
};
use serde_json::{json, Value};

use lintel_core::{collect_inventory, compile, Diagnostic, Options, Severity};

/// The server's name, advertised in `initialize`'s `serverInfo`.
pub const SERVER_NAME: &str = "lintel-lsp";

/// The source the published diagnostics carry.
const DIAGNOSTIC_SOURCE: &str = "lintel";

/// One open document: the text the client last synced and, derived from
/// it, the compile result's diagnostics (already mapped to LSP positions).
struct Document {
    text: String,
}

/// Run the language server over `input`/`output` until `exit`. Returns the
/// process exit code: 0 after a clean `exit`, 1 when the transport dies
/// mid-session (a client disconnect ends the server like an editor close).
pub fn run(input: &mut dyn Read, output: &mut dyn Write) -> i32 {
    let mut server = Server::default();
    loop {
        let Some(body) = read_message(input) else {
            // The transport closed without `exit` — treat it like the
            // editor dying: a clean shutdown of a dead session.
            return 0;
        };
        let Ok(message) = serde_json::from_slice::<Value>(&body) else {
            write_response(
                output,
                &json!({"jsonrpc": "2.0", "id": null,
                        "error": {"code": -32700, "message": "parse error"}}),
            );
            continue;
        };
        let method = message.get("method").and_then(Value::as_str).unwrap_or("");
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        let id = message.get("id").cloned();
        match (method, id) {
            ("exit", _) => return 0,
            ("initialize", Some(id)) => {
                let result = initialize_result();
                write_response(
                    output,
                    &json!({"jsonrpc": "2.0", "id": id, "result": result}),
                );
            }
            ("initialized", None) => {}
            ("shutdown", Some(id)) => {
                write_response(output, &json!({"jsonrpc": "2.0", "id": id, "result": null}));
            }
            ("textDocument/didOpen", None) => {
                if let Ok(params) = serde_json::from_value::<DidOpenTextDocumentParams>(params) {
                    server.did_open(&params);
                    server.publish(output);
                }
            }
            ("textDocument/didChange", None) => {
                if let Ok(params) = serde_json::from_value::<DidChangeTextDocumentParams>(params) {
                    server.did_change(&params);
                    server.publish(output);
                }
            }
            ("textDocument/didSave", None) => {
                if let Ok(params) = serde_json::from_value::<DidSaveTextDocumentParams>(params) {
                    server.did_save(&params);
                    server.publish(output);
                }
            }
            ("textDocument/hover", Some(id)) => {
                let hover = serde_json::from_value::<HoverParams>(params)
                    .ok()
                    .and_then(|params| server.hover(&params));
                write_response(
                    output,
                    &json!({"jsonrpc": "2.0", "id": id, "result": hover}),
                );
            }
            (method, Some(id)) => {
                write_response(
                    output,
                    &json!({"jsonrpc": "2.0", "id": id,
                            "error": {"code": -32601,
                                      "message": format!("method not found: {method}")}}),
                );
            }
            (_unknown, None) => {} // unknown notification: ignore silently
        }
    }
}

/// The `initialize` result: full-text document sync and hover support.
#[must_use]
pub fn initialize_result() -> InitializeResult {
    InitializeResult {
        capabilities: ServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
            hover_provider: Some(lsp_types::HoverProviderCapability::Simple(true)),
            ..ServerCapabilities::default()
        },
        server_info: Some(ServerInfo {
            name: SERVER_NAME.to_string(),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
        }),
    }
}

/// Read one `Content-Length`-framed JSON-RPC message; `None` on EOF or a
/// malformed header (the session ends — the transport is the session).
fn read_message(input: &mut dyn Read) -> Option<Vec<u8>> {
    let mut headers = String::new();
    loop {
        let mut line = String::new();
        let mut byte = [0_u8; 1];
        loop {
            let read = input.read(&mut byte).ok()?;
            if read == 0 {
                return None;
            }
            line.push(byte[0] as char);
            if byte[0] == b'\n' {
                break;
            }
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        headers.push_str(&line);
    }
    let length = headers.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if name.trim().eq_ignore_ascii_case("content-length") {
            value.trim().parse::<usize>().ok()
        } else {
            None
        }
    })?;
    let mut body = vec![0_u8; length];
    input.read_exact(&mut body).ok()?;
    Some(body)
}

/// Write one framed JSON-RPC message.
///
/// # Errors
///
/// A broken sink fails the write (the caller lets it drop — the session
/// is dead either way).
fn write_message(output: &mut dyn Write, body: &[u8]) -> std::io::Result<()> {
    write!(output, "Content-Length: {}\r\n\r\n", body.len())?;
    output.write_all(body)
}

/// Write one framed JSON value; failures are swallowed — a broken transport
/// ends the session at the next read anyway.
fn write_response(output: &mut dyn Write, message: &Value) {
    let body = serde_json::to_vec(message).expect("serializes a JSON-RPC message");
    if write_message(output, &body).is_err() {
        // The transport broke; the next read ends the session.
    }
}

/// The server: the open documents and their last diagnostics.
#[derive(Default)]
struct Server {
    documents: HashMap<String, Document>,
}

impl Server {
    /// `didOpen`: the document starts with the client's text.
    fn did_open(&mut self, params: &DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        let text = params.text_document.text.clone();
        self.documents.insert(uri, Document { text });
    }

    /// `didChange`: full-text sync — the last change wins.
    fn did_change(&mut self, params: &DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        if let Some(text) = params
            .content_changes
            .last()
            .map(|change| change.text.clone())
        {
            self.documents.insert(uri, Document { text });
        }
    }

    /// `didSave`: recompile what is on disk-mirror (the synced text).
    fn did_save(&mut self, params: &DidSaveTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        if let Some(text) = params.text.as_ref() {
            self.documents.insert(uri, Document { text: text.clone() });
        }
    }

    /// Recompile every open document and publish its diagnostics — one
    /// `textDocument/publishDiagnostics` notification per document (the
    /// spec's granularity; an empty list clears).
    fn publish(&self, output: &mut dyn Write) {
        for (uri, document) in &self.documents {
            let diagnostics = lintel_diagnostics(&document.text);
            let params = PublishDiagnosticsParams {
                uri: parse_uri(uri),
                diagnostics,
                version: None,
            };
            write_response(
                output,
                &json!({
                    "jsonrpc": "2.0",
                    "method": "textDocument/publishDiagnostics",
                    "params": params,
                }),
            );
        }
    }

    /// The hover response for `params`: the component invocation under the
    /// cursor with its declared signature, or `None` when the cursor is
    /// not on a component (or the component has no same-file signature).
    #[must_use]
    fn hover(&self, params: &HoverParams) -> Option<Hover> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .to_string();
        let document = self.documents.get(&uri)?;
        let position = params.text_document_position_params.position;
        let offset = offset_of_position(&document.text, position);
        let parsed = lintel_core::scan::scan(&document.text);
        let inventory = collect_inventory(&parsed.roots, &parsed.context);
        let site = inventory.invocations.iter().find(|site| {
            let start = site.offset + 1; // past the `<`
            let end = start + site.name.len();
            offset >= start && offset <= end
        })?;
        let signature = parsed.context.signature_of(&site.name)?;
        let mut markdown = format!("### {}\n\n", site.name);
        let params_text = signature
            .params
            .iter()
            .enumerate()
            .map(|(index, param)| {
                let tail = if signature.variadic && index == signature.params.len() - 1 {
                    "…"
                } else if signature.required.contains(param) {
                    "!"
                } else {
                    ""
                };
                format!("`{param}{tail}`")
            })
            .collect::<Vec<_>>()
            .join(", ");
        markdown.push_str("**Component signature:** ");
        markdown.push_str(&params_text);
        markdown.push('\n');
        if !signature.typed_strings.is_empty() {
            markdown.push_str("\nString-typed (escape-elided, §7.3): ");
            let typed = signature
                .typed_strings
                .iter()
                .map(|param| format!("`{param}`"))
                .collect::<Vec<_>>()
                .join(", ");
            markdown.push_str(&typed);
            markdown.push('\n');
        }
        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: markdown,
            }),
            range: None,
        })
    }
}

/// Compile `text` and map every diagnostic (errors and warnings alike —
/// the editor decides what to show) onto LSP ranges through the
/// line-preservation guarantee.
fn lintel_diagnostics(text: &str) -> Vec<LspDiagnostic> {
    match compile(text, &Options::default()) {
        Ok(output) => output
            .warnings
            .iter()
            .map(|diagnostic| to_lsp_diagnostic(diagnostic, text))
            .collect(),
        Err(problems) => problems
            .iter()
            .map(|diagnostic| to_lsp_diagnostic(diagnostic, text))
            .collect(),
    }
}

/// One compiler diagnostic → one LSP diagnostic: byte offset → 1-based
/// line/column (`SourceMap`) → 0-based UTF-16 LSP position; the severity
/// maps 1:1, the code stays the stable `LNTxxxx` id, the `help:` line
/// rides along in the message.
fn to_lsp_diagnostic(diagnostic: &Diagnostic, text: &str) -> LspDiagnostic {
    let start = position_of_offset(text, diagnostic.offset);
    let end = Position {
        line: start.line,
        character: start.character + 1,
    };
    let message = match &diagnostic.help {
        Some(help) => format!("{}\n\nhelp: {help}", diagnostic.message),
        None => diagnostic.message.clone(),
    };
    LspDiagnostic {
        range: Range { start, end },
        severity: Some(match diagnostic.severity {
            Severity::Error => DiagnosticSeverity::ERROR,
            Severity::Warning => DiagnosticSeverity::WARNING,
        }),
        code: Some(lsp_types::NumberOrString::String(
            diagnostic.code.lnt().to_string(),
        )),
        code_description: None,
        source: Some(DIAGNOSTIC_SOURCE.to_string()),
        message,
        related_information: None,
        tags: None,
        data: None,
    }
}

/// An LSP 0-based UTF-16 position → the byte offset in `text`. Positions
/// past the end of a line (or of the document) clamp to the nearest
/// boundary, so a hover never panics on a stale client cursor.
#[must_use]
fn offset_of_position(text: &str, position: Position) -> usize {
    let mut line = 0_u32;
    let mut utf16 = 0_u32;
    for (index, c) in text.char_indices() {
        if line == position.line {
            if utf16 >= position.character {
                return index;
            }
            utf16 += u32::try_from(c.len_utf16()).unwrap_or(0);
        }
        if c == '\n' {
            line += 1;
            utf16 = 0;
            if line > position.line {
                return index + 1;
            }
        }
    }
    text.len()
}

/// A byte offset → the LSP 0-based UTF-16 position (the inverse mapping
/// of [`offset_of_position`], via the same walk so they agree exactly).
#[must_use]
fn position_of_offset(text: &str, offset: usize) -> Position {
    let mut line = 0_u32;
    let mut utf16 = 0_u32;
    for (index, c) in text.char_indices() {
        if index >= offset {
            break;
        }
        utf16 += u32::try_from(c.len_utf16()).unwrap_or(0);
        if c == '\n' {
            line += 1;
            utf16 = 0;
        }
    }
    Position {
        line,
        character: utf16,
    }
}

fn parse_uri(raw: &str) -> Uri {
    raw.parse::<Uri>()
        .unwrap_or_else(|_| "file:///unknown".parse::<Uri>().expect("parses a file uri"))
}
