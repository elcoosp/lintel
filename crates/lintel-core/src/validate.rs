//! Compile-time validation pass (architecture doc task T-08).
//!
//! Checks that need the whole tree or the file's declared-function context
//! run here, after scanning, before the compiled PHP is returned:
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
//! here; parse diagnostics never reach this pass, so a directive cannot
//! hide a misparse.
//!
//! The pass is pure: `validate` maps roots + context → diagnostics, and
//! [`crate::compile`] splits them by severity — errors fail the compilation,
//! warnings ride along on the output.

use crate::ast::{is_void_element, Attr, Element, Node};
use crate::scan::{ScanContext, Signature};
use lintel_diag::{Code, Diagnostic, Severity};

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

/// Validate every root (top-level and nested) against the scan context.
///
/// Diagnostics anchored inside a `lintel-ignore-next` suppression window
/// (recorded by the scanner, see `docs/ignore-directives.md`) are dropped
/// here — the single choke point through which every validation diagnostic
/// flows. Parse diagnostics (`LNT01xx`) never reach this pass, so a
/// directive cannot suppress them.
#[must_use]
pub fn validate(roots: &[Node], context: &ScanContext) -> Vec<Diagnostic> {
    let mut problems: Vec<Diagnostic> = vec![];
    for root in roots {
        check_node(root, context, &mut problems);
    }
    problems.retain(|problem| !context.suppressed_at(problem.offset));
    problems
}

fn check_node(node: &Node, context: &ScanContext, out: &mut Vec<Diagnostic>) {
    match node {
        Node::Element(element) => {
            check_element(element, context, out);
            for child in &element.children {
                check_node(child, context, out);
            }
        }
        Node::Fragment { children, .. } => {
            for child in children {
                check_node(child, context, out);
            }
        }
        Node::Text { .. } | Node::Expr { .. } => {}
    }
}

fn check_element(element: &Element, context: &ScanContext, out: &mut Vec<Diagnostic>) {
    if element.is_component {
        check_component(element, context, out);
    } else {
        check_html_element(element, out);
    }
}

fn check_component(element: &Element, context: &ScanContext, out: &mut Vec<Diagnostic>) {
    let bare = last_segment(&element.name);
    let lower = bare.to_lowercase();
    if !context.declares_function(&lower) && BUILTINS.contains(&lower.as_str()) {
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
    for attr in &element.attrs {
        let Some(name) = attr_name(attr) else {
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
                    "pass it through a spread instead: `<{bare} {{...['{name}' => value]}} />`"
                )),
            );
        }
    }
    // Signature checks only fire for components declared in this file: that
    // is the only place the parameter list is knowable (doc §17.1).
    if let Some(signature) = context.signature_of(&element.name) {
        check_signature(element, bare, signature, out);
    }
}

/// Check a component invocation against its declared in-file signature
/// (doc §17.1): an unknown prop and JSX children without a `children`
/// parameter are guaranteed runtime fatals (PHP rejects the call with
/// `Unknown named parameter`), so both fail the compile like `LNT0202`.
/// PHP named arguments are case-sensitive (unlike function calls), so the
/// parameter comparison is too.
fn check_signature(
    element: &Element,
    bare: &str,
    signature: &Signature,
    out: &mut Vec<Diagnostic>,
) {
    if signature.variadic {
        return; // `...$rest` accepts arbitrary named arguments
    }
    for attr in &element.attrs {
        let Some(name) = attr_name(attr) else {
            continue; // spreads bypass named-argument rules
        };
        if !is_php_identifier(name) {
            continue; // non-identifier props are LNT0202's report already
        }
        if !signature.params.iter().any(|param| param == name) {
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
    // Required parameters (declared without a default) that the invocation
    // does not supply are guaranteed PHP runtime fatals (`Argument ... not
    // given`), so they fail the compile like the other signature checks. A
    // spread may supply anything, so its presence disables the check rather
    // than risk a false positive; JSX children satisfy a `children`
    // parameter, and a named `children` prop satisfies it too.
    let has_spread = element
        .attrs
        .iter()
        .any(|attr| matches!(attr, Attr::Spread { .. }));
    if !has_spread {
        for param in &signature.required {
            let supplied = element
                .attrs
                .iter()
                .any(|attr| attr_name(attr).is_some_and(|name| name == param))
                || (param == "children" && !element.children.is_empty());
            if !supplied {
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
    if !element.children.is_empty() && !signature.params.iter().any(|param| param == "children") {
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

fn check_html_element(element: &Element, out: &mut Vec<Diagnostic>) {
    for attr in &element.attrs {
        match attr {
            Attr::Expr { name, .. } if name.to_ascii_lowercase().starts_with("on") => {
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
            Attr::Str { name, .. } if name.to_ascii_lowercase().starts_with("on") => {
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
            _ => {}
        }
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

fn last_segment(name: &str) -> &str {
    name.rsplit('\\').next().unwrap_or(name)
}

fn attr_name(attr: &Attr) -> Option<&str> {
    match attr {
        Attr::Str { name, .. } | Attr::Bool { name, .. } | Attr::Expr { name, .. } => Some(name),
        Attr::Spread { .. } => None,
    }
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
    use super::{is_php_identifier, last_segment};

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
}
