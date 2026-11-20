//! The §7.3-Y2 analyze seam: external, PHPStan-proven child-expression
//! types consumed by the lowering (`docs/type-elision.md`).
//!
//! The `lintel analyze` path runs the `PHPStan` bridge over a compiled
//! artifact, resolves every `\Lintel\c()` argument's type, and reports the
//! provably-safe ones per (absolute source line, expression text). This
//! module holds the consumed side: the [`TypeHints`] table the CLI parses
//! the report into, and the per-root [`EscapeDecisions`] resolution that
//! keys the hints onto the AST's absolute byte offsets — the same offsets
//! the emitter already carries, so the lowering looks decisions up with no
//! line math of its own.
//!
//! Matching is doubly keyed and conservative: a hint applies to a child
//! only when BOTH the absolute line and the exact expression text agree
//! (the artifact is line-preserved, so the bridge's per-line report and
//! the source agree 1:1), and two hints naming one child with different
//! kinds mean the analysis is ambiguous — the child keeps the helper.

use std::collections::{HashMap, HashSet};

use crate::ast::{visit_node, Node, NodeVisitor};

/// One proven child kind the elision accepts (architecture doc §7.3-Y2).
///
/// `bool` is deliberately absent: `c()` renders every bool as `''` while a
/// raw bool interpolates as `'1'` — elision cannot preserve the rendered
/// output without dropping the expression (and its evaluation), so the
/// kind stays unelidable and the helper keeps it
/// (`docs/type-elision.md` records the decision).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChildKind {
    /// `is_int` values interpolate as digits.
    Int,
    /// `is_float` values interpolate as PHP's float-to-string form — the
    /// exact conversion `c()` performs.
    Float,
    /// `is_numeric` strings (`[0-9.eE+-]` plus whitespace): none of the
    /// markup characters `<>&"'`, so escaping is the identity.
    NumericString,
    /// A constant string the analysis fixed at analysis time. Elision
    /// inlines the ESCAPED value — byte-identical to what `c()` renders
    /// for it, and attacker-uncontrollable by construction (the value is
    /// baked into the artifact, not read at runtime).
    Literal(String),
}

impl ChildKind {
    /// The report spelling of the kind — the analyze report's `type`
    /// string (`literal` inlines its value separately).
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Int => "int",
            Self::Float => "float",
            Self::NumericString => "numeric-string",
            Self::Literal(_) => "literal",
        }
    }

    /// Parse one type string from the analyze report. `Ok(None)` marks a
    /// KNOWN but non-elidable kind (the report carries every resolved
    /// type; only the allowlist above elides), `Err` an unknown string —
    /// a contract mismatch between the report producer and this crate.
    ///
    /// # Errors
    ///
    /// Unknown `type` strings fail: the report is a bridge contract, and
    /// silently dropping an unrecognized kind would hide drift.
    pub fn parse(kind: &str, value: Option<&str>) -> Result<Option<Self>, String> {
        match kind {
            "int" => Ok(Some(Self::Int)),
            "float" => Ok(Some(Self::Float)),
            "numeric-string" => Ok(Some(Self::NumericString)),
            "literal" => value
                .map(|v| Some(Self::Literal(v.to_string())))
                .ok_or_else(|| "a `literal` entry requires its `value`".to_string()),
            // Known, resolved, NOT elidable: bool (render parity — see the
            // enum doc), plain `string` (can carry markup), and the
            // composite/unknown-shape kinds the helper keeps.
            "bool" | "string" | "mixed" | "null" | "object" | "array" | "callable" | "iterable"
            | "resource" => Ok(None),
            other => Err(format!(
                "unknown child type `{other}` in the analyze report"
            )),
        }
    }
}

/// One child-expression hint from the analyze report: the child's
/// ABSOLUTE source line (1-based — the artifact is line-preserved, so the
/// bridge's per-line report and the source agree 1:1), the exact
/// expression text as the compiled artifact carries it inside `c(...)`
/// (the lowering emits the trimmed source text verbatim), and the proven
/// kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildHint {
    /// Absolute 1-based source line of the child expression.
    pub line: u32,
    /// The child's expression text, as written inside `c(...)` in the
    /// artifact (the trimmed source text).
    pub expr: String,
    /// The proven kind.
    pub kind: ChildKind,
}

/// The analyze report for one source file, keyed for lookup. Conflicting
/// entries (same line + text, different kinds) poison their key: the
/// analysis is ambiguous and the child keeps the helper.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TypeHints {
    by_key: HashMap<(u32, String), Option<ChildKind>>,
}

impl TypeHints {
    /// Build the table from the report's entries.
    #[must_use]
    pub fn from_entries(entries: impl IntoIterator<Item = ChildHint>) -> Self {
        let mut by_key: HashMap<(u32, String), Option<ChildKind>> = HashMap::new();
        for ChildHint { line, expr, kind } in entries {
            let key = (line, expr.trim().to_string());
            match by_key.get(&key) {
                // Absent: record it.
                None => {
                    by_key.insert(key, Some(kind));
                }
                // Same kind twice (duplicate report lines): harmless.
                Some(existing) if existing.as_ref() == Some(&kind) => {}
                // Conflicting kinds, or an already-poisoned key: the
                // analysis is ambiguous — keep the key poisoned.
                Some(_) => {
                    by_key.insert(key, None);
                }
            }
        }
        Self { by_key }
    }

    /// Whether the report carries no entries — the analyze pass ran and
    /// proved nothing (the common conservative outcome).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }

    /// The proven kind for a child at `line` whose artifact text is
    /// `expr` (both compared trimmed), or `None` when absent or ambiguous.
    #[must_use]
    pub fn lookup(&self, line: u32, expr: &str) -> Option<&ChildKind> {
        self.by_key
            .get(&(line, expr.trim().to_string()))
            .and_then(|slot| slot.as_ref())
    }

    /// Every report entry as `(line, expr, kind)` — `kind` is `None` when
    /// the entry was poisoned by conflicting kinds — ordered by line then
    /// expression, so surfaces built on it are stable.
    #[must_use]
    pub fn entries(&self) -> Vec<(u32, &str, Option<&ChildKind>)> {
        let mut out: Vec<(u32, &str, Option<&ChildKind>)> = self
            .by_key
            .iter()
            .map(|((line, expr), kind)| (*line, expr.as_str(), kind.as_ref()))
            .collect();
        out.sort_unstable_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(b.1)));
        out
    }
}

/// Per-root decisions resolved from [`TypeHints`], keyed by the child
/// expression node's absolute source byte offset — the field every tree
/// node already carries, so the lowering looks decisions up with no line
/// math. Only child expressions resolve: attributes keep their
/// context-aware helpers (`docs/escape-elision.md` rule 5).
pub type EscapeDecisions = HashMap<usize, ChildKind>;

/// Resolve the hint table onto one root's child expressions: a hint
/// applies when BOTH the absolute line and the exact trimmed expression
/// text match the node's recorded position. Hints that match nothing are
/// silently ignored — a stale report must degrade to the wrapped
/// emission, never to a guessed fold.
#[must_use]
pub(crate) fn resolve_decisions(root: &Node, src: &str, hints: &TypeHints) -> EscapeDecisions {
    if hints.is_empty() {
        return EscapeDecisions::new();
    }
    let mut resolver = Resolver {
        src,
        hints,
        decisions: EscapeDecisions::new(),
    };
    visit_node(root, &mut resolver);
    resolver.decisions
}

struct Resolver<'h> {
    src: &'h str,
    hints: &'h TypeHints,
    decisions: EscapeDecisions,
}

impl Resolver<'_> {
    /// Absolute 1-based line of a byte offset — where the lowering emits
    /// the child's part, and therefore where the artifact's `c(...)` call
    /// (and its argument) starts. Same newline counting the scanner's
    /// `newline_count` uses.
    fn line_of(&self, offset: usize) -> u32 {
        u32::try_from(self.src[..offset.min(self.src.len())].matches('\n').count())
            .unwrap_or(u32::MAX)
            + 1
    }
}

impl NodeVisitor for Resolver<'_> {
    fn visit(&mut self, node: &Node) {
        let Node::Expr { php, offset, .. } = node else {
            return;
        };
        if let Some(kind) = self.hints.lookup(self.line_of(*offset), php.trim()) {
            self.decisions.insert(*offset, kind.clone());
        }
    }
}

/// One report entry that matched no child expression of the current
/// source: the analysis ran against an older artifact (or names a line
/// the source no longer has). Surfaced by `lintel check --types` and the
/// analyze report so a stale report degrades loudly instead of silently
/// compiling without its hints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleHint {
    /// The report's absolute line — no child expression sits there now.
    pub line: u32,
    /// The report's expression text.
    pub expr: String,
    /// The entry's kind spelling, or `ambiguous` when the entry was
    /// poisoned by conflicting kinds.
    pub kind: String,
}

/// The report entries that match no child expression of `src` — the stale
/// surface `lintel check --types` renders as advisories and the analyze
/// report counts. Advisory by construction: elision ignores these entries
/// (a stale report degrades to the wrapped emission), this seam only makes
/// the degradation visible.
#[must_use]
pub fn stale_hints(src: &str, hints: &TypeHints) -> Vec<StaleHint> {
    if hints.is_empty() {
        return Vec::new();
    }
    let parsed = crate::scan::scan(src);
    let mut collector = ChildPositions {
        src,
        matched: HashSet::new(),
    };
    for root in &parsed.roots {
        visit_node(root, &mut collector);
    }
    hints
        .entries()
        .into_iter()
        .filter(|(line, expr, _)| !collector.matched.contains(&(*line, (*expr).to_string())))
        .map(|(line, expr, kind)| StaleHint {
            line,
            expr: expr.to_string(),
            kind: kind.map_or_else(
                || "ambiguous".to_string(),
                |value| value.as_str().to_string(),
            ),
        })
        .collect()
}

/// Every child expression's `(line, trimmed text)` key of one source — the
/// positions a report entry must hit to count as fresh.
struct ChildPositions<'s> {
    src: &'s str,
    matched: HashSet<(u32, String)>,
}

impl NodeVisitor for ChildPositions<'_> {
    fn visit(&mut self, node: &Node) {
        let Node::Expr { php, offset, .. } = node else {
            return;
        };
        let line = u32::try_from(
            self.src[..(*offset).min(self.src.len())]
                .matches('\n')
                .count(),
        )
        .unwrap_or(u32::MAX)
            + 1;
        self.matched.insert((line, php.trim().to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::{ChildHint, ChildKind, TypeHints};

    #[test]
    fn parse_maps_the_allowlist_and_the_known_non_elidable_kinds() {
        assert_eq!(ChildKind::parse("int", None), Ok(Some(ChildKind::Int)));
        assert_eq!(
            ChildKind::parse("literal", Some("row")),
            Ok(Some(ChildKind::Literal("row".into())))
        );
        assert!(ChildKind::parse("literal", None).is_err());
        assert_eq!(
            ChildKind::parse("bool", None),
            Ok(None),
            "bool is known but deferred"
        );
        assert_eq!(ChildKind::parse("string", None), Ok(None));
        assert!(
            ChildKind::parse("numeric", None).is_err(),
            "unknown kinds fail loudly"
        );
    }

    #[test]
    fn duplicate_identical_hints_are_harmless_but_conflicts_poison() {
        let entries = |kinds: Vec<ChildKind>| {
            kinds
                .into_iter()
                .map(|kind| ChildHint {
                    line: 3,
                    expr: "$id".to_string(),
                    kind,
                })
                .collect::<Vec<_>>()
        };
        let same = TypeHints::from_entries(entries(vec![ChildKind::Int, ChildKind::Int]));
        assert_eq!(same.lookup(3, "$id"), Some(&ChildKind::Int));
        let conflict = TypeHints::from_entries(entries(vec![ChildKind::Int, ChildKind::Float]));
        assert_eq!(
            conflict.lookup(3, "$id"),
            None,
            "conflicting kinds keep the helper"
        );
    }

    #[test]
    fn lookup_trims_both_sides() {
        let hints = TypeHints::from_entries([ChildHint {
            line: 2,
            expr: " $id ".to_string(),
            kind: ChildKind::Int,
        }]);
        assert_eq!(hints.lookup(2, "$id"), Some(&ChildKind::Int));
        assert_eq!(hints.lookup(2, " $id "), Some(&ChildKind::Int));
        assert_eq!(hints.lookup(3, "$id"), None);
    }
}
