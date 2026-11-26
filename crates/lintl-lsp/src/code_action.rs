//! The `textDocument/codeAction` lane (§17.1 Q3+): two refactor/quickfix
//! actions over the same `ScanContext`/inventory tables the rest of the
//! server reads.
//!
//! **"Extract to Lintl component"** — the selection is a JSX subtree, so
//! the action creates a sibling `.lintl.php` declaring a component for
//! it and replaces the selection with the invocation. The free `$var`
//! references the selection makes become the component's parameters, and
//! the invocation passes them back (`<Extracted var={$var} />`). The
//! component name is the stable placeholder `Extracted` — renaming is the
//! client's one extra keystroke, and a deterministic name keeps the
//! action's preview exact. The would-be component must compile before the
//! action is offered: a selection the compiler rejects is not offered as
//! a refactor that would ship the same rejection.
//!
//! **"Add missing required props"** — the cursor sits on a component
//! invocation whose same-file signature declares required params the tag
//! does not pass, so the action inserts them with `""` placeholders
//! right before the tag's `>`.
//!
//! Both return plain [`CodeAction`]s (no commands): the whole edit ships
//! in the response, so the client previews it without a round-trip.

use std::collections::HashMap;

use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, CreateFile,
    DocumentChangeOperation, DocumentChanges, OneOf, OptionalVersionedTextDocumentIdentifier,
    Range, ResourceOp, TextDocumentEdit, TextEdit, Uri, WorkspaceEdit,
};

use lintl_core::Options;

/// The placeholder component name the extract action declares.
const EXTRACTED_NAME: &str = "Extracted";

/// The code actions for `params` against the open document `text` at
/// `uri`: the extract refactor when the selection is a JSX subtree, the
/// quickfix when the cursor sits on a component invocation missing
/// required props. Both are computed independently — a client can show
/// either or both.
#[must_use]
pub fn code_actions(text: &str, uri: &Uri, params: &CodeActionParams) -> Vec<CodeActionOrCommand> {
    let mut actions = Vec::new();
    if let Some(action) = extract_action(text, uri, params) {
        actions.push(CodeActionOrCommand::CodeAction(action));
    }
    if let Some(action) = add_missing_props_action(text, uri, params) {
        actions.push(CodeActionOrCommand::CodeAction(action));
    }
    actions
}

/// The extract action, or `None` when the selection is not a JSX subtree
/// (it must start with `<` and end with `>` after trimming, carry no PHP
/// — `<?php`/`?>` — because the extracted body must compile, and the
/// would-be component must itself compile).
fn extract_action(text: &str, uri: &Uri, params: &CodeActionParams) -> Option<CodeAction> {
    let start = crate::offset_of_position(text, params.range.start);
    let end = crate::offset_of_position(text, params.range.end);
    if end <= start {
        return None;
    }
    let selected = text[start..end].trim();
    if !(selected.starts_with('<') && selected.ends_with('>')) {
        return None;
    }
    if selected.contains("<?php") || selected.contains("?>") {
        return None;
    }
    // The selection must be a complete element: its first `<` closes.
    if !selected[1..].contains('>') {
        return None;
    }

    let props = free_variables(selected);
    let invocation = if props.is_empty() {
        format!("<{EXTRACTED_NAME} />")
    } else {
        format!(
            "<{EXTRACTED_NAME} {} />",
            props
                .iter()
                .map(|prop| format!("{prop}={{${prop}}}"))
                .collect::<Vec<_>>()
                .join(" ")
        )
    };
    let parameters = props
        .iter()
        .map(|prop| format!("${prop}"))
        .collect::<Vec<_>>()
        .join(", ");
    let body = format!(
        "<?php\n\nfunction {EXTRACTED_NAME}({parameters}) {{\n    return {selected};\n}}\n"
    );
    // The refactor only offers what compiles — the same gate the
    // diagnostics lane already runs on every keystroke.
    if lintl_core::compile(&body, &Options::default()).is_err() {
        return None;
    }

    let new_uri = sibling_uri(uri, &format!("{EXTRACTED_NAME}.lintl.php"))?;
    let edit = WorkspaceEdit {
        changes: None,
        document_changes: Some(DocumentChanges::Operations(vec![
            DocumentChangeOperation::Op(ResourceOp::Create(CreateFile {
                uri: new_uri.clone(),
                options: None,
                annotation_id: None,
            })),
            DocumentChangeOperation::Edit(TextDocumentEdit {
                text_document: OptionalVersionedTextDocumentIdentifier {
                    uri: new_uri,
                    version: None,
                },
                edits: vec![OneOf::Left(TextEdit {
                    range: Range {
                        start: lsp_types::Position {
                            line: 0,
                            character: 0,
                        },
                        end: lsp_types::Position {
                            line: 0,
                            character: 0,
                        },
                    },
                    new_text: body,
                })],
            }),
            DocumentChangeOperation::Edit(TextDocumentEdit {
                text_document: OptionalVersionedTextDocumentIdentifier {
                    uri: uri.clone(),
                    version: None,
                },
                edits: vec![OneOf::Left(TextEdit {
                    range: params.range,
                    new_text: invocation,
                })],
            }),
        ])),
        change_annotations: None,
    };
    Some(CodeAction {
        title: "Extract to Lintl component".to_string(),
        kind: Some(CodeActionKind::REFACTOR_EXTRACT),
        edit: Some(edit),
        ..CodeAction::default()
    })
}

/// The add-missing-props quickfix, or `None` when the cursor is not on a
/// component invocation, the component has no same-file signature, or
/// every required prop is already passed.
fn add_missing_props_action(
    text: &str,
    uri: &Uri,
    params: &CodeActionParams,
) -> Option<CodeAction> {
    let cursor = crate::offset_of_position(text, params.range.start);
    let parsed = lintl_core::scan::scan(text);
    let inventory = lintl_core::collect_inventory(&parsed.roots, &parsed.context);
    let site = inventory.invocations.iter().find(|site| {
        // From the `<` through the name — the client puts the cursor at
        // (or inside) the tag it edits.
        let end = site.offset + 1 + site.name.len();
        cursor >= site.offset && cursor <= end
    })?;
    if !site.declared_in_file {
        return None;
    }
    let signature = parsed.context.signature_of(&site.name)?;
    let missing: Vec<&String> = signature
        .required
        .iter()
        .filter(|prop| !site.props.contains(prop))
        .collect();
    if missing.is_empty() {
        return None;
    }

    // Insert right before the open tag's `>` (before a self-closing `/`).
    let spans = crate::tag_spans(text);
    let span = spans.iter().find(|span| span.open == site.offset)?;
    // A self-closing tag inserts before the `/`; a plain tag, before the `>`.
    let insert_at = if text.as_bytes()[span.close - 2] == b'/' {
        span.close - 2
    } else {
        span.close - 1
    };
    let new_text = format!(
        " {}",
        missing
            .iter()
            .map(|prop| format!("{prop}=\"\""))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let position = crate::position_of_offset(text, insert_at);
    let edit = WorkspaceEdit {
        changes: Some(HashMap::from([(
            uri.clone(),
            vec![TextEdit {
                range: Range {
                    start: position,
                    end: position,
                },
                new_text,
            }],
        )])),
        document_changes: None,
        change_annotations: None,
    };
    Some(CodeAction {
        title: format!(
            "Add missing required props: {}",
            missing
                .iter()
                .map(|prop| prop.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        kind: Some(CodeActionKind::QUICKFIX),
        edit: Some(edit),
        ..CodeAction::default()
    })
}

/// The free `$var` identifiers `selected` references, deduplicated in
/// source order (the invocation's prop list and the component's
/// parameter list share this order, so the round-trip is stable).
fn free_variables(selected: &str) -> Vec<String> {
    let b = selected.as_bytes();
    let mut props = Vec::new();
    let mut index = 0;
    while index < b.len() {
        if b[index] == b'$'
            && b.get(index + 1)
                .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
        {
            let start = index + 1;
            let mut end = start;
            while end < b.len() && (b[end].is_ascii_alphanumeric() || b[end] == b'_') {
                end += 1;
            }
            let name = &selected[start..end];
            if !props.iter().any(|prop| prop == name) {
                props.push(name.to_string());
            }
            index = end;
        } else {
            index += 1;
        }
    }
    props
}

/// The sibling file URI in the same directory as `uri`, or `None` when
/// the URI has no directory part (or does not parse back).
fn sibling_uri(uri: &Uri, file_name: &str) -> Option<Uri> {
    let dir = uri.as_str().rsplit_once('/')?.0;
    format!("{dir}/{file_name}").parse().ok()
}
