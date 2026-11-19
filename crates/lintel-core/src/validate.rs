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
//! - **`LNT0401` void children** — void elements cannot take children.
//!
//! The pass is pure: `validate` maps roots + context → diagnostics, and
//! [`crate::compile`] refuses to return output when any are found.

use crate::ast::{is_void_element, Attr, Element, Node};
use crate::scan::ScanContext;
use lintel_diag::{Code, Diagnostic};

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
#[must_use]
pub fn validate(roots: &[Node], context: &ScanContext) -> Vec<Diagnostic> {
    let mut problems: Vec<Diagnostic> = vec![];
    for root in roots {
        check_node(root, context, &mut problems);
    }
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
}

fn check_html_element(element: &Element, out: &mut Vec<Diagnostic>) {
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
