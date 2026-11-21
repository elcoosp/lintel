//! The tree-sitter lane (architecture doc §17.1 Q3's "tree-sitter
//! grammar"; the Phase 11 LSP-parsing card): an **independent second
//! parse** of every `.lintel.php` source, built on the upstream
//! `tree-sitter-php` grammar. The lane complements the lintel-core
//! scanner — it never replaces it:
//!
//! - **Component declarations** (`function_definition` nodes: name,
//!   identifier offset, `$param` names + offsets) are what the lane
//!   contributes to the queries: `textDocument/definition`,
//!   `textDocument/rename` and `textDocument/references` consult the
//!   tree-sitter index **in addition to** the Phase 9 persistent
//!   workspace index's scanner facts, and a declaration only one of the
//!   two lanes sees still resolves, renames and lists references.
//! - **JSX structure is not modeled by the PHP grammar**: markup regions
//!   degrade to ERROR nodes (tree-sitter error recovery keeps the
//!   surrounding `function_definition` nodes intact — verified for the
//!   declaration shape; a declaration whose header itself broke yields
//!   nothing). The lintel-core scanner stays the JSX-structure authority;
//!   no upstream JSX extension of the PHP grammar exists to reuse, which
//!   is the card's documented grammar limit.
//!
//! The per-file lane contract: a parse that produced a tree and saw at
//! least one usable declaration (or a clean parse with none — a file
//! without components) rides the [`TsLane::TreeSitter`] lane; a parse
//! that failed outright or saw no usable declaration structure falls
//! back to the lintel-core scanner ([`TsLane::ScannerFallback`]) — the
//! caller then serves the file's declarations from
//! [`crate::index::scan_declarations`] alone, so malformed input never
//! blanks a file out of the workspace. The merged view
//! ([`merge`]) is the union: every tree-sitter declaration plus every
//! scanner declaration the tree-sitter lane did not already see,
//! deduplicated by `(name, offset)` — both lanes point at the same
//! identifier bytes for the same declaration (pinned by a unit test
//! against the scanner).

use crate::index::IndexedComponent;
use serde_json::{json, Value};

/// Which parser served one file's declaration facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TsLane {
    /// tree-sitter produced the file's declarations (possibly none — a
    /// clean parse of a component-less source).
    TreeSitter,
    /// tree-sitter could not see the file's declarations (parse failure
    /// or no usable declaration structure) — the lintel-core scanner
    /// serves the file instead.
    ScannerFallback,
}

impl TsLane {
    /// The cache-file spelling of the lane.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::TreeSitter => "tree-sitter",
            Self::ScannerFallback => "scanner-fallback",
        }
    }

    /// The lane a cache row names; `None` for an unknown spelling (the
    /// caller re-parses the file — a stale lane name is a fact drift).
    pub(crate) fn from_str(raw: &str) -> Option<Self> {
        match raw {
            "tree-sitter" => Some(Self::TreeSitter),
            "scanner-fallback" => Some(Self::ScannerFallback),
            _ => None,
        }
    }
}

/// One file's tree-sitter facts: the lane that served it plus the
/// declarations tree-sitter saw (empty on the fallback lane — the caller
/// merges the scanner's view).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TsScan {
    pub(crate) lane: TsLane,
    pub(crate) declarations: Vec<IndexedComponent>,
}

impl TsScan {
    /// The cache-file shape of one scan.
    pub(crate) fn to_json(&self) -> Value {
        json!({
            "lane": self.lane.as_str(),
            "declarations": super::index::declarations_json(&self.declarations),
        })
    }

    /// The scan a cache row carries; `None` on a shape mismatch (the
    /// caller re-parses the file).
    pub(crate) fn from_json(value: &Value) -> Option<Self> {
        let lane = TsLane::from_str(value.get("lane")?.as_str()?)?;
        let declarations = super::index::declarations_from_json(value.get("declarations")?);
        Some(Self { lane, declarations })
    }
}

/// Parse `text` with the tree-sitter PHP grammar and collect the
/// component declarations the lane sees: every `function_definition`
/// whose name and parameter list parsed cleanly contributes its name,
/// identifier offset and `$param` names + offsets — the same shape the
/// scanner's `Declaration` table carries, with byte offsets that agree
/// with it (both point at the identifier's / sigil's first byte).
///
/// Markup regions are ERROR soup (the PHP grammar does not model JSX, §
/// module docs): they do not invalidate a declaration whose own nodes
/// parsed; a file whose every declaration is invisible to the lane —
/// the broken-header malformations — falls back to the scanner lane.
#[must_use]
pub(crate) fn scan(text: &str) -> TsScan {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_php::LANGUAGE_PHP.into())
        .expect("the PHP grammar loads");
    let Some(tree) = parser.parse(text, None) else {
        // No parse tree (cancelled): nothing usable — the scanner serves.
        return TsScan {
            lane: TsLane::ScannerFallback,
            declarations: Vec::new(),
        };
    };
    let mut declarations = Vec::new();
    collect(tree.root_node(), text, &mut declarations);
    let lane = if declarations.is_empty() && tree.root_node().has_error() {
        TsLane::ScannerFallback
    } else {
        TsLane::TreeSitter
    };
    TsScan { lane, declarations }
}

/// The merged workspace view for one file: every tree-sitter declaration
/// first (the independent parse — the definition anchor when both lanes
/// see the same name), then every scanner declaration the tree-sitter
/// lane did not already see. Deduplication is exact: the same name **and**
/// the same identifier byte offset — the lanes agree on offsets (pinned
/// by a unit test); a disagreement is two entries, never a silent loss.
#[must_use]
pub(crate) fn merge(scanner: Vec<IndexedComponent>, ts: &TsScan) -> Vec<IndexedComponent> {
    let mut merged = ts.declarations.clone();
    for declaration in scanner {
        let known = merged
            .iter()
            .any(|seen| seen.name == declaration.name && seen.offset == declaration.offset);
        if !known {
            merged.push(declaration);
        }
    }
    merged
}

/// Walk the tree collecting usable `function_definition` declarations.
fn collect(node: tree_sitter::Node<'_>, text: &str, out: &mut Vec<IndexedComponent>) {
    if node.kind() == "function_definition" {
        if let Some(declaration) = declaration_of(node, text) {
            out.push(declaration);
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect(child, text, out);
    }
}

/// One `function_definition` node as an indexed declaration, when its
/// name parsed as a real identifier: the name node must be present,
/// non-ERROR, non-MISSING and alphabetic/underscore-led (tree-sitter
/// error recovery can produce a `name` field over garbage — `function
/// $Badge`'s recovered name node is skipped the same way).
fn declaration_of(node: tree_sitter::Node<'_>, text: &str) -> Option<IndexedComponent> {
    let name = node.child_by_field_name("name")?;
    if name.is_error() || name.is_missing() {
        return None;
    }
    let range = name.byte_range();
    let name_text = text.get(range.start..range.end)?;
    let first = name_text.bytes().next()?;
    if !first.is_ascii_alphabetic() && first != b'_' {
        return None;
    }
    let mut params = Vec::new();
    if let Some(parameters) = node.child_by_field_name("parameters") {
        let mut cursor = parameters.walk();
        for child in parameters.children(&mut cursor) {
            if child.kind() != "simple_parameter" || child.is_error() || child.is_missing() {
                continue;
            }
            let Some(variable) = child.child_by_field_name("name") else {
                continue;
            };
            let variable_range = variable.byte_range();
            let variable_text = text.get(variable_range.start..variable_range.end)?;
            let param = variable_text.strip_prefix('$')?;
            if param.is_empty() {
                continue;
            }
            params.push((param.to_string(), variable_range.start));
        }
    }
    Some(IndexedComponent {
        name: name_text.to_string(),
        offset: range.start,
        params,
    })
}

#[cfg(test)]
mod tests {
    use super::{merge, scan, TsLane};
    use crate::index::scan_declarations;

    /// The JSX-laden source the offsets pin against the scanner.
    const HEALTHY: &str = concat!(
        "<?php\n\n",
        "function Badge(string $label): Html\n{\n",
        "    return <b class=\"pill\">{$label}</b>;\n}\n\n",
        "function Page(string $title): Html\n{\n",
        "    return <html><body><Badge label={$title} /></body></html>;\n}\n"
    );

    #[test]
    fn a_healthy_source_rides_the_tree_sitter_lane_and_the_union_covers_the_whole_file() {
        let ts = scan(HEALTHY);
        assert_eq!(ts.lane, TsLane::TreeSitter);
        assert!(
            ts.declarations.iter().any(|d| d.name == "Badge"),
            "the first declaration parses, JSX body and all"
        );

        // tree-sitter error recovery can swallow a declaration that
        // follows a markup region (the probe-documented `Page` shape):
        // the lane is a SUBSET of the scanner's view on such files. The
        // merged view — what the queries read — covers the whole file:
        let scanner = scan_declarations(HEALTHY);
        let merged = merge(scanner, &ts);
        let names: Vec<&str> = merged.iter().map(|d| d.name.as_str()).collect();
        assert!(
            names.contains(&"Badge") && names.contains(&"Page"),
            "the union sees both declarations: {names:?}"
        );

        // The lanes agree on the identifier and sigil offsets for every
        // declaration both see.
        let parsed = lintel_core::scan::scan(HEALTHY);
        for (name, _signature) in parsed.context.declared_signatures() {
            let Some(ts_declaration) = ts.declarations.iter().find(|d| d.name == name) else {
                continue; // a recovery-swallowed declaration — the union's job
            };
            let scanner_declaration = parsed
                .context
                .declaration_of(name)
                .expect("the scanner sees its own declaration");
            assert_eq!(ts_declaration.offset, scanner_declaration.offset);
            assert_eq!(ts_declaration.params, scanner_declaration.params);
        }
    }

    #[test]
    fn multibyte_content_leaves_the_byte_offsets_byte_accurate() {
        // Multibyte text BEFORE the declaration (a CJK label literal):
        // both lanes' offsets are byte offsets into the same text —
        // neither lane silently counts chars. (Multibyte *identifiers*
        // are outside both lanes: the scanner's identifier rule is
        // ASCII-shaped, tree-sitter-php's grammar likewise — a CJK-named
        // component is invisible to both, so there is no lane
        // disagreement to paper over.)
        let source = concat!(
            "<?php\n\n",
            "function Badge(string $label): Html\n{\n",
            "    return <b class=\"pill\">{$label}</b>;\n}\n"
        );
        let prefix = "<?php\n// 従业员のラベル\n";
        let shifted = source.replacen("<?php\n", prefix, 1);
        let shift = prefix.len() - "<?php\n".len();

        let ts = scan(&shifted);
        let plain = scan(source);
        assert_eq!(ts.declarations.len(), plain.declarations.len());
        for (shifted_declaration, plain_declaration) in
            ts.declarations.iter().zip(&plain.declarations)
        {
            assert_eq!(shifted_declaration.offset, plain_declaration.offset + shift);
            for (shifted_param, plain_param) in shifted_declaration
                .params
                .iter()
                .zip(&plain_declaration.params)
            {
                assert_eq!(shifted_param.0, plain_param.0);
                assert_eq!(shifted_param.1, plain_param.1 + shift);
            }
        }
    }

    #[test]
    fn a_declaration_after_a_markup_region_still_parses() {
        // tree-sitter error recovery keeps later declarations usable
        // across an earlier ERROR soup (the markup blob) — the union with
        // the scanner closes the gap where recovery swallows one.
        let source = concat!(
            "<?php\n\n",
            "function Badge(string $label): Html\n{\n    return <b>{$label}</b>;\n}\n\n",
            "huge markup blob with <div class=\"x\"> and </div> stray </p>\n\n",
            "function After(string $t): Html\n{\n    return <i>{$t}</i>;\n}\n"
        );
        let ts = scan(source);
        assert_eq!(ts.lane, TsLane::TreeSitter, "Badge at least is usable");
        assert!(ts
            .declarations
            .iter()
            .any(|d| d.name == "Badge" && d.params.len() == 1));
    }

    #[test]
    fn a_broken_declaration_header_falls_back_to_the_scanner() {
        // `function Badge(***): Html` — the header is beyond recovery:
        // the lane sees no usable declaration structure, the file rides
        // the fallback lane, and the lintel-core scanner (which tolerates
        // the malformed parameter list) serves the declarations.
        let broken = "<?php\nfunction Badge(***): Html\n{\n    return <b>{$label}</b>;\n}\n";
        let ts = scan(broken);
        assert_eq!(ts.lane, TsLane::ScannerFallback);
        assert_eq!(ts.declarations.len(), 0);

        // The fallback lane keeps the file's declarations alive: the
        // merged view is exactly the scanner's.
        let scanner = scan_declarations(broken);
        assert_eq!(scanner.len(), 1, "the scanner tolerates the header");
        let merged = merge(scanner.clone(), &ts);
        assert_eq!(merged, scanner);
    }

    #[test]
    fn a_component_less_clean_parse_stays_on_the_tree_sitter_lane() {
        let source = "<?php\n\necho 'no components here';\n";
        let ts = scan(source);
        assert_eq!(ts.lane, TsLane::TreeSitter);
        assert!(ts.declarations.is_empty(), "nothing to contribute");
    }

    #[test]
    fn a_valid_header_with_garbage_params_still_declares_with_the_params_it_saw() {
        // `string $label,,` — the double comma survives recovery; the
        // declaration contributes the params it parsed cleanly, and the
        // scanner's fuller view rides the union.
        let source =
            "<?php\nfunction Badge(string $label,,): Html\n{\n    return <b>{$label}</b>;\n}\n";
        let ts = scan(source);
        assert_eq!(ts.lane, TsLane::TreeSitter);
        assert!(ts
            .declarations
            .iter()
            .any(|d| d.name == "Badge" && d.params.iter().any(|(name, _)| name == "label")));
    }

    #[test]
    fn the_merge_is_a_deduplicated_union_preferring_the_tree_sitter_row() {
        let ts = scan(HEALTHY);
        // A scanner declaration the lane already saw (same name + offset)
        // deduplicates; one the lane never saw (different offset) rides
        // the union.
        let mut scanner = scan_declarations(HEALTHY);
        scanner.push(crate::index::IndexedComponent {
            name: "Badge".to_string(),
            offset: usize::MAX, // same name, impossible offset — not a duplicate
            params: Vec::new(),
        });
        let scanner_len = scanner.len();
        let merged = merge(scanner, &ts);
        assert_eq!(
            merged.len(),
            scanner_len,
            "the healthy pair dedupes, the stranger rides"
        );
        assert_eq!(
            merged.iter().find(|d| d.name == "Badge").expect("badge"),
            ts.declarations
                .iter()
                .find(|d| d.name == "Badge")
                .expect("ts badge"),
            "the tree-sitter row is the union's Badge"
        );
    }

    #[test]
    fn the_lane_name_round_trips_through_the_cache_spelling() {
        use super::TsLane;
        assert_eq!(TsLane::TreeSitter.as_str(), "tree-sitter");
        assert_eq!(TsLane::from_str("tree-sitter"), Some(TsLane::TreeSitter));
        assert_eq!(
            TsLane::from_str("scanner-fallback"),
            Some(TsLane::ScannerFallback)
        );
        assert_eq!(TsLane::from_str("something-else"), None);
    }

    #[test]
    fn the_scan_round_trips_through_the_cache_json() {
        let ts = scan(HEALTHY);
        let restored = super::TsScan::from_json(&ts.to_json()).expect("restores");
        assert_eq!(restored, ts, "lane + declarations survive the cache row");

        let broken = super::TsScan::from_json(&serde_json::json!({"lane": "unknown"}));
        assert!(broken.is_none(), "an unknown lane resets the row");
    }
}
