//! Compile-time validation pass (architecture doc task T-08).
//!
//! Checks that need the whole tree or the file's declared-function context
//! run here, after scanning, before the compiled PHP is returned. The pass
//! is a **registry of rules**: every lint is a [`ValidationRule`] strategy
//! that walks the same AST the emitter does, and the default set is
//! injectable — [`crate::compile_with`] swaps it for a consumer's rules the
//! way [`crate::scan_with`] swaps the lowering target (doc 5.3). The
//! built-in lints:
//!
//! - **`LNT0201` builtin shadow** — a component whose name resolves to a PHP
//!   builtin function (`<Header>` → `header()`) unless the file declares or
//!   imports that function (doc 4.7).
//! - **`LNT0202` invalid component prop** — an attribute that cannot become
//!   a PHP named argument (e.g. `data-x`), with a spread suggestion.
//! - **`LNT0301` dynamic event handler** — `onclick={expr}` and friends are
//!   compile errors; static strings stay author-trusted (doc 4.4).
//! - **`LNT0302` static event handler** — `onclick="…"` strings compile but
//!   carry a [`Severity::Warning`] (doc 4.4: "static strings allowed with a
//!   lint warning"); the compiler still returns output.
//! - **`LNT0401` void children** — void elements cannot take children.
//! - **`LNT0203`/`LNT0204` signature checks** — for components declared in
//!   the same file (doc §17.1 signature scanning), every named prop must
//!   match a declared parameter (`LNT0203`) and JSX children require a
//!   `children` parameter (`LNT0204`); both situations are guaranteed PHP
//!   runtime fatals (`Unknown named parameter`).
//! - **`LNT0205` required parameter** — a parameter declared without a
//!   default value must be supplied by the invocation (a spread counts as
//!   supplying, JSX children satisfy a `children` parameter); omitting one
//!   is the guaranteed `Argument ... not given` runtime fatal.
//!
//! Diagnostics anchored inside a `lintel-ignore-next` suppression window
//! (recorded by the scanner, see `docs/ignore-directives.md`) are dropped
//! here, and a file-level `lintel-ignore-file` directive short-circuits the
//! whole walk before any rule runs; parse diagnostics never reach this
//! pass, so a directive cannot hide a misparse. The filters sit in the
//! shared walk ([`validate_with`]), so custom rules inherit them — no rule
//! sees suppression at all.
//!
//! The pass is pure: `validate` maps roots + context → diagnostics, and
//! [`crate::compile`] splits them by severity — errors fail the compilation,
//! warnings ride along on the output.

use crate::ast::{is_void_element, Attr, Element, Node};
use crate::scan::{ScanContext, Signature};
use lintel_diag::{Code, Diagnostic, Severity};

/// One compile-time validation rule (doc T-08): a strategy consulted for
/// every AST node the walk visits, in registry order.
///
/// A rule receives each [`Node`] top-down, depth-first, exactly once, and
/// pushes any [`Diagnostic`] it wants to report onto `out`. Contract:
///
/// - **Codes are stable classes.** `Code` is a closed, documented enum
///   (`LNT0101`–`LNT0401`); a custom rule reuses the closest existing class
///   for its policy (the message and `help:` lines carry the specifics).
///   New first-party rules land new codes in the standard ranges; the
///   `LNTxxxx` docs-URL contract stays total.
/// - **Severity is the rule's choice.** [`Severity::Error`] diagnostics
///   fail [`crate::compile_with`]; [`Severity::Warning`] diagnostics ride
///   on [`crate::Output::warnings`] (doc 4.4).
/// - **No suppression knowledge.** Diagnostics anchored inside a
///   `lintel-ignore-next` window are dropped by the shared walk
///   ([`validate_with`]) after rules run, and a file-level
///   `lintel-ignore-file` flag short-circuits the walk before any rule
///   runs — a rule cannot opt out of, or observe, suppression either way.
/// - **Rules see every node.** Filter by [`Node`] variant (elements,
///   fragments, text, expressions) and by `Element::is_component`; pushing
///   nothing is a valid rule.
///
/// The registry order is the diagnostic order: rules run in the order of
/// the slice they were given ([`builtin_rules`] for the default compile),
/// and the walk is depth-first, so a rule appended last reports after the
/// built-in lints on the same node.
pub trait ValidationRule: Sync {
    /// Inspect one AST node and push diagnostics onto `out`.
    ///
    /// `context` carries the file's declared functions and signatures (doc
    /// §17.1) plus the recorded suppression windows; `out` is the shared
    /// diagnostic list for the whole pass, in walk order.
    fn check(&self, node: &Node, context: &ScanContext, out: &mut Vec<Diagnostic>);
}

/// PHP builtin function names a component call could shadow.
///
/// A curated subset of the php.net function index (doc 4.7: "the compiler
/// ships the PHP builtin function list"); it covers the short, global names
/// that are realistic shadowing hazards. Namespaced or exotic extensions are
/// out of scope for v0.1. PHP function lookups are case-insensitive, so
/// `<Header>` resolves to `header()` unless the file declares or imports a
/// function with that name.
const BUILTINS: &[&str] = &[
    "abs",
    "acos",
    "array_combine",
    "array_diff",
    "array_filter",
    "array_flip",
    "array_intersect",
    "array_key_exists",
    "array_keys",
    "array_map",
    "array_merge",
    "array_pop",
    "array_push",
    "array_reduce",
    "array_replace",
    "array_reverse",
    "array_search",
    "array_slice",
    "array_splice",
    "array_sum",
    "array_unique",
    "array_values",
    "array_walk",
    "asort",
    "assert",
    "base64_decode",
    "base64_encode",
    "ceil",
    "chmod",
    "class_alias",
    "class_exists",
    "compact",
    "copy",
    "count",
    "crc32",
    "curl_close",
    "curl_errno",
    "curl_error",
    "curl_exec",
    "curl_getinfo",
    "curl_init",
    "curl_setopt",
    "date",
    "date_create",
    "date_default_timezone_get",
    "date_default_timezone_set",
    "define",
    "defined",
    "die",
    "enum_exists",
    "error_log",
    "error_reporting",
    "escapeshellarg",
    "escapeshellcmd",
    "exec",
    "exit",
    "explode",
    "extension_loaded",
    "fclose",
    "feof",
    "fgets",
    "file",
    "file_exists",
    "file_get_contents",
    "file_put_contents",
    "filemtime",
    "filesize",
    "floatval",
    "floor",
    "flush",
    "fmod",
    "fopen",
    "fread",
    "function_exists",
    "fwrite",
    "gc_collect_cycles",
    "get_class",
    "get_debug_type",
    "get_parent_class",
    "getcwd",
    "getenv",
    "gettype",
    "glob",
    "hash",
    "hash_hmac",
    "header",
    "headers_list",
    "headers_sent",
    "hexdec",
    "html_entity_decode",
    "htmlentities",
    "htmlspecialchars",
    "htmlspecialchars_decode",
    "http_response_code",
    "implode",
    "in_array",
    "ini_get",
    "ini_set",
    "intdiv",
    "interface_exists",
    "is_array",
    "is_bool",
    "is_callable",
    "is_dir",
    "is_file",
    "is_float",
    "is_int",
    "is_iterable",
    "is_null",
    "is_numeric",
    "is_object",
    "is_scalar",
    "is_string",
    "iterator_count",
    "iterator_to_array",
    "join",
    "json_decode",
    "json_encode",
    "json_last_error",
    "json_validate",
    "krsort",
    "ksort",
    "lcfirst",
    "log",
    "log10",
    "ltrim",
    "max",
    "mb_convert_encoding",
    "mb_detect_encoding",
    "mb_internal_encoding",
    "mb_strlen",
    "mb_strtolower",
    "mb_strtoupper",
    "mb_substr",
    "md5",
    "memory_get_peak_usage",
    "memory_get_usage",
    "method_exists",
    "microtime",
    "min",
    "mkdir",
    "mktime",
    "mt_rand",
    "mt_srand",
    "nl2br",
    "number_format",
    "ob_end_clean",
    "ob_end_flush",
    "ob_get_clean",
    "ob_get_contents",
    "ob_get_length",
    "ob_start",
    "ord",
    "pack",
    "passthru",
    "password_hash",
    "password_needs_rehash",
    "password_verify",
    "php_sapi_name",
    "php_uname",
    "phpversion",
    "pi",
    "preg_grep",
    "preg_match",
    "preg_match_all",
    "preg_quote",
    "preg_replace",
    "preg_replace_callback",
    "preg_split",
    "print_r",
    "printf",
    "proc_close",
    "proc_open",
    "property_exists",
    "putenv",
    "rand",
    "random_bytes",
    "random_int",
    "rawurldecode",
    "rawurlencode",
    "readfile",
    "register_shutdown_function",
    "rename",
    "restore_error_handler",
    "restore_exception_handler",
    "rmdir",
    "round",
    "rsort",
    "rtrim",
    "scandir",
    "serialize",
    "session_destroy",
    "session_id",
    "session_name",
    "session_regenerate_id",
    "session_start",
    "session_status",
    "set_error_handler",
    "set_exception_handler",
    "setcookie",
    "setrawcookie",
    "settype",
    "sha1",
    "shell_exec",
    "shuffle",
    "sleep",
    "sort",
    "spl_autoload_register",
    "spl_autoload_unregister",
    "sprintf",
    "sqrt",
    "srand",
    "str_contains",
    "str_ends_with",
    "str_ireplace",
    "str_pad",
    "str_repeat",
    "str_replace",
    "str_shuffle",
    "str_starts_with",
    "str_word_count",
    "strchr",
    "stripos",
    "stripslashes",
    "strlen",
    "strpos",
    "strrchr",
    "strrev",
    "strripos",
    "strrpos",
    "strstr",
    "strtolower",
    "strtotime",
    "strtoupper",
    "strtr",
    "substr",
    "substr_count",
    "sys_get_temp_dir",
    "system",
    "time",
    "touch",
    "trigger_error",
    "trim",
    "uasort",
    "uksort",
    "uniqid",
    "unlink",
    "unserialize",
    "urldecode",
    "urlencode",
    "usleep",
    "usort",
    "var_dump",
    "var_export",
    "version_compare",
    "wordwrap",
];

/// The built-in registry, in diagnostic order for every element kind: a
/// component emits `LNT0201` → `LNT0202` → `LNT0203` → `LNT0205` →
/// `LNT0204`, an HTML element emits `LNT0301` → `LNT0302` → `LNT0401`.
const BUILTIN_RULES: &[&dyn ValidationRule] = &[
    &ShadowRule,
    &HyphenArgRule,
    &UnknownPropRule,
    &MissingRequiredPropRule,
    &UnexpectedChildrenRule,
    &DynamicHandlerRule,
    &StaticHandlerRule,
    &VoidChildrenRule,
];

/// The default validation registry, in built-in diagnostic order.
///
/// Pass it back through [`validate_with`] (or [`crate::compile_with`]) to
/// extend or reorder the set: a slice of `&[&ShadowRule, &my_rule]` keeps
/// the shadow lint and appends a custom policy after it. The zero-rules
/// slice disables built-in validation entirely; parse diagnostics
/// (`LNT01xx`) are scanner output and are never rule-selected.
#[must_use]
pub fn builtin_rules() -> &'static [&'static dyn ValidationRule] {
    BUILTIN_RULES
}

/// `LNT0201` — a component whose name resolves to a PHP builtin function
/// (`<Header>` → `header()`) unless the file declares or imports that
/// function (doc 4.7).
pub struct ShadowRule;

impl ValidationRule for ShadowRule {
    fn check(&self, node: &Node, context: &ScanContext, out: &mut Vec<Diagnostic>) {
        let Node::Element(element) = node else {
            return;
        };
        if !element.is_component {
            return;
        }
        let bare = last_segment(&element.name);
        let lower = bare.to_lowercase();
        // `BUILTINS` is sorted, so the lookup is a binary search (see the
        // `builtins_are_sorted_for_binary_search` unit test that keeps it
        // true as names are appended).
        if !context.declares_function(&lower) && BUILTINS.binary_search(&lower.as_str()).is_ok() {
            out.push(
                Diagnostic::new(
                    Code::BuiltinShadow,
                    format!(
                        "component `{}` shadows the PHP builtin function `{}()`",
                        element.name, lower
                    ),
                    element.offset,
                )
                .with_help(format!(
                    "declare `function {bare}()` in this file or add `use function {lower};` \
                     so the call resolves to your component"
                )),
            );
        }
    }
}

/// `LNT0202` — a component attribute that cannot become a PHP named argument
/// (e.g. `data-x`), with a spread suggestion.
pub struct HyphenArgRule;

impl ValidationRule for HyphenArgRule {
    fn check(&self, node: &Node, _context: &ScanContext, out: &mut Vec<Diagnostic>) {
        let Node::Element(element) = node else {
            return;
        };
        if !element.is_component {
            return;
        }
        for attr in &element.attrs {
            let Some(name) = attr.name() else {
                continue; // spreads bypass named-argument rules
            };
            if !is_php_identifier(name) {
                out.push(
                    Diagnostic::new(
                        Code::HyphenArg,
                        format!("component attribute `{name}` cannot become a PHP named argument"),
                        attr_offset(attr),
                    )
                    .with_help(format!(
                        "pass it through a spread instead: `<{} {{...['{name}' => value]}} />`",
                        last_segment(&element.name)
                    )),
                );
            }
        }
    }
}

/// `LNT0203` — a named prop that does not match a declared parameter of the
/// in-file component signature; PHP rejects the call at runtime with
/// `Unknown named parameter`. Signature checks only fire for components
/// declared in this file: that is the only place the parameter list is
/// knowable (doc §17.1). A variadic signature accepts arbitrary named
/// arguments, so every signature check is disabled for it.
pub struct UnknownPropRule;

impl ValidationRule for UnknownPropRule {
    fn check(&self, node: &Node, context: &ScanContext, out: &mut Vec<Diagnostic>) {
        let Node::Element(element) = node else {
            return;
        };
        if !element.is_component {
            return;
        }
        let Some(signature) = context.signature_of(&element.name) else {
            return;
        };
        if signature.variadic {
            return; // `...$rest` accepts arbitrary named arguments
        }
        check_unknown_props(element, signature, out);
    }
}

/// The `LNT0203` loop shared by the rule: unknown named props, in
/// attribute order. Spreads bypass named-argument rules and non-identifier
/// props are `LNT0202`'s report already.
fn check_unknown_props(element: &Element, signature: &Signature, out: &mut Vec<Diagnostic>) {
    for attr in &element.attrs {
        let Some(name) = attr.name() else {
            continue; // spreads bypass named-argument rules
        };
        if !is_php_identifier(name) {
            continue; // non-identifier props are LNT0202's report already
        }
        if !signature.params.iter().any(|param| param == name) {
            let bare = last_segment(&element.name);
            out.push(
                Diagnostic::new(
                    Code::UnknownProp,
                    format!("component `{bare}` declares no parameter `${name}`"),
                    attr_offset(attr),
                )
                .with_help(signature_help(bare, signature, name)),
            );
        }
    }
}

/// `LNT0205` — a parameter declared without a default value that the
/// invocation does not supply (a spread counts as supplying, JSX children
/// satisfy a `children` parameter); omitting one is the guaranteed
/// `Argument ... not given` runtime fatal.
pub struct MissingRequiredPropRule;

impl ValidationRule for MissingRequiredPropRule {
    fn check(&self, node: &Node, context: &ScanContext, out: &mut Vec<Diagnostic>) {
        let Node::Element(element) = node else {
            return;
        };
        if !element.is_component {
            return;
        }
        let Some(signature) = context.signature_of(&element.name) else {
            return;
        };
        check_missing_required(element, signature, out);
    }
}

/// The `LNT0205` loop: required parameters the invocation omits, in
/// declaration order. A spread may supply anything, so its presence
/// disables the check rather than risk a false positive; JSX children
/// satisfy a `children` parameter, and a named `children` prop satisfies
/// it too.
fn check_missing_required(element: &Element, signature: &Signature, out: &mut Vec<Diagnostic>) {
    if signature.variadic {
        return; // `...$rest` accepts arbitrary named arguments
    }
    let has_spread = element
        .attrs
        .iter()
        .any(|attr| matches!(attr, Attr::Spread { .. }));
    if has_spread {
        return;
    }
    for param in &signature.required {
        let supplied = element
            .attrs
            .iter()
            .any(|attr| attr.name().is_some_and(|name| name == param))
            || (param == "children" && !element.children.is_empty());
        if !supplied {
            let bare = last_segment(&element.name);
            out.push(
                Diagnostic::new(
                    Code::MissingRequiredProp,
                    format!("component `{bare}` is missing required parameter `${param}`"),
                    element.offset,
                )
                .with_help(format!(
                    "pass `{param}: value` on `<{bare}>` or give the parameter a \
                     default value in `function {bare}(...)`"
                )),
            );
        }
    }
}

/// `LNT0204` — JSX children passed to a component whose declared signature
/// has no `children` parameter; PHP rejects the call at runtime with
/// `Unknown named parameter`.
pub struct UnexpectedChildrenRule;

impl ValidationRule for UnexpectedChildrenRule {
    fn check(&self, node: &Node, context: &ScanContext, out: &mut Vec<Diagnostic>) {
        let Node::Element(element) = node else {
            return;
        };
        if !element.is_component {
            return;
        }
        let Some(signature) = context.signature_of(&element.name) else {
            return;
        };
        if element.children.is_empty() || signature.variadic {
            return;
        }
        if !signature.params.iter().any(|param| param == "children") {
            let bare = last_segment(&element.name);
            out.push(
                Diagnostic::new(
                    Code::UnexpectedChildren,
                    format!("component `{bare}` does not accept children"),
                    element.offset,
                )
                .with_help(format!(
                    "add a `children` parameter to `function {bare}(...)` or remove the JSX children"
                )),
            );
        }
    }
}

/// `LNT0301` — a dynamic event-handler attribute (`onclick={expr}` and
/// friends), a compile error; static strings stay author-trusted (doc 4.4).
pub struct DynamicHandlerRule;

impl ValidationRule for DynamicHandlerRule {
    fn check(&self, node: &Node, _context: &ScanContext, out: &mut Vec<Diagnostic>) {
        let Node::Element(element) = node else {
            return;
        };
        if element.is_component {
            return;
        }
        for attr in &element.attrs {
            if let Attr::Expr { name, .. } = attr {
                if name.to_ascii_lowercase().starts_with("on") {
                    out.push(
                        Diagnostic::new(
                            Code::DynamicHandler,
                            format!("dynamic event-handler attribute `{name}` is not allowed"),
                            attr_offset(attr),
                        )
                        .with_help(
                            "use `data-action={...}` (or an `hx-*` / `x-on:*` binding) \
                             and attach behaviour in JavaScript",
                        ),
                    );
                }
            }
        }
    }
}

/// `LNT0302` — a static event-handler string: allowed but discouraged, so it
/// compiles with a [`Severity::Warning`] (doc 4.4).
pub struct StaticHandlerRule;

impl ValidationRule for StaticHandlerRule {
    fn check(&self, node: &Node, _context: &ScanContext, out: &mut Vec<Diagnostic>) {
        let Node::Element(element) = node else {
            return;
        };
        if element.is_component {
            return;
        }
        for attr in &element.attrs {
            if let Attr::Str { name, .. } = attr {
                if name.to_ascii_lowercase().starts_with("on") {
                    out.push(
                        Diagnostic::new(
                            Code::StaticHandler,
                            format!("static event-handler attribute `{name}` is discouraged"),
                            attr_offset(attr),
                        )
                        .with_severity(Severity::Warning)
                        .with_help(
                            "inline JS strings bypass CSP nonces and cannot be tree-shaken: \
                             use `data-action={...}` and attach the listener in JavaScript \
                             (or an `hx-*` / `x-on:*` binding)",
                        ),
                    );
                }
            }
        }
    }
}

/// `LNT0401` — a void element given children (architecture doc 4.2).
pub struct VoidChildrenRule;

impl ValidationRule for VoidChildrenRule {
    fn check(&self, node: &Node, _context: &ScanContext, out: &mut Vec<Diagnostic>) {
        let Node::Element(element) = node else {
            return;
        };
        if element.is_component {
            return;
        }
        if is_void_element(&element.name) && !element.children.is_empty() {
            out.push(
                Diagnostic::new(
                    Code::VoidChildren,
                    format!("void element `<{}>` cannot have children", element.name),
                    element.offset,
                )
                .with_help("remove the children or switch to a non-void element"),
            );
        }
    }
}

/// Validate every root (top-level and nested) against the scan context with
/// the built-in rule registry ([`builtin_rules`]).
///
/// Diagnostics anchored inside a `lintel-ignore-next` suppression window
/// (recorded by the scanner, see `docs/ignore-directives.md`) are dropped
/// here — the single choke point through which every validation diagnostic
/// flows. Parse diagnostics (`LNT01xx`) never reach this pass, so a
/// directive cannot suppress them.
#[must_use]
pub fn validate(roots: &[Node], context: &ScanContext) -> Vec<Diagnostic> {
    validate_with(roots, context, BUILTIN_RULES)
}

/// Validate with an explicit rule registry (doc T-08 seam).
///
/// Rules run in slice order over a depth-first walk, then the
/// `lintel-ignore-next` filter applies to the combined output. A
/// `lintel-ignore-file` directive short-circuits before the walk: the
/// context's file-level flag drops every validation diagnostic without
/// running any rule, so a suppressed file pays no lint cost. Pass
/// [`builtin_rules`] for the default set, or a custom slice to extend,
/// reorder or replace it ([`crate::compile_with`] wires the same choice
/// into a full compile).
#[must_use]
pub fn validate_with(
    roots: &[Node],
    context: &ScanContext,
    rules: &[&dyn ValidationRule],
) -> Vec<Diagnostic> {
    if context.file_suppressed() {
        return vec![];
    }
    let mut problems: Vec<Diagnostic> = vec![];
    for root in roots {
        walk(root, rules, context, &mut problems);
    }
    problems.retain(|problem| !context.suppressed_at(problem.offset));
    problems
}

/// Depth-first walk: every rule inspects the node, then the children are
/// visited in source order — the traversal the built-in pass has always
/// used, now shared with injected rules.
fn walk(
    node: &Node,
    rules: &[&dyn ValidationRule],
    context: &ScanContext,
    out: &mut Vec<Diagnostic>,
) {
    for rule in rules {
        rule.check(node, context, out);
    }
    match node {
        Node::Element(element) => {
            for child in &element.children {
                walk(child, rules, context, out);
            }
        }
        Node::Fragment { children, .. } => {
            for child in children {
                walk(child, rules, context, out);
            }
        }
        Node::Text { .. } | Node::Expr { .. } => {}
    }
}

/// The `help:` line for an unknown prop: the declared parameter list plus the
/// spread escape hatch, mirroring [`Code::HyphenArg`]'s advice.
fn signature_help(bare: &str, signature: &Signature, name: &str) -> String {
    let params = if signature.params.is_empty() {
        "none".to_string()
    } else {
        signature
            .params
            .iter()
            .map(|param| format!("${param}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "`function {bare}()` declares: {params}; pass unknown keys through a \
         spread instead: `<{bare} {{...['{name}' => value]}} />`"
    )
}

fn last_segment(name: &str) -> &str {
    name.rsplit('\\').next().unwrap_or(name)
}

fn attr_offset(attr: &Attr) -> usize {
    match attr {
        Attr::Str { offset, .. }
        | Attr::Bool { offset, .. }
        | Attr::Expr { offset, .. }
        | Attr::Spread { offset, .. } => *offset,
    }
}

/// PHP identifier rules (ASCII subset): `[A-Za-z_][A-Za-z0-9_]*`.
fn is_php_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::{is_php_identifier, last_segment, BUILTINS};

    #[test]
    fn builtins_are_sorted_for_binary_search() {
        // `ShadowRule` looks builtins up with `binary_search`, which silently
        // misses names when the table stops being sorted; keep the table and
        // the lookup in lockstep.
        assert!(BUILTINS.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn last_segment_takes_the_namespace_tail() {
        assert_eq!(last_segment("Ui\\Button"), "Button");
        assert_eq!(last_segment("Card"), "Card");
    }

    #[test]
    fn php_identifier_rules() {
        assert!(is_php_identifier("title"));
        assert!(is_php_identifier("_private"));
        assert!(!is_php_identifier("data-x"));
        assert!(!is_php_identifier("xlink:href"));
        assert!(!is_php_identifier("@click"));
        assert!(!is_php_identifier(""));
    }

    #[test]
    fn spreads_have_no_name_but_named_attrs_do() {
        // The rules read names through the public `Attr::name` accessor
        // (`ast.rs`); the spread-is-nameless half of the contract is pinned
        // here because every named-argument rule depends on it.
        let spread = crate::ast::Attr::Spread {
            php: "$props".to_string(),
            line: 0,
            offset: 0,
        };
        assert!(spread.name().is_none());
        let named = crate::ast::Attr::Bool {
            name: "open".to_string(),
            line: 0,
            offset: 0,
        };
        assert_eq!(named.name(), Some("open"));
    }
}
