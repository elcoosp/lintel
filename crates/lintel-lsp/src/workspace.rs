//! The §17.1-Q3 advanced LSP features over the open workspace:
//! `textDocument/rename`, `textDocument/references` and
//! `workspace/symbol`.
//!
//! The server's workspace is its open documents (`didOpen`/`didChange`
//! sync them) — the same set the diagnostics loop already recompiles, so
//! every query reads the client's live buffers without a filesystem index.
//! That is the honest v0.5 seam (architecture doc §17.1 Q3 leaves the
//! persistent workspace index on the backlog): a reference in a closed
//! file is invisible to all three queries, and `rename` never touches the
//! disk.
//!
//! # What a rename edits
//!
//! A **component** rename (cursor on a declared function name or a
//! component-shaped tag) edits the declaration identifier and every
//! `<Component />` invocation — open *and* close tags — across the open
//! documents. A **prop** rename (cursor on a declaration parameter or a
//! named attribute of a declared component) edits the `$param` in the
//! declaration and every `prop={...}` / `prop="..."` attribute the
//! invocations carry. The target must be declared somewhere in the open
//! workspace (a component whose declaration the server cannot see is not
//! renameable — a partial rename would break the workspace), and the new
//! name must be a valid component tag name / PHP identifier. Tag matches
//! are exact-as-written: `<Ui\Button>` matches only a `Ui\Button`
//! declaration, never a bare `Button`, so the edit set stays predictable.
//!
//! All offsets map through the same UTF-16 positions the diagnostics use,
//! so a client applies the `WorkspaceEdit` against the buffers it synced.

use std::collections::HashMap;

use lsp_types::{
    Location, ReferenceParams, RenameParams, SymbolKind, TextEdit, WorkspaceEdit,
    WorkspaceSymbolParams,
};
use serde_json::{json, Value};

use crate::{
    attribute_region_at, byte_range_location, in_flight_tag, offset_of_position, parse_uri,
    position_of_offset, tag_focus_at, tag_spans, CursorInTag, Document, Server, TagSpan,
};

/// One occurrence of the rename/references target in one open document:
/// the byte span `offset..offset + len` both surfaces share. A prop
/// declaration's span covers the leading `$` (the declaration's `$name`
/// text), so its replacement keeps the sigil; attribute and tag spans
/// replace bare.
struct Hit {
    offset: usize,
    len: usize,
    /// The span carries a leading `$` (a prop declaration) — the rename
    /// writes `$new_name` instead of `new_name`.
    with_dollar: bool,
}

/// The three workspace queries' JSON-RPC arms, collapsed out of `run`'s
/// dispatch (the respond shape repeats per method and `run` stays under
/// its `too_many_lines` budget): parse the params, run the handler, write
/// the response.
pub(crate) fn respond(
    server: &Server,
    method: &str,
    output: &mut dyn std::io::Write,
    id: &Value,
    params: Value,
) {
    match method {
        "textDocument/rename" => {
            crate::respond_query::<RenameParams, WorkspaceEdit>(output, id, params, |p| {
                server.rename(p)
            });
        }
        "textDocument/references" => {
            crate::respond_query::<ReferenceParams, Vec<Location>>(output, id, params, |p| {
                server.references(p)
            });
        }
        _ => {
            crate::respond_query::<WorkspaceSymbolParams, Vec<Value>>(output, id, params, |p| {
                Some(server.workspace_symbols(p))
            });
        }
    }
}

/// What the cursor edits or looks up, resolved against the open
/// workspace's scan tables.
enum Target {
    /// A declared component, by its written name.
    Component(String),
    /// One declared parameter (prop) of a declared component.
    Prop { component: String, prop: String },
}

impl Server {
    /// The `textDocument/rename` result: a `WorkspaceEdit` over the open
    /// documents, or `None` when the cursor is not on a renameable
    /// symbol, the target is undeclared, or `new_name` is invalid (the
    /// spec's empty answer — the client stays free to surface its own).
    #[must_use]
    pub(crate) fn rename(&self, params: &RenameParams) -> Option<WorkspaceEdit> {
        let target = self.target_at(
            &params.text_document_position.text_document.uri.to_string(),
            params.text_document_position.position,
        )?;
        let new_name = params.new_name.as_str();
        let valid = match &target {
            Target::Component(name) => valid_component_name(new_name) && new_name != name,
            Target::Prop { prop, .. } => valid_prop_name(new_name) && new_name != prop,
        };
        if !valid {
            return None;
        }
        // The per-document edit lists are collected keyed by the URI
        // STRING and converted where the `WorkspaceEdit` is built: the
        // intermediate map stays on owned-String keys (a `HashMap<Uri, _>`
        // binding trips clippy::mutable_key_type — `Uri` is
        // interior-mutable — and the conversion happens inside the
        // expression that consumes it).
        let mut changes: Vec<(String, Vec<TextEdit>)> = Vec::new();
        // A rename edits the declaration TOO: the component's identifier
        // or the prop's `$param` — every document that declares the
        // target gets the declaration edit alongside the usage edits.
        for (uri, hits) in self.file_hits(|document| {
            let mut hits = match &target {
                Target::Component(name) => component_hits(document, name),
                Target::Prop { component, prop } => prop_hits(document, component, prop),
            };
            match &target {
                Target::Component(name) => {
                    if let Some(hit) = declaration_hit(document, name) {
                        hits.push(hit);
                    }
                }
                Target::Prop { component, prop } => {
                    if let Some(hit) = prop_declaration_hit(document, component, prop) {
                        hits.push(hit);
                    }
                }
            }
            hits
        }) {
            let document = self.documents.get(&uri)?;
            let edits: Vec<TextEdit> = hits
                .iter()
                .map(|hit| TextEdit {
                    range: range_of(&document.text, hit),
                    new_text: if hit.with_dollar {
                        format!("${new_name}")
                    } else {
                        new_name.to_string()
                    },
                })
                .collect();
            if !edits.is_empty() {
                changes.push((uri, edits));
            }
        }
        Some(WorkspaceEdit {
            changes: Some(
                changes
                    .into_iter()
                    .map(|(uri, edits)| (parse_uri(&uri), edits))
                    .collect(),
            ),
            ..WorkspaceEdit::default()
        })
    }

    /// The `textDocument/references` result: every occurrence of the
    /// component (tag invocations, open and close) or prop (attribute
    /// usages) across the open documents, plus the declaration when
    /// `include_declaration` asks for it. `None` when the cursor is not
    /// on a declared component or prop.
    #[must_use]
    pub(crate) fn references(&self, params: &ReferenceParams) -> Option<Vec<Location>> {
        let uri = params.text_document_position.text_document.uri.to_string();
        let target = self.target_at(&uri, params.text_document_position.position)?;
        let include = params.context.include_declaration;
        let mut locations = Vec::new();
        // A direct walk (not `file_hits`): a document that only DECLARES
        // the target — no usage hits — still contributes its declaration
        // location when `include` asks for it.
        for file_uri in sorted_uris(&self.documents) {
            let document = self.documents.get(&file_uri)?;
            if include {
                if let Some(hit) = match &target {
                    Target::Component(name) => declaration_hit(document, name),
                    Target::Prop { component, prop } => {
                        prop_declaration_hit(document, component, prop)
                    }
                } {
                    locations.push(location_of(&file_uri, &document.text, &hit));
                }
            }
            let hits = match &target {
                Target::Component(name) => component_hits(document, name),
                Target::Prop { component, prop } => prop_hits(document, component, prop),
            };
            for hit in &hits {
                locations.push(location_of(&file_uri, &document.text, hit));
            }
        }
        Some(locations)
    }

    /// The `workspace/symbol` result: every component declared in the
    /// open documents whose name fuzzy-matches the query (a
    /// case-insensitive subsequence — `sal` matches `Salute`; the empty
    /// query lists everything), each as a FUNCTION symbol on the
    /// declaration's identifier span. Documents are visited in URI order,
    /// so the list is stable across identical workspaces.
    #[must_use]
    pub(crate) fn workspace_symbols(&self, params: &WorkspaceSymbolParams) -> Vec<Value> {
        let query = params.query.to_lowercase();
        let mut symbols = Vec::new();
        for uri in sorted_uris(&self.documents) {
            let Some(document) = self.documents.get(&uri) else {
                continue;
            };
            let parsed = lintel_core::scan::scan(&document.text);
            for (name, _signature) in parsed.context.declared_signatures() {
                if !fuzzy_matches(&query, name) {
                    continue;
                }
                let Some(declaration) = parsed.context.declaration_of(name) else {
                    continue;
                };
                symbols.push(json!({
                    "name": name,
                    "kind": SymbolKind::FUNCTION,
                    "location": byte_range_location(
                        &parse_uri(&uri),
                        &document.text,
                        declaration.offset,
                        declaration.name.len(),
                    ),
                }));
            }
        }
        symbols
    }

    /// Resolve what the cursor sits on, against the cursor document's
    /// declaration tables first (a declaration identifier or parameter),
    /// then the tag walk (an invocation name or attribute). The target
    /// must be declared in the open workspace: an undeclared component's
    /// declaration is invisible, so renaming or listing its references
    /// would produce a broken or partial answer.
    fn target_at(&self, uri: &str, position: lsp_types::Position) -> Option<Target> {
        let document = self.documents.get(uri)?;
        let text = &document.text;
        let cursor = offset_of_position(text, position);
        let parsed = lintel_core::scan::scan(text);
        // The declaration side: the identifier span, or a `$param` span
        // (the offset points at the `$`, the span covers the whole
        // `$name`).
        for (name, _signature) in parsed.context.declared_signatures() {
            let Some(declaration) = parsed.context.declaration_of(name) else {
                continue;
            };
            let name_end = declaration.offset + declaration.name.len();
            if cursor >= declaration.offset && cursor <= name_end {
                return Some(Target::Component(name.to_string()));
            }
            for (param, offset) in &declaration.params {
                let end = offset + param.len() + 1;
                if cursor >= *offset && cursor <= end {
                    return Some(Target::Prop {
                        component: name.to_string(),
                        prop: (*param).clone(),
                    });
                }
            }
        }
        // The invocation side: a tag name (open or close) or a written
        // attribute of an open component tag.
        let focus = tag_focus_at(text, cursor).or_else(|| in_flight_tag(text, cursor))?;
        match focus.cursor {
            CursorInTag::Name { .. } if focus.span.is_component() => {
                let name = focus.span.name;
                self.declared_component(name)
                    .then(|| Target::Component(name.to_string()))
            }
            CursorInTag::Attributes { attrs, .. }
                if focus.span.is_component() && !focus.span.closing =>
            {
                let (prop, _) = attrs
                    .iter()
                    .find(|(_, (start, end))| cursor >= *start && cursor <= *end)?;
                let component = focus.span.name;
                self.declared_prop(component, prop).then(|| Target::Prop {
                    component: component.to_string(),
                    prop: (*prop).to_string(),
                })
            }
            _ => None,
        }
    }

    /// Whether `name` is declared as a component in any open document.
    fn declared_component(&self, name: &str) -> bool {
        self.documents.values().any(|document| {
            let parsed = lintel_core::scan::scan(&document.text);
            parsed.context.declaration_of(name).is_some()
        })
    }

    /// Whether some open document declares `prop` as a parameter of the
    /// component `component`.
    fn declared_prop(&self, component: &str, prop: &str) -> bool {
        self.documents.values().any(|document| {
            let parsed = lintel_core::scan::scan(&document.text);
            parsed
                .context
                .declaration_of(component)
                .is_some_and(|declaration| declaration.params.iter().any(|(name, _)| name == prop))
        })
    }

    /// Run `collect` over every open document (URI order), returning the
    /// non-empty per-document hit lists.
    fn file_hits(&self, collect: impl Fn(&Document) -> Vec<Hit>) -> Vec<(String, Vec<Hit>)> {
        let mut out = Vec::new();
        for uri in sorted_uris(&self.documents) {
            let Some(document) = self.documents.get(&uri) else {
                continue;
            };
            let hits = collect(document);
            if !hits.is_empty() {
                out.push((uri, hits));
            }
        }
        out
    }
}

/// The open documents' URIs in sorted order — every workspace walk is
/// deterministic regardless of the document map's hash order.
fn sorted_uris(documents: &HashMap<String, Document>) -> Vec<String> {
    let mut uris: Vec<String> = documents.keys().cloned().collect();
    uris.sort();
    uris
}

/// Every tag-name span of `name` in one document — open *and* close tags,
/// written exactly like the target.
fn component_hits(document: &Document, name: &str) -> Vec<Hit> {
    let text = &document.text;
    tag_spans(text)
        .iter()
        .filter(|span| span.name == name)
        .map(|span| tag_hit(span))
        .collect()
}

/// Every written attribute span named `prop` on an open tag of
/// `component` in one document.
fn prop_hits(document: &Document, component: &str, prop: &str) -> Vec<Hit> {
    let text = &document.text;
    let mut hits = Vec::new();
    for span in tag_spans(text) {
        if span.name != component || span.closing {
            continue;
        }
        // The sentinel cursor (`usize::MAX`) never lands inside a value
        // and never matches the partial/active computations — the
        // classification returns the tag's full attribute list.
        if let CursorInTag::Attributes { attrs, .. } = attribute_region_at(text, &span, usize::MAX)
        {
            for (attr, (start, end)) in attrs {
                if attr == prop {
                    hits.push(Hit {
                        offset: start,
                        len: end - start,
                        with_dollar: false,
                    });
                }
            }
        }
    }
    hits
}

/// The declaration identifier span of `name` in one document, when the
/// document declares it.
fn declaration_hit(document: &Document, name: &str) -> Option<Hit> {
    let parsed = lintel_core::scan::scan(&document.text);
    let declaration = parsed.context.declaration_of(name)?;
    Some(Hit {
        offset: declaration.offset,
        len: declaration.name.len(),
        with_dollar: false,
    })
}

/// The `$param` span of `prop` on a declaration of `component` in one
/// document.
fn prop_declaration_hit(document: &Document, component: &str, prop: &str) -> Option<Hit> {
    let parsed = lintel_core::scan::scan(&document.text);
    let declaration = parsed.context.declaration_of(component)?;
    let (_, offset) = declaration.params.iter().find(|(name, _)| name == prop)?;
    Some(Hit {
        offset: *offset,
        len: prop.len() + 1, // the `$name` span
        with_dollar: true,
    })
}

/// A tag span as a byte-span hit (the tag name inside `<Name` / `</Name`).
fn tag_hit(span: &TagSpan<'_>) -> Hit {
    let (start, end) = span.name_span;
    Hit {
        offset: start,
        len: end - start,
        with_dollar: false,
    }
}

/// The LSP location of one hit in its document.
fn location_of(uri: &str, text: &str, hit: &Hit) -> Location {
    byte_range_location(&parse_uri(uri), text, hit.offset, hit.len)
}

/// The LSP range of one hit in its document (UTF-16 positions).
fn range_of(text: &str, hit: &Hit) -> lsp_types::Range {
    lsp_types::Range {
        start: position_of_offset(text, hit.offset),
        end: position_of_offset(text, hit.offset + hit.len),
    }
}

/// Whether `name` is a valid component tag name: identifier characters
/// and `\` path segments, component-shaped (capitalized first letter or a
/// `\` path — the scanner's own rule).
fn valid_component_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'\\')
        && (name.chars().next().is_some_and(char::is_uppercase) || name.contains('\\'))
}

/// Whether `name` is a valid PHP identifier (a prop name).
fn valid_prop_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    match bytes.next() {
        Some(byte) if byte.is_ascii_alphabetic() || byte == b'_' => {}
        _ => return false,
    }
    bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

/// Case-insensitive subsequence match: every query character appears in
/// `name` in order (`sal` matches `Salute`, `sb` matches `SaluteBadge`).
fn fuzzy_matches(query: &str, name: &str) -> bool {
    let query = query.to_lowercase();
    if query.is_empty() {
        return true;
    }
    let name_lower = name.to_lowercase();
    let mut cursor = 0usize;
    for char in query.chars() {
        match name_lower[cursor..].find(char) {
            Some(index) => cursor += index + char.len_utf8(),
            None => return false,
        }
    }
    true
}
