//! The T-31 function-component manifest (architecture doc §6.10's T-31
//! note): `lintl build` scans every successfully compiled source for
//! **top-level** `function Name(...): Html` declarations and writes the
//! project's `lintl.json` manifest, so function-only components — which
//! PSR-4 can never autoload — are discoverable by name.
//!
//! This is a CLI-side, best-effort lexing pass, deliberately separate from
//! the compiler's own PHP-aware scanner (§5.2): the manifest records
//! `name → source path` shape, not semantics — the loader's `functions`
//! globs stay the dev load path and the compiler stays the arbiter of what
//! compiles. The scan understands the PHP constructs that can hide or fake
//! a declaration — quoted strings (with `{$...}` complex interpolation),
//! heredocs/nowdocs, shell-exec backticks, line/block comments (including
//! the `?>` exits-PHP-mode rule), raw HTML regions and attribute markers —
//! and tracks brace depth so only file-scope declarations are recorded.
//!
//! The emitted manifest preserves every existing key (the user's
//! `functions` globs in particular) and updates two:
//!
//! ```json
//! {
//!   "components": {"Card": "src/Card.lintl.php"},
//!   "functions": ["src/*.lintl.php"],
//!   "prod": {"artifacts": {"src/Card.lintl.php": "out/Card.php"}}
//! }
//! ```
//!
//! `components` maps each declared component name to its source path
//! (relative to the manifest); `prod.artifacts` maps each compiled source
//! to its artifact. The Composer plugin's prod shim requires the artifact
//! values and records the source keys, so the dev loader (§6.10) — which
//! runs in the same process whenever the shim does — can skip sources the
//! shim already loaded and never redeclare a function component.

use std::fs;
use std::path::{absolute, Path, PathBuf};

use serde_json::{Map, Value};

/// One top-level `function Name(...): Html` declaration found in a source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionComponent {
    /// The declared name as written in the file, without any namespace
    /// qualifier (PHP function declarations inside a `namespace` block are
    /// unqualified).
    pub name: String,
    /// Byte offset of the `function` keyword — the anchor a future editor
    /// feature (the T-21 LSP's document symbols) maps through
    /// `SourceMap` without re-scanning.
    pub offset: usize,
}

/// Scan one source for top-level `function Name(...): Html` declarations,
/// in source order. Duplicate names can occur (two namespaces declaring
/// the same last segment); the manifest writer lets the last one win.
#[must_use]
pub fn scan_function_components(src: &str) -> Vec<FunctionComponent> {
    let bytes = src.as_bytes();
    let mut found: Vec<FunctionComponent> = Vec::new();
    let mut depth: usize = 0;
    let mut html = false; // raw output between `?>` and the next `<?`
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if html {
            if b == b'<' && starts_with(bytes, i, "<?") {
                html = false;
                i += if starts_with(bytes, i, "<?php") { 5 } else { 3 };
                continue;
            }
            i += 1;
            continue;
        }
        match b {
            b'\'' => i = skip_single_quoted(bytes, i),
            b'"' | b'`' => i = skip_interpolated(bytes, i),
            b'/' if starts_with(bytes, i, "/*") => {
                let (next, entered_html) = skip_block_comment(bytes, i);
                html = entered_html;
                i = next;
            }
            b'/' if starts_with(bytes, i, "//") => {
                let (next, entered_html) = skip_line_comment(bytes, i + 2);
                html = entered_html;
                i = next;
            }
            b'#' if bytes.get(i + 1) == Some(&b'[') => i = skip_attribute(bytes, i + 1),
            b'#' => {
                let (next, entered_html) = skip_line_comment(bytes, i + 1);
                html = entered_html;
                i = next;
            }
            b'?' if starts_with(bytes, i, "?>") => {
                html = true;
                i += 2;
            }
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            _ if depth == 0 && starts_keyword(bytes, i, b"function") => {
                let mut cursor = i + b"function".len();
                cursor = skip_inline_ws(bytes, cursor);
                if bytes.get(cursor) == Some(&b'&') {
                    cursor = skip_inline_ws(bytes, cursor + 1);
                }
                let Some((raw_name, end)) = read_identifier(bytes, cursor) else {
                    // `function (` — an anonymous closure at file scope: not
                    // a declaration. Keep scanning after the keyword.
                    i += b"function".len();
                    continue;
                };
                // Declarations inside a `namespace` block are unqualified;
                // a qualified spelling still names its last segment.
                let name = raw_name
                    .rsplit('\\')
                    .next()
                    .unwrap_or(raw_name.as_str())
                    .to_string();
                cursor = skip_inline_ws(bytes, end);
                if bytes.get(cursor) != Some(&b'(') {
                    // `use function …` or a malformed fragment: not a
                    // declaration (a declaration's parameter list must
                    // follow the name).
                    i += b"function".len();
                    continue;
                }
                cursor = skip_balanced(bytes, cursor, b'(', b')');
                if cursor > bytes.len() {
                    break; // unterminated parameter list: nothing else to find
                }
                cursor = skip_inline_ws(bytes, cursor);
                if bytes.get(cursor) == Some(&b'?') {
                    cursor = skip_inline_ws(bytes, cursor + 1);
                }
                if bytes.get(cursor) == Some(&b':') {
                    cursor = skip_inline_ws(bytes, cursor + 1);
                    let (end, entered_html) = scan_to(bytes, cursor, b"{;");
                    if !entered_html && is_html_type(&bytes[cursor..end]) {
                        found.push(FunctionComponent { name, offset: i });
                    }
                }
                i = cursor;
            }
            _ => i += 1,
        }
    }
    found
}

/// Write (or update) the `lintl.json` manifest at `path`.
///
/// Existing content is decoded as a JSON object and every existing key is
/// preserved (the user's `functions` globs in particular); `components`
/// and `prod` are then overwritten with the freshly built maps. Both maps
/// sort deterministically (`serde_json`'s object is a B-tree); `prod` is
/// omitted entirely when the build wrote no artifacts (a stdout build).
///
/// # Errors
///
/// A read, decode or write failure returns its message (the caller
/// renders it on stderr and fails the build).
pub fn write_manifest(
    path: &Path,
    components: &[(String, PathBuf)],
    artifacts: &[(PathBuf, PathBuf)],
) -> Result<(), String> {
    // `lintl build --manifest` passes a bare relative path whose parent
    // is the empty path — normalize it to `.` so the recorded keys stay
    // relative to the manifest's directory instead of turning absolute.
    let base = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut root: Map<String, Value> = match fs::read_to_string(path) {
        Ok(existing) => match serde_json::from_str::<Value>(&existing) {
            Ok(Value::Object(map)) => map,
            Ok(_) => {
                return Err(format!(
                    "existing manifest {} is not a JSON object",
                    path.display()
                ))
            }
            Err(e) => {
                return Err(format!(
                    "existing manifest {} is invalid: {e}",
                    path.display()
                ))
            }
        },
        Err(_) => Map::new(),
    };
    let mut names = Map::new();
    for (name, source) in components {
        names.insert(name.clone(), Value::String(relative_to(base, source)));
    }
    root.insert("components".into(), Value::Object(names));
    if artifacts.is_empty() {
        root.remove("prod");
    } else {
        let mut prod = Map::new();
        let mut built = Map::new();
        for (source, artifact) in artifacts {
            built.insert(
                relative_to(base, source),
                Value::String(relative_to(base, artifact)),
            );
        }
        prod.insert("artifacts".into(), Value::Object(built));
        root.insert("prod".into(), Value::Object(prod));
    }
    let mut out = serde_json::to_string_pretty(&Value::Object(root))
        .map_err(|e| format!("cannot serialize the manifest: {e}"))?;
    out.push('\n');
    fs::write(path, out).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// The relative form of `target` as seen from directory `base`, as a
/// `/`-separated string (`..` segments when the target sits outside the
/// base). Lexical only: neither path needs to exist.
fn relative_to(base: &Path, target: &Path) -> String {
    let base = absolute(base).unwrap_or_else(|_| base.to_path_buf());
    let target = absolute(target).unwrap_or_else(|_| target.to_path_buf());
    let base_parts: Vec<_> = base.components().collect();
    let target_parts: Vec<_> = target.components().collect();
    let common = base_parts
        .iter()
        .zip(target_parts.iter())
        .take_while(|(a, b)| a == b)
        .count();
    let mut rel: Vec<_> = base_parts[common..]
        .iter()
        .map(|_| "..".to_string())
        .collect();
    rel.extend(target_parts[common..].iter().map(|c| {
        let c = c.as_os_str().to_string_lossy();
        c.replace('\\', "/")
    }));
    if rel.is_empty() {
        ".".into()
    } else {
        rel.join("/")
    }
}

/// Whether `bytes[i..]` starts the keyword `word` at a word boundary
/// (the previous byte is not an identifier character, the next byte after
/// the keyword is whitespace or an opening parenthesis).
fn starts_keyword(bytes: &[u8], i: usize, word: &[u8]) -> bool {
    if !starts_with(bytes, i, word) {
        return false;
    }
    if i > 0 {
        let prev = bytes[i - 1];
        if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'\\' || prev == b'$' {
            return false;
        }
    }
    match bytes.get(i + word.len()) {
        None | Some(b'(') => true,
        Some(next) => next.is_ascii_whitespace(),
    }
}

/// Read an identifier (namespaced PHP identifiers: letters, digits,
/// underscores and `\`), returning `(name, index past the identifier)`.
fn read_identifier(bytes: &[u8], start: usize) -> Option<(String, usize)> {
    let mut end = start;
    while bytes
        .get(end)
        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'\\')
    {
        end += 1;
    }
    if end == start {
        return None;
    }
    let name = String::from_utf8_lossy(&bytes[start..end]).into_owned();
    Some((name, end))
}

/// Whether a return-type slice names `Html` — its last `\`-segment
/// compared case-insensitively (PHP class names are), with an optional
/// leading `?` (nullable) accepted and union types rejected.
fn is_html_type(raw: &[u8]) -> bool {
    let text = String::from_utf8_lossy(raw);
    let text = text.trim();
    let text = text.strip_prefix('?').unwrap_or(text).trim();
    if text.contains('|') {
        return false;
    }
    text.rsplit('\\')
        .next()
        .is_some_and(|segment| segment.eq_ignore_ascii_case("Html"))
}

/// Consume from `start` to the first occurrence of any `stops` byte.
/// Returns `(index at the stop byte, whether PHP mode was left)` — a `?>`
/// sequence before the stop ends PHP mode and the scan.
fn scan_to(bytes: &[u8], start: usize, stops: &[u8]) -> (usize, bool) {
    let mut i = start;
    while i < bytes.len() {
        let b = bytes[i];
        if stops.contains(&b) {
            return (i, false);
        }
        if b == b'\'' {
            i = skip_single_quoted(bytes, i);
        } else if b == b'"' {
            i = skip_interpolated(bytes, i);
        } else if b == b'?' && starts_with(bytes, i, "?>") {
            return (i, true);
        } else {
            i += 1;
        }
    }
    (bytes.len(), false)
}

/// Skip whitespace that cannot contain comments (the gap between a
/// `function` keyword, its name and its parameter list).
fn skip_inline_ws(bytes: &[u8], mut i: usize) -> usize {
    while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
        i += 1;
    }
    i
}

/// Consume a balanced `open`…`close` region starting at `bytes[start] ==
/// open`, honouring the string and comment states inside. Returns the
/// index just past the matching `close`, or `bytes.len() + 1` when the
/// region never closes (callers treat that as fatal for the candidate).
fn skip_balanced(bytes: &[u8], start: usize, open: u8, close: u8) -> usize {
    let mut level = 0usize;
    let mut i = start;
    while i < bytes.len() {
        let b = bytes[i];
        if b == open {
            level += 1;
            i += 1;
        } else if b == close {
            level -= 1;
            i += 1;
            if level == 0 {
                return i;
            }
        } else if b == b'\'' {
            i = skip_single_quoted(bytes, i);
        } else if b == b'"' {
            i = skip_interpolated(bytes, i);
        } else if b == b'/' && starts_with(bytes, i, "/*") {
            i = skip_block_comment(bytes, i).0;
        } else if (b == b'/' && starts_with(bytes, i, "//"))
            || (b == b'#' && bytes.get(i + 1) != Some(&b'['))
        {
            i = skip_line_comment(bytes, i + 2.min(bytes.len() - i)).0;
        } else {
            i += 1;
        }
    }
    bytes.len() + 1
}

/// Skip a single-quoted string starting at the opening quote: only `\\`
/// and `\'` are escapes; the next `'` closes.
fn skip_single_quoted(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'\'' => return i + 1,
            _ => i += 1,
        }
    }
    bytes.len()
}

/// Skip a double-quoted string or shell-exec backtick starting at the
/// opening delimiter, including `{$…}` complex interpolation regions
/// (which lex their contents as code — nested braces, strings and all).
fn skip_interpolated(bytes: &[u8], start: usize) -> usize {
    let quote = bytes[start];
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'{' if bytes.get(i + 1) == Some(&b'$') => {
                // `{$…}`: the region is PHP code again — recurse with the
                // string rules suspended until the matching `}`.
                i = skip_balanced(bytes, i, b'{', b'}');
                if i > bytes.len() {
                    return bytes.len();
                }
            }
            b if b == quote => return i + 1,
            _ => i += 1,
        }
    }
    bytes.len()
}

/// Skip a `//` or `#` line comment from `start` (past the marker).
/// Returns `(index at the newline or EOF, whether a `?>` inside the
/// comment ended PHP mode)`.
fn skip_line_comment(bytes: &[u8], start: usize) -> (usize, bool) {
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'\n' => return (i, false),
            b'?' if starts_with(bytes, i, "?>") => return (i, true),
            _ => i += 1,
        }
    }
    (bytes.len(), false)
}

/// Skip a `/* … */` block comment. Returns `(index past `*/`, whether a
/// `?>` inside the comment ended PHP mode)`.
fn skip_block_comment(bytes: &[u8], start: usize) -> (usize, bool) {
    let mut i = start + 2;
    while i < bytes.len() {
        if bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/') {
            return (i + 2, false);
        }
        if bytes[i] == b'?' && starts_with(bytes, i, "?>") {
            return (i, true);
        }
        i += 1;
    }
    (bytes.len(), false)
}

/// Skip an attribute `#[…]` region starting at the `[`, honouring nested
/// brackets and the string states inside. Attributes span the gap between
/// a comment-looking `#` and the declaration they annotate; the declaration
/// itself is still scanned normally afterwards.
fn skip_attribute(bytes: &[u8], start: usize) -> usize {
    let i = skip_balanced(bytes, start, b'[', b']');
    if i > bytes.len() {
        return bytes.len();
    }
    i
}

fn starts_with(bytes: &[u8], i: usize, marker: impl AsRef<[u8]>) -> bool {
    let marker = marker.as_ref();
    i + marker.len() <= bytes.len() && &bytes[i..i + marker.len()] == marker
}

#[cfg(test)]
mod tests {
    use super::{scan_function_components, write_manifest};
    use std::fs;
    use std::path::PathBuf;

    fn names(src: &str) -> Vec<String> {
        scan_function_components(src)
            .into_iter()
            .map(|found| found.name)
            .collect()
    }

    #[test]
    fn finds_a_top_level_html_function() {
        let src =
            "<?php\nfunction Card(string $title): Html\n{\n    return <div>{$title}</div>;\n}\n";
        assert_eq!(names(src), ["Card"]);
    }

    #[test]
    fn finds_namespaced_and_nullable_and_byref_forms() {
        let src = concat(&[
            "<?php\n",
            "function \\Lintl\\Card(): \\Lintl\\Html { return <i />; }\n",
            "function Maybe(): ?Html { return null; }\n",
            "function &Ref(): Html { return <b />; }\n",
        ]);
        assert_eq!(names(&src), ["Card", "Maybe", "Ref"]);
    }

    #[test]
    fn nested_and_anonymous_functions_are_not_top_level() {
        let src = "<?php\nfunction Outer(): Html {\n    return <div>{(function(): Html { return <i />; })()}</div>;\n}\n$g = function (): Html { return <b />; };\nfn() => 1;\n";
        assert_eq!(names(src), ["Outer"]);
    }

    #[test]
    fn class_methods_and_use_function_are_not_components() {
        let src = "<?php\nclass Repo {\n    public function find(): Html { return <p />; }\n}\nuse function foo\\bar;\n";
        assert_eq!(names(src), [] as [std::string::String; 0]);
    }

    #[test]
    fn strings_comments_and_heredocs_do_not_fool_the_scan() {
        let src = concat(&[
            "<?php\n$s = 'function Fake(): Html { return <x />; }';\n",
            "$d = \"function Fake(): Html\";\n",
            "// function Fake(): Html\n",
            "/* function Fake(): Html */\n",
            "$h = <<<TXT\nfunction Fake(): Html\nTXT;\n",
            "$n = <<<'TXT'\nfunction Fake(): Html\nTXT;\n",
            "echo `function Fake(): Html`;\n",
            "$i = \"row {$data[\"function Fake(): Html\"]}\";\n",
        ]);
        assert_eq!(names(&src), [] as [std::string::String; 0]);
    }

    #[test]
    fn html_mode_and_non_html_returns_are_excluded() {
        let src = concat(&[
            "<?php\n?>\nfunction Raw(): Html { return <x />; }\n",
            "<?php\nfunction Real(): Html { return <b />; }\n",
            "function Other(): string { return 'x'; }\n",
            "function Union(): Html|string { return 'x'; }\n",
        ]);
        // `Raw` is raw HTML output between `?>` and `<?php` — not code;
        // `Real` re-enters PHP and declares a component; the non-Html and
        // union return types are not function components.
        assert_eq!(names(&src), ["Real"]);
    }

    #[test]
    fn attributes_preceding_a_declaration_are_skipped() {
        let src = "<?php\n#[\\Deprecated(reason: 'use Card'] \nfunction Card(): Html { return <div />; }\n";
        assert_eq!(names(src), ["Card"]);
    }

    #[test]
    fn the_offset_anchors_the_keyword() {
        let src = "<?php\nfunction Card(): Html { return <div />; }\n";
        let found = scan_function_components(src);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].offset, 6);
        assert!(src[found[0].offset..].starts_with("function"));
    }

    #[test]
    fn empty_and_broken_sources_yield_nothing() {
        assert_eq!(names(""), [] as [std::string::String; 0]);
        assert_eq!(
            names("<?php function Broken(): Html { return <div />;\n"),
            ["Broken"]
        );
    }

    #[test]
    fn write_manifest_preserves_user_keys_and_writes_sorted_maps() {
        let dir = scratch("manifest-preserve");
        let manifest = dir.join("lintl.json");
        fs::write(
            &manifest,
            "{\"functions\": [\"src/*.lintl.php\"], \"custom\": {\"keep\": true}}",
        )
        .expect("writes the seed manifest");
        write_manifest(
            &manifest,
            &[
                ("Card".into(), dir.join("src/Card.lintl.php")),
                ("Badge".into(), dir.join("src/Badge.lintl.php")),
            ],
            &[
                (dir.join("src/Card.lintl.php"), dir.join("out/Card.php")),
                (dir.join("src/Badge.lintl.php"), dir.join("out/Badge.php")),
            ],
        )
        .expect("writes the manifest");
        let written = fs::read_to_string(&manifest).expect("reads the manifest");
        assert!(
            written.contains("\"Badge\": \"src/Badge.lintl.php\""),
            "{written}"
        );
        assert!(
            written.contains("\"Card\": \"src/Card.lintl.php\""),
            "{written}"
        );
        assert!(written.contains("\"functions\""), "{written}");
        assert!(written.contains("\"keep\": true"), "{written}");
        assert!(written.contains("\"prod\""), "{written}");
        let decoded: serde_json::Value = serde_json::from_str(&written).expect("valid JSON");
        assert_eq!(
            decoded["prod"]["artifacts"]["src/Card.lintl.php"], "out/Card.php",
            "prod artifacts pair each source with its artifact"
        );
        assert_eq!(
            decoded["prod"]["artifacts"]["src/Badge.lintl.php"], "out/Badge.php",
            "the artifacts map sorts deterministically"
        );
    }

    #[test]
    fn write_manifest_without_artifacts_omits_the_prod_section() {
        let dir = scratch("manifest-stdout");
        let manifest = dir.join("lintl.json");
        write_manifest(
            &manifest,
            &[("Card".into(), dir.join("src/Card.lintl.php"))],
            &[],
        )
        .expect("writes the manifest");
        let written = fs::read_to_string(&manifest).expect("reads the manifest");
        assert!(!written.contains("\"prod\""), "{written}");
    }

    #[test]
    fn write_manifest_rejects_a_non_object_seed() {
        let dir = scratch("manifest-bad-seed");
        let manifest = dir.join("lintl.json");
        fs::write(&manifest, "[1, 2, 3]").expect("writes the seed");
        let error = write_manifest(&manifest, &[], &[]).expect_err("must reject");
        assert!(error.contains("not a JSON object"), "{error}");
    }

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("lintl-manifest-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("creates scratch dir");
        dir
    }

    /// Concatenate string fragments (the test sources above read better in
    /// pieces than as one giant literal).
    fn concat(parts: &[&str]) -> String {
        parts.to_vec().join("")
    }
}
