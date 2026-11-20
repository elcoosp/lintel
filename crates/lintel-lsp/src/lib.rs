//! The `lintel-lsp` language server (architecture doc §15 task-card T-21
//! and the §17.1 Q3 LSP card): `textDocument/publishDiagnostics`,
//! `textDocument/hover`, `textDocument/completion`,
//! `textDocument/definition` and `textDocument/signatureHelp` for
//! `.lintel.php` files, with every compiler position mapped back through
//! the line-preservation guarantee (§5.2) — the byte offsets the compiler
//! anchors diagnostics, declarations and signatures on become 1-based
//! `SourceMap` line/columns, then the LSP's 0-based UTF-16 positions.
//!
//! The server is a plain JSON-RPC loop over any `Read`/`Write` pair
//! (stdio in production, in-memory buffers in the tests): `initialize`
//! advertises full-text sync and the five capabilities, `textDocument/
//! didOpen` / `didChange` / `didSave` recompile the document and publish
//! diagnostics, `hover` over a `<ComponentName>` looks the component's
//! signature up in the same `ScanContext` table the compiler validates
//! against, `completion` offers HTML elements, same-file component names,
//! per-element attributes and signature-driven props, `definition` jumps
//! from a component tag or prop to the declared function (the scanner's
//! `Declaration` position table), `signatureHelp` shows the typed
//! parameter list with the required-`!` marks at a component tag's
//! attribute region or a `(` / `,` call site, and `shutdown`/`exit` end
//! the session. Everything else answers method-not-found, so a client's
//! later capabilities never mislead.
//!
//! # Cross-file signatures
//!
//! Hover, completion and definition resolve **in the hovered file** (the
//! same-file tables the same-file validation rules use). Folding in other
//! files' signatures is the cross-file seam's job
//! ([`lintel_core::scan::ScanContext::merged_with`]); definition positions
//! deliberately do not merge (another file's byte offsets point into *its*
//! source) — cross-file definition is the workspace-index seam's job and
//! stays on the §17.1 Q3 backlog with tree-sitter and rename.

use std::collections::HashMap;
use std::io::{Read, Write};

use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionOptions, CompletionParams,
    Diagnostic as LspDiagnostic, DiagnosticSeverity, DidChangeTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams, Hover, HoverContents, HoverParams,
    InitializeResult, Location, MarkupContent, MarkupKind, ParameterInformation, ParameterLabel,
    Position, PublishDiagnosticsParams, Range, ServerCapabilities, ServerInfo, SignatureHelp,
    SignatureHelpOptions, SignatureHelpParams, SignatureInformation, TextDocumentSyncCapability,
    TextDocumentSyncKind, Uri,
};
use serde_json::{json, Value};

use lintel_core::scan::Signature;
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
            ("textDocument/completion", Some(id)) => {
                let items = serde_json::from_value::<CompletionParams>(params)
                    .ok()
                    .and_then(|params| server.completion(&params));
                write_response(
                    output,
                    &json!({"jsonrpc": "2.0", "id": id, "result": items}),
                );
            }
            ("textDocument/definition", Some(id)) => {
                let location = serde_json::from_value::<lsp_types::GotoDefinitionParams>(params)
                    .ok()
                    .and_then(|params| server.definition(&params));
                write_response(
                    output,
                    &json!({"jsonrpc": "2.0", "id": id, "result": location}),
                );
            }
            ("textDocument/signatureHelp", Some(id)) => {
                let help = serde_json::from_value::<SignatureHelpParams>(params)
                    .ok()
                    .and_then(|params| server.signature_help(&params));
                write_response(output, &json!({"jsonrpc": "2.0", "id": id, "result": help}));
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

/// The `initialize` result: full-text document sync, hover, completion
/// (`<` and space triggers), go-to-definition and signature help (`(`/`,`
/// triggers).
#[must_use]
pub fn initialize_result() -> InitializeResult {
    InitializeResult {
        capabilities: ServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
            hover_provider: Some(lsp_types::HoverProviderCapability::Simple(true)),
            completion_provider: Some(CompletionOptions {
                trigger_characters: Some(vec!["<".to_string(), " ".to_string()]),
                ..CompletionOptions::default()
            }),
            definition_provider: Some(lsp_types::OneOf::Left(true)),
            signature_help_provider: Some(SignatureHelpOptions {
                trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
                ..SignatureHelpOptions::default()
            }),
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

// ---------------------------------------------------------------------------
// Cursor resolution (§17.1 Q3): the editor-cursor logic the completion,
// definition and signature-help handlers share. The compiler's AST anchors
// elements at byte offsets but carries no cursor-level attribute detail,
// and completion must work on an in-flight tag the parser cannot close yet
// — so this is a small text walk, not a second parser: it locates the tag
// the cursor edits, classifies the cursor's position inside it, and the
// handlers turn that into lookups against the same `ScanContext` tables
// the compiler, hover and the inventory use.
// ---------------------------------------------------------------------------

/// The raw-text bodies (§4.7.1) whose `<` are data, not tags — matched
/// exactly like the parser's own `raw_text` check.
const RAW_TEXT_ELEMENTS: &[&str] = &["script", "style"];

/// One tag the walk found, with the spans its handlers need.
struct TagSpan<'t> {
    /// Byte offset of the opening `<` (a close tag: its own `<`).
    open: usize,
    /// Byte offset just past the closing `>`; a raw-text element reports
    /// the span through its matching close tag's `>`, like the compiler's
    /// own element span.
    close: usize,
    /// The tag name as written (`div`, `Card`, `Ui\\Button`).
    name: &'t str,
    /// Byte span of the name inside `<Name` / `</Name`.
    name_span: (usize, usize),
    /// A close tag (`</Name>`).
    closing: bool,
}

impl TagSpan<'_> {
    /// The parser's own component rule (scan.rs): capitalized first letter
    /// or a `\` path segment.
    fn is_component(&self) -> bool {
        self.name.chars().next().is_some_and(char::is_uppercase) || self.name.contains('\\')
    }
}

/// What the cursor edits inside its tag.
enum CursorInTag<'t> {
    /// The tag-name region — `<`, `<C`, `</Ca`; `partial` is the name text
    /// already typed (empty right after `<`).
    Name { partial: &'t str },
    /// The attribute region, outside quotes and braces. `attrs` are the
    /// written attribute names with their byte spans in source order;
    /// `attrs_before` counts the ones fully before the cursor (the
    /// signature-help active-parameter index); `partial` is the trailing
    /// name fragment under the cursor.
    Attributes {
        attrs: Vec<(&'t str, (usize, usize))>,
        attrs_before: usize,
        partial: Option<&'t str>,
    },
    /// Inside a quoted attribute value or a `{expr}` — completion stays
    /// silent (signature help may still resolve a call inside the braces).
    InValue,
}

/// The tag the cursor edits and how the cursor sits in it.
struct TagFocus<'t> {
    span: TagSpan<'t>,
    cursor: CursorInTag<'t>,
}

/// Collect every tag span in `text` the compiler would parse as a tag.
/// Outside a tag, PHP strings, comments and heredocs are data; inside a
/// tag's attribute region, quotes and `{expr}` braces are tracked so a `>`
/// or `<` inside a value (`title="a>b"`, `cond={a > b}`) cannot split the
/// span. Raw-text elements report one span through their matching close
/// tag, the way the compiler's scanner reads their bodies verbatim.
fn tag_spans(text: &str) -> Vec<TagSpan<'_>> {
    let b = text.as_bytes();
    let mut spans = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\'' | b'"' | b'`' => i = skip_quoted(b, i),
            b'/' if b.get(i + 1) == Some(&b'/') || b.get(i + 1) == Some(&b'#') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(b.len());
            }
            b'<' if b.len() > i + 4 && &b[i..i + 3] == b"<<<" => {
                i = skip_heredoc(b, i);
            }
            b'<' if b.get(i + 1).is_some_and(u8::is_ascii_alphabetic) => {
                let name_start = i + 1;
                let name_end = take_tag_name_end(b, name_start);
                let name = &text[name_start..name_end];
                let (close, self_closing) = skip_attr_region(b, name_end);
                let raw = RAW_TEXT_ELEMENTS.contains(&name) && !self_closing;
                spans.push(TagSpan {
                    open: i,
                    close,
                    name,
                    name_span: (name_start, name_end),
                    closing: false,
                });
                if raw {
                    i = skip_raw_body(b, close, name);
                } else {
                    i = close;
                }
            }
            b'<' if b.get(i + 1) == Some(&b'/') => {
                let name_start = i + 2;
                let name_end = take_tag_name_end(b, name_start);
                if name_end == name_start {
                    i += 2;
                    continue;
                }
                let name = &text[name_start..name_end];
                let (close, _self_closing) = skip_attr_region(b, name_end);
                spans.push(TagSpan {
                    open: i,
                    close,
                    name,
                    name_span: (name_start, name_end),
                    closing: true,
                });
                i = close;
            }
            _ => i += 1,
        }
    }
    spans
}

/// The end of the tag name starting at `start`: identifier characters and
/// `\` path segments, mirroring the parser's tag-name read.
fn take_tag_name_end(b: &[u8], start: usize) -> usize {
    let mut end = start;
    while end < b.len() && (b[end].is_ascii_alphanumeric() || b[end] == b'_' || b[end] == b'\\') {
        end += 1;
    }
    end
}

/// Walk the attribute region from `start` to the tag's closing `>` (at
/// top level: not inside a quoted value or `{expr}` braces). Returns the
/// offset just past the `>` and whether the tag is self-closing (`/>`).
fn skip_attr_region(b: &[u8], start: usize) -> (usize, bool) {
    let mut i = start;
    let mut brace_depth = 0usize;
    while i < b.len() {
        let c = b[i];
        match c {
            b'\'' | b'"' if brace_depth == 0 => i = skip_quoted(b, i),
            b'{' => {
                brace_depth += 1;
                i += 1;
            }
            b'}' => {
                brace_depth = brace_depth.saturating_sub(1);
                i += 1;
            }
            b'\'' | b'"' => i = skip_quoted(b, i),
            b'>' if brace_depth == 0 => {
                return (i + 1, i > start && b[i - 1] == b'/');
            }
            _ => i += 1,
        }
    }
    (b.len(), false)
}

/// A raw-text body (§4.7.1): verbatim from just past the open tag's `>` to
/// the matching lowercase close tag. Unmatched: the end of the file.
fn skip_raw_body(b: &[u8], body_start: usize, name: &str) -> usize {
    let mut needle = Vec::with_capacity(name.len() + 3);
    needle.extend_from_slice(b"</");
    needle.extend_from_slice(name.as_bytes());
    let mut i = body_start;
    while i + needle.len() <= b.len() {
        if b[i..i + needle.len()] == needle[..] {
            return skip_attr_region(b, i + needle.len()).0;
        }
        i += 1;
    }
    b.len()
}

/// A PHP single/double/backtick string starting at the quote at `start`:
/// the offset just past its closing quote (escape-aware). Unterminated:
/// the end of the input.
fn skip_quoted(b: &[u8], start: usize) -> usize {
    let quote = b[start];
    let mut i = start + 1;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 1,
            c if c == quote => return i + 1,
            _ => {}
        }
        i += 1;
    }
    b.len()
}

/// A PHP heredoc/nowdoc starting at the `<<<` at `start`: the offset just
/// past the terminator line. Unterminated: the end of the input.
fn skip_heredoc(b: &[u8], start: usize) -> usize {
    let mut i = start + 3;
    while i < b.len() && (b[i] == b'"' || b[i] == b'\'') {
        i += 1;
    }
    let id_start = i;
    while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
        i += 1;
    }
    if i == id_start {
        return start + 3;
    }
    let id = &b[id_start..i];
    while i < b.len() && b[i] != b'\n' {
        i += 1;
    }
    while i < b.len() {
        let line_start = i;
        while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
            i += 1;
        }
        if b[i..].starts_with(id) {
            let mut j = i + id.len();
            while j < b.len() && b[j] != b'\n' {
                j += 1;
            }
            return j.min(b.len());
        }
        i = line_start;
        while i < b.len() && b[i] != b'\n' {
            i += 1;
        }
        if i < b.len() {
            i += 1;
        }
    }
    b.len()
}

/// Classify the cursor inside the tag span that contains it. A span whose
/// walk reached the end of the input without a closing `>` (an in-flight
/// tag) contains a cursor sitting at end-of-input, so completion works
/// mid-edit; a bare trailing `<` with no name yet (or a `</`) is resolved
/// by [`in_flight_tag`].
fn tag_focus_at(text: &str, cursor: usize) -> Option<TagFocus<'_>> {
    let span = tag_spans(text).into_iter().find(|span| {
        span.open <= cursor
            && (cursor < span.close
                || (cursor == span.close && span.close == text.len() && !span.closing))
    })?;
    let (name_start, name_end) = span.name_span;
    if !span.closing && cursor <= name_end {
        let partial = if cursor >= name_start {
            &text[name_start..cursor]
        } else {
            ""
        };
        return Some(TagFocus {
            span,
            cursor: CursorInTag::Name { partial },
        });
    }
    if span.closing {
        if cursor <= name_end {
            return Some(TagFocus {
                span,
                cursor: CursorInTag::Name {
                    partial: if cursor >= name_start {
                        &text[name_start..cursor]
                    } else {
                        ""
                    },
                },
            });
        }
        return None;
    }
    // The attribute region: walk it once, collecting names and state.
    let cursor_in_tag = attribute_region_at(text, &span, cursor);
    if matches!(cursor_in_tag, CursorInTag::InValue) {
        return Some(TagFocus {
            span,
            cursor: CursorInTag::InValue,
        });
    }
    Some(TagFocus {
        span,
        cursor: cursor_in_tag,
    })
}

/// The cursor's classification inside an open tag's attribute region:
/// every written attribute name with its span (a prop definition target),
/// the count fully before the cursor (the signature-help active-parameter
/// index), the trailing fragment under the cursor (a completion filter) —
/// or [`CursorInTag::InValue`] inside a quoted value or `{expr}`.
fn attribute_region_at<'t>(text: &'t str, span: &TagSpan<'_>, cursor: usize) -> CursorInTag<'t> {
    let bytes = text.as_bytes();
    let region_end = span.close.saturating_sub(1); // at the closing `>`
    let mut attrs: Vec<(&str, (usize, usize))> = Vec::new();
    let mut attrs_before = 0usize;
    let mut partial = None;
    let mut index = span.name_span.1;
    while index < region_end {
        let byte = bytes[index];
        if byte == b'{' && cursor <= index {
            break; // nothing further is written before the cursor
        }
        if byte.is_ascii_whitespace() {
            index += 1;
            continue;
        }
        if byte == b'{' {
            // The `{expr}` value: skip it wholesale; the cursor inside is
            // InValue (signature help re-resolves calls separately).
            let after = skip_braced(bytes, index, region_end);
            if cursor > index && cursor < after {
                return CursorInTag::InValue;
            }
            index = after;
            continue;
        }
        if byte == b'=' || byte == b'/' {
            index += 1;
            continue;
        }
        if byte == b'\'' || byte == b'"' || byte == b'`' {
            let after = skip_quoted(bytes, index);
            if cursor > index && cursor < after {
                return CursorInTag::InValue;
            }
            index = after;
            continue;
        }
        // An attribute name (HTML names carry `-`, `:`, `@`; component
        // props are PHP identifiers — the union reads both).
        let name_start = index;
        while index < region_end
            && (bytes[index].is_ascii_alphanumeric()
                || matches!(bytes[index], b'_' | b'-' | b':' | b'@' | b'\\'))
        {
            index += 1;
        }
        if index == name_start {
            index += 1;
            continue;
        }
        if cursor >= name_start && cursor <= index {
            partial = Some(&text[name_start..cursor]);
        }
        if index <= cursor {
            attrs_before += 1;
        }
        attrs.push((&text[name_start..index], (name_start, index)));
        // Skip an attached value so its quotes/braces cannot confuse the
        // next name read (`title="a"` — the `=` lands here, the value is
        // skipped on the next iterations).
        let probe = index;
        let mut probe = probe;
        while probe < region_end && bytes[probe].is_ascii_whitespace() {
            probe += 1;
        }
        if probe < region_end && bytes[probe] == b'=' {
            index = probe + 1;
        }
    }
    CursorInTag::Attributes {
        attrs,
        attrs_before,
        partial,
    }
}

/// Skip a `{expr}` span from the `{` at `start` (nesting- and
/// quote-aware): the offset just past its matching `}`, or the region end.
fn skip_braced(bytes: &[u8], start: usize, region_end: usize) -> usize {
    let mut depth = 1usize;
    let mut probe = start + 1;
    let mut in_quote: Option<u8> = None;
    while probe < region_end && depth > 0 {
        let byte = bytes[probe];
        match byte {
            q if Some(q) == in_quote => in_quote = None,
            q if in_quote.is_none() && (q == b'\'' || q == b'"' || q == b'`') => {
                in_quote = Some(q);
            }
            b'{' if in_quote.is_none() => depth += 1,
            b'}' if in_quote.is_none() => depth -= 1,
            _ => {}
        }
        probe += 1;
    }
    probe
}

/// The in-flight tag a trailing cursor is typing: `<` (or `</`) followed
/// only by identifier characters up to the cursor — the bare-`<` completion
/// trigger before any name exists. The run is pure identifiers, so no `>`
/// can sit between the `<` and the cursor.
fn in_flight_tag(text: &str, cursor: usize) -> Option<TagFocus<'_>> {
    let b = text.as_bytes();
    if cursor == 0 {
        return None;
    }
    let run_start = identifier_run_before(b, cursor);
    let mut lt = run_start;
    let closing = if lt > 1 && b[lt - 1] == b'/' && b[lt - 2] == b'<' {
        lt -= 2;
        true
    } else if lt > 0 && b[lt - 1] == b'<' {
        lt -= 1;
        false
    } else {
        return None;
    };
    let name = &text[run_start..cursor];
    Some(TagFocus {
        span: TagSpan {
            open: lt,
            close: cursor,
            name,
            name_span: (run_start, cursor),
            closing,
        },
        cursor: CursorInTag::Name { partial: name },
    })
}

/// The start of the identifier run ending at `cursor` (the in-flight tag
/// name's first byte, or the cursor itself when nothing identifier-shaped
/// precedes it).
fn identifier_run_before(bytes: &[u8], cursor: usize) -> usize {
    let mut start = cursor;
    while start > 0
        && (bytes[start - 1].is_ascii_alphanumeric()
            || bytes[start - 1] == b'_'
            || bytes[start - 1] == b'\\')
    {
        start -= 1;
    }
    start
}

/// The PHP call form under the cursor — `Name(` … `,` … — as
/// `(name, top-level commas since the open paren)`. Walks back to the
/// nearest statement-unbounded `(` (quoted spans are skipped whole), then
/// counts the separators between it and the cursor the same way. This is
/// the `(`/`,` trigger shape inside a `{expr}` child.
fn call_at(text: &str, cursor: usize) -> Option<(&str, usize)> {
    let b = text.as_bytes();
    let mut depth = 0usize;
    let mut paren = None;
    let mut i = cursor;
    while i > 0 {
        i -= 1;
        match b[i] {
            b')' => depth += 1,
            b'(' => {
                if depth == 0 {
                    paren = Some(i);
                    break;
                }
                depth -= 1;
            }
            b';' | b'{' | b'}' if depth == 0 => return None,
            b'\'' | b'"' | b'`' => {
                // Walk back out of the quoted span (best-effort: proper
                // pairing, escapes untracked — a stray backslash inside a
                // string only shifts the skip, never the result).
                let mut j = i;
                while j > 0 && b[j - 1] != b[i] {
                    j -= 1;
                }
                i = if j > 0 { j - 1 } else { 0 };
            }
            _ => {}
        }
    }
    let paren = paren?;
    let mut name_start = paren;
    while name_start > 0
        && (b[name_start - 1].is_ascii_alphanumeric()
            || b[name_start - 1] == b'_'
            || b[name_start - 1] == b'\\')
    {
        name_start -= 1;
    }
    if name_start == paren {
        return None;
    }
    let name = &text[name_start..paren];
    let mut separators = 0usize;
    let mut nested = 0usize;
    let mut j = paren + 1;
    while j < cursor.min(b.len()) {
        match b[j] {
            b'\'' | b'"' | b'`' => j = skip_quoted(b, j),
            b'(' | b'[' | b'{' => nested += 1,
            b')' | b']' | b'}' => nested = nested.saturating_sub(1),
            b',' if nested == 0 => separators += 1,
            _ => {}
        }
        j += 1;
    }
    Some((name, separators))
}

/// The `SignatureHelp` for one component: the typed parameter list with
/// the required-`!` marks the signature scanner computed, each parameter
/// labeled by its span within the label, `active` clamped into range.
fn component_signature_help(signature: &Signature, name: &str, active: u32) -> SignatureHelp {
    let mut label = format!("function {name}(");
    let mut parameters = Vec::new();
    for (index, param) in signature.params.iter().enumerate() {
        let tail = if signature.variadic && index == signature.params.len() - 1 {
            "…"
        } else if signature.required.contains(param) {
            "!"
        } else {
            ""
        };
        let start = u32::try_from(label.len()).unwrap_or(u32::MAX);
        label.push_str(param);
        label.push_str(tail);
        let end = u32::try_from(label.len()).unwrap_or(u32::MAX);
        parameters.push(ParameterInformation {
            label: ParameterLabel::LabelOffsets([start, end]),
            documentation: None,
        });
        if index + 1 < signature.params.len() {
            label.push_str(", ");
        }
    }
    label.push(')');
    let max = u32::try_from(signature.params.len().saturating_sub(1)).unwrap_or(0);
    let active = active.min(max);
    SignatureHelp {
        signatures: vec![SignatureInformation {
            label,
            documentation: None,
            parameters: Some(parameters),
            active_parameter: Some(active),
        }],
        active_signature: Some(0),
        active_parameter: Some(active),
    }
}

/// The completion `detail` line for one declared component: its parameter
/// list with the required-`!` marks, the same facts hover renders.
fn component_detail(name: &str, signature: &Signature) -> String {
    let params = signature
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
            format!("{param}{tail}")
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("component: function {name}({params})")
}

/// A byte offset + length in `text` → a same-document `Location`, mapped
/// through the shared offset→UTF-16 walk so the start and end positions
/// agree exactly with every other mapping in the server.
fn byte_range_location(uri: &Uri, text: &str, offset: usize, len: usize) -> Location {
    Location {
        uri: uri.clone(),
        range: Range {
            start: position_of_offset(text, offset),
            end: position_of_offset(text, offset + len),
        },
    }
}

// ---------------------------------------------------------------------------
// The HTML completion data (§4.2 elements): presentation-only tables the
// completion handler reads. The compiler validates attribute *shapes*
// (§7.1's URL allowlist, event-handler and named-argument rules) but
// carries no per-element attribute whitelist — HTML-validity lints are the
// §17.1 Q3 backlog — so these tables never gate compilation; they curate
// the common HTML Living Standard subset for the editor.
// ---------------------------------------------------------------------------

/// The common HTML elements a Lintel template reaches for.
const HTML_ELEMENTS: &[&str] = &[
    "a",
    "abbr",
    "address",
    "article",
    "aside",
    "b",
    "blockquote",
    "body",
    "br",
    "button",
    "caption",
    "code",
    "col",
    "colgroup",
    "dd",
    "del",
    "details",
    "dialog",
    "div",
    "dl",
    "dt",
    "em",
    "embed",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "head",
    "header",
    "hr",
    "html",
    "i",
    "iframe",
    "img",
    "input",
    "ins",
    "label",
    "legend",
    "li",
    "link",
    "main",
    "mark",
    "meta",
    "meter",
    "nav",
    "noscript",
    "object",
    "ol",
    "optgroup",
    "option",
    "output",
    "p",
    "picture",
    "pre",
    "progress",
    "q",
    "script",
    "section",
    "select",
    "slot",
    "small",
    "source",
    "span",
    "strong",
    "style",
    "sub",
    "summary",
    "sup",
    "table",
    "tbody",
    "td",
    "template",
    "textarea",
    "tfoot",
    "th",
    "thead",
    "time",
    "title",
    "tr",
    "track",
    "u",
    "ul",
    "video",
];

/// The global attributes every element accepts.
const GLOBAL_ATTRS: &[&str] = &[
    "class",
    "id",
    "style",
    "title",
    "hidden",
    "lang",
    "dir",
    "tabindex",
    "draggable",
    "role",
];

/// Element-specific attributes (the common subset).
const ELEMENT_ATTRS: &[(&str, &[&str])] = &[
    (
        "a",
        &["href", "target", "rel", "download", "hreflang", "type"],
    ),
    (
        "img",
        &[
            "src", "alt", "width", "height", "loading", "decoding", "srcset", "sizes",
        ],
    ),
    (
        "input",
        &[
            "type",
            "name",
            "value",
            "placeholder",
            "required",
            "disabled",
            "readonly",
            "checked",
            "min",
            "max",
            "step",
            "pattern",
            "autocomplete",
        ],
    ),
    ("button", &["type", "name", "value", "disabled", "form"]),
    (
        "form",
        &[
            "action",
            "method",
            "enctype",
            "target",
            "novalidate",
            "name",
        ],
    ),
    ("label", &["for"]),
    (
        "select",
        &["name", "required", "disabled", "multiple", "size"],
    ),
    ("option", &["value", "selected", "disabled"]),
    ("optgroup", &["label", "disabled"]),
    (
        "textarea",
        &[
            "name",
            "rows",
            "cols",
            "placeholder",
            "required",
            "disabled",
            "readonly",
        ],
    ),
    ("td", &["colspan", "rowspan", "headers"]),
    ("th", &["colspan", "rowspan", "scope", "headers"]),
    ("ol", &["start", "reversed", "type"]),
    ("li", &["value"]),
    ("meta", &["charset", "content", "name", "http-equiv"]),
    ("link", &["rel", "href", "type", "media", "as"]),
    (
        "script",
        &["src", "async", "defer", "type", "integrity", "crossorigin"],
    ),
    ("style", &["media", "type"]),
    (
        "video",
        &[
            "src", "controls", "autoplay", "loop", "muted", "poster", "width", "height", "preload",
        ],
    ),
    ("source", &["src", "srcset", "type", "media", "sizes"]),
    ("track", &["src", "kind", "srclang", "label", "default"]),
    ("time", &["datetime"]),
    ("del", &["cite", "datetime"]),
    ("ins", &["cite", "datetime"]),
    ("q", &["cite"]),
    ("blockquote", &["cite"]),
    ("details", &["open"]),
    ("dialog", &["open"]),
    ("canvas", &["width", "height"]),
    (
        "iframe",
        &[
            "src", "width", "height", "title", "loading", "allow", "sandbox", "srcdoc",
        ],
    ),
    ("embed", &["src", "type", "width", "height"]),
    ("object", &["data", "type", "width", "height", "name"]),
    ("meter", &["value", "min", "max", "low", "high", "optimum"]),
    ("progress", &["value", "max"]),
];

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

    /// The completion list for `params`: HTML element names and same-file
    /// component names at a tag-name position (`<`, `<Ca`), attribute names
    /// per element and prop names from the scanned signature at an
    /// attribute position (after a tag name's space), nothing inside a
    /// quoted value or `{expr}`. Components come from the same
    /// `ScanContext` table the inventory's `declared` list is built from;
    /// an attribute position on a component without a same-file signature
    /// offers nothing (its props are unknowable here).
    #[must_use]
    fn completion(&self, params: &CompletionParams) -> Option<Vec<CompletionItem>> {
        let uri = params.text_document_position.text_document.uri.to_string();
        let document = self.documents.get(&uri)?;
        let cursor = offset_of_position(&document.text, params.text_document_position.position);
        let focus = tag_focus_at(&document.text, cursor)
            .or_else(|| in_flight_tag(&document.text, cursor))?;
        let parsed = lintel_core::scan::scan(&document.text);
        Some(match focus.cursor {
            CursorInTag::Name { partial } => {
                let mut items = Vec::new();
                for element in HTML_ELEMENTS {
                    if element.starts_with(partial) {
                        items.push(CompletionItem {
                            label: (*element).to_string(),
                            kind: Some(CompletionItemKind::KEYWORD),
                            detail: Some("HTML element".to_string()),
                            ..CompletionItem::default()
                        });
                    }
                }
                for (name, signature) in parsed.context.declared_signatures() {
                    if name.starts_with(partial) {
                        items.push(CompletionItem {
                            label: name.to_string(),
                            kind: Some(CompletionItemKind::FUNCTION),
                            detail: Some(component_detail(name, signature)),
                            ..CompletionItem::default()
                        });
                    }
                }
                items
            }
            CursorInTag::InValue => return None,
            CursorInTag::Attributes { attrs, partial, .. } => {
                let written: Vec<&str> = attrs.iter().map(|(name, _)| *name).collect();
                if focus.span.is_component() {
                    // Props from the scanned signature; `children` is JSX
                    // children (§4.7), not an attribute; already-passed
                    // props are filtered the same way the compiler's
                    // LNT0205 check counts them.
                    let signature = parsed.context.signature_of(focus.span.name)?;
                    signature
                        .params
                        .iter()
                        .filter(|param| param.as_str() != "children")
                        .filter(|param| !written.contains(&param.as_str()))
                        .filter(|param| partial.map_or(true, |prefix| param.starts_with(prefix)))
                        .map(|param| CompletionItem {
                            label: param.clone(),
                            kind: Some(CompletionItemKind::PROPERTY),
                            detail: Some(if signature.required.contains(param) {
                                "required prop".to_string()
                            } else {
                                "prop".to_string()
                            }),
                            ..CompletionItem::default()
                        })
                        .collect()
                } else {
                    let element_attrs = ELEMENT_ATTRS
                        .iter()
                        .find(|(element, _)| *element == focus.span.name)
                        .map_or(&[][..], |(_, attrs)| *attrs);
                    element_attrs
                        .iter()
                        .chain(GLOBAL_ATTRS.iter())
                        .filter(|attr| !written.contains(attr))
                        .filter(|attr| partial.map_or(true, |prefix| attr.starts_with(prefix)))
                        .map(|attr| CompletionItem {
                            label: (*attr).to_string(),
                            kind: Some(CompletionItemKind::PROPERTY),
                            detail: Some("attribute".to_string()),
                            ..CompletionItem::default()
                        })
                        .collect()
                }
            }
        })
    }

    /// The go-to-definition target for `params`: a component tag (open or
    /// close) resolves to the declared function's name span; a prop of a
    /// component tag resolves to the parameter it names — both through the
    /// scanner's `Declaration` table (the same walk that fills the
    /// signature table), mapped through the line-preservation guarantee.
    /// Same-file only: another file's offsets point into *its* source, and
    /// cross-file definition is the workspace-index seam's job (§17.1 Q3
    /// backlog).
    #[must_use]
    fn definition(&self, params: &lsp_types::GotoDefinitionParams) -> Option<Location> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .to_string();
        let document = self.documents.get(&uri)?;
        let cursor = offset_of_position(
            &document.text,
            params.text_document_position_params.position,
        );
        let focus = tag_focus_at(&document.text, cursor)
            .or_else(|| in_flight_tag(&document.text, cursor))?;
        let parsed = lintel_core::scan::scan(&document.text);
        match focus.cursor {
            CursorInTag::Name { .. } if focus.span.is_component() => {
                let declaration = parsed.context.declaration_of(focus.span.name)?;
                Some(byte_range_location(
                    &parse_uri(&uri),
                    &document.text,
                    declaration.offset,
                    declaration.name.len(),
                ))
            }
            CursorInTag::Attributes { attrs, .. } if focus.span.is_component() => {
                let declaration = parsed.context.declaration_of(focus.span.name)?;
                // The attribute name under the cursor resolves to the
                // parameter it names (PHP named arguments are
                // case-sensitive, §4.7 — the match is too).
                let (prop, _) = attrs
                    .iter()
                    .find(|(_, (start, end))| cursor >= *start && cursor <= *end)?;
                let (param, offset) = declaration.params.iter().find(|(name, _)| name == prop)?;
                Some(byte_range_location(
                    &parse_uri(&uri),
                    &document.text,
                    *offset,
                    param.len() + 1, // the `$name` span
                ))
            }
            _ => None,
        }
    }

    /// The signature help for `params`: the typed parameter list with the
    /// required-`!` marks the signature scanner computed. Two trigger
    /// shapes — the attribute region of a component invocation (the tag's
    /// attribute list **is** the named-argument list, §4.7; the active
    /// parameter advances per written attribute) and the PHP call form
    /// inside a `{expr}` child, where `(` and `,` are the spec'd trigger
    /// characters and the active parameter advances per top-level comma.
    #[must_use]
    fn signature_help(&self, params: &SignatureHelpParams) -> Option<SignatureHelp> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .to_string();
        let document = self.documents.get(&uri)?;
        let text = &document.text;
        let cursor = offset_of_position(text, params.text_document_position_params.position);
        let parsed = lintel_core::scan::scan(text);
        if let Some(focus) = tag_focus_at(text, cursor).or_else(|| in_flight_tag(text, cursor)) {
            if !focus.span.closing {
                if let (CursorInTag::Attributes { attrs_before, .. }, Some(signature)) =
                    (&focus.cursor, parsed.context.signature_of(focus.span.name))
                {
                    if focus.span.is_component() {
                        let active = u32::try_from(*attrs_before).unwrap_or(0);
                        return Some(component_signature_help(signature, focus.span.name, active));
                    }
                }
            }
        }
        let (name, arg_index) = call_at(text, cursor)?;
        let signature = parsed.context.signature_of(name)?;
        let active = u32::try_from(arg_index).unwrap_or(0);
        Some(component_signature_help(signature, name, active))
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
