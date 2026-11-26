//! The self-describing rule-registry contracts (`docs/rule-registry.md`):
//! every built-in rule carries a stable [`RuleMeta`] — a unique kebab-case
//! id, the `LNLxxxx` codes it emits and a one-line summary — the built-in
//! listing is complete against every rule-emittable code, the JSON
//! emission follows the shared machine contract, and consumer rules keep
//! compiling whether or not they classify themselves.

use lintl_core::{
    ast::Node, builtin_rule_metas, builtin_rules, registry_metas, scan::ScanContext, Code,
    Diagnostic, RuleMeta, ValidationRule,
};

/// Every `LNLxxxx` code a validation rule can emit: the five `LNL01xx`
/// parse classes are scanner output and never rule-selected, so the
/// registry must cover exactly the other eight.
const RULE_EMITTABLE_CODES: &[Code] = &[
    Code::BuiltinShadow,
    Code::HyphenArg,
    Code::UnknownProp,
    Code::UnexpectedChildren,
    Code::MissingRequiredProp,
    Code::DynamicHandler,
    Code::StaticHandler,
    Code::VoidChildren,
];

#[test]
fn every_builtin_rule_is_classified() {
    let rules = builtin_rules();
    let classified: Vec<RuleMeta> = rules.iter().filter_map(|rule| rule.meta()).collect();
    assert_eq!(
        classified.len(),
        rules.len(),
        "every built-in rule carries a RuleMeta"
    );
}

#[test]
fn builtin_rule_metas_follow_the_registry_order() {
    let ids: Vec<&str> = builtin_rule_metas().map(|meta| meta.id).collect();
    assert_eq!(
        ids,
        vec![
            "builtin-shadow",
            "hyphen-arg",
            "unknown-prop",
            "missing-required-prop",
            "unexpected-children",
            "dynamic-handler",
            "static-handler",
            "void-children",
        ],
        "registry order is the diagnostic order"
    );
}

#[test]
fn builtin_ids_are_unique_kebab_case() {
    let ids: Vec<&str> = builtin_rule_metas().map(|meta| meta.id).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), ids.len(), "ids are unique: {ids:?}");
    for id in ids {
        let bytes = id.as_bytes();
        assert!(
            !id.is_empty()
                && bytes[0].is_ascii_lowercase()
                && bytes
                    .iter()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
                && !id.ends_with('-')
                && !id.contains("--"),
            "`{id}` is kebab-case",
        );
    }
}

#[test]
fn builtin_codes_cover_exactly_the_rule_emittable_set() {
    let mut emitted: Vec<Code> = builtin_rule_metas()
        .flat_map(|meta| meta.codes)
        .copied()
        .collect();
    emitted.sort_by_key(|code| code.lnt());
    let mut expected = RULE_EMITTABLE_CODES.to_vec();
    expected.sort_by_key(|code| code.lnt());
    assert_eq!(
        emitted, expected,
        "the registry covers every rule-emittable code exactly once"
    );
}

#[test]
fn no_parse_code_appears_in_the_registry() {
    for meta in builtin_rule_metas() {
        for code in meta.codes {
            assert!(
                !code.lnt().starts_with("LNL01"),
                "parse diagnostics are scanner output, never rule-selected: {} in {}",
                code.lnt(),
                meta.id
            );
        }
    }
}

#[test]
fn meta_json_is_one_stable_line() {
    let meta = builtin_rule_metas().next().expect("the first built-in");
    assert_eq!(
        meta.to_json(),
        "{\"id\":\"builtin-shadow\",\"codes\":[\"LNL0201\"],\"summary\":\"a component must not \
         resolve to a PHP builtin function unless the file declares or imports that name\"}"
    );
    assert!(!meta.to_json().contains('\n'), "no trailing newline");
}

#[test]
fn meta_json_escapes_quotes_and_backslashes() {
    let meta = RuleMeta {
        id: "custom",
        codes: &[Code::StaticHandler],
        summary: "quotes `\"` and a backslash `\\` survive",
    };
    let json = meta.to_json();
    assert!(
        json.contains("\"summary\":\"quotes `\\\"` and a backslash `\\\\` survive\"}"),
        "{json}"
    );
}

#[test]
fn meta_json_allows_an_empty_code_list() {
    let meta = RuleMeta {
        id: "policy-only",
        codes: &[],
        summary: "reports nothing yet",
    };
    assert!(
        meta.to_json()
            .starts_with("{\"id\":\"policy-only\",\"codes\":[],"),
        "{}",
        meta.to_json()
    );
}

/// A consumer rule that classifies itself: the registry lists what the
/// rule declares, exactly as declared.
struct ClassifiedCustom;

impl ValidationRule for ClassifiedCustom {
    fn check(&self, _node: &Node, _context: &ScanContext, _out: &mut Vec<Diagnostic>) {}
    fn meta(&self) -> Option<RuleMeta> {
        Some(RuleMeta {
            id: "consumer-policy",
            codes: &[Code::VoidChildren],
            summary: "a consumer's own policy",
        })
    }
}

/// A consumer rule that does not classify itself: the default keeps it
/// valid — it simply reports no registry entry.
struct UnclassifiedCustom;

impl ValidationRule for UnclassifiedCustom {
    fn check(&self, _node: &Node, _context: &ScanContext, _out: &mut Vec<Diagnostic>) {}
}

#[test]
fn registry_metas_classifies_only_what_rules_declare() {
    let registry: [&dyn ValidationRule; 2] = [&UnclassifiedCustom, &ClassifiedCustom];
    let metas = registry_metas(&registry);
    assert_eq!(metas.len(), 2, "one entry per rule, in registry order");
    assert_eq!(
        metas[0], None,
        "an unclassified rule reports no registry entry"
    );
    assert_eq!(
        metas[1].map(|meta| meta.id),
        Some("consumer-policy"),
        "a classified rule reports its declared identity verbatim"
    );
}

#[test]
fn registry_metas_over_the_builtins_matches_builtin_rule_metas() {
    let from_registry: Vec<Option<RuleMeta>> =
        registry_metas(builtin_rules()).into_iter().collect();
    let direct: Vec<Option<RuleMeta>> = builtin_rule_metas().map(Some).collect();
    assert_eq!(from_registry, direct);
}
