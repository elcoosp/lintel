//! The persistent workspace index (architecture doc §17.1 Q3: "persistent
//! workspace index, tree-sitter, cross-file definition"; the Phase 9
//! scale-and-hardening card): `initialize` walks the workspace root —
//! from the client's `workspaceFolders` or `rootUri` — and parses every
//! `.lintl.php` file's component declarations into an in-memory index,
//! backed by a lightweight on-disk cache keyed by file mtime+size so a
//! fresh session skips re-parsing unchanged files.
//!
//! This is the seam the open-documents-only workspace (§15 v0.5) could not
//! cross: a component declared in a closed file is invisible to rename,
//! references, workspace symbols and definition. The index lifts that —
//! rename/references/symbols consult the index in addition to the open
//! documents, and definition resolves a `<Component />` tag to a closed
//! file's declaration location.
//!
//! Freshness: every index access re-validates the file's mtime+size (the
//! same facts the dev loader's compile-if-stale lane uses — mtime alone
//! lies after a git checkout, content hashing is the build cache's M2
//! job); a mismatched entry is evicted and re-parsed from the disk bytes.
//! Files synced by the client (`didOpen`/`didChange`/`didSave`) are
//! re-parsed from the synced text immediately — the buffer is the truth
//! while the file is open — and their entries are marked disk-unknown so
//! the next disk access re-validates from the file itself.
//!
//! The walk skips dot-directories (`.git`, `.lintl-cache`, …) and the
//! conventional dependency directories (`vendor`, `node_modules`): vendored
//! fixture sources are not workspace components any more than they are
//! project sources for the dev loader.
//!
//! The on-disk cache (`.lintl-cache/lsp-index.json`) is best-effort on
//! both ends: an unreadable/unwritable cache never fails a session — the
//! index just re-parses.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde_json::{json, Value};

use crate::tree_sitter::{self, TsScan};

/// The on-disk cache's shape version — a mismatch resets the cache (the
/// index re-parses; the file is rewritten on the next persist). Version 2
/// added the per-entry tree-sitter scan (the Phase 11 lane): v1 caches
/// reset once, transparently.
const CACHE_VERSION: u64 = 2;

/// The persist's write goes through an exclusive `flock(2)` (fs2:
/// `flock` on Unix, `LockFileEx` on Windows): two concurrent LSP sessions
/// over one workspace both rewrite `.lintl-cache/lsp-index.json`, and an
/// interleaved plain write would leave one session's loader a truncated
/// mix. Best-effort like the persist itself: any failure degrades to the
/// unlocked write the previous sessions shipped, never to a failed
/// session.
fn lock_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        // No truncate at open: an open-time truncate of a locked writer's
        // file would punch a hole mid-write. The length resets under the
        // lock, where the racing writer is serialized out.
        .open(path)?;
    // fs2's trait methods are called EXPLICITLY: std's own `File` locking
    // (stable 1.89) would win inherent-method resolution and break the
    // workspace MSRV (1.80).
    fs2::FileExt::lock_exclusive(&file)?;
    // The length resets under the lock: a previous longer write must not
    // leave stale tail bytes past this write's length.
    file.set_len(0)?;
    file.write_all(bytes)?;
    fs2::FileExt::unlock(&file)?;
    Ok(())
}

/// One component declaration indexed from a workspace file: the scan
/// table's facts the rename/references/symbols/definition queries read
/// (name, identifier byte offset, `$param` names + offsets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IndexedComponent {
    /// The declared name, as written (last path segment).
    pub(crate) name: String,
    /// Absolute byte offset of the declared name identifier.
    pub(crate) offset: usize,
    /// Parameter names in declaration order paired with the absolute byte
    /// offset of each name's `$` — the same shape `Declaration::params`
    /// carries.
    pub(crate) params: Vec<(String, usize)>,
}

/// One indexed file's declarations plus the freshness facts that decide
/// whether the entry still describes the source it was parsed from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    /// The file's absolute path (the disk side of its URI).
    path: PathBuf,
    /// Last-modified time as `(unix seconds, nanoseconds)`; `None` when
    /// the entry came from a client buffer sync (the disk state is
    /// unknown until the next disk access re-validates it).
    mtime: Option<(u64, u32)>,
    /// The file size in bytes at parse time (`None` alongside `mtime`).
    size: Option<u64>,
    /// The lintl-core scanner's declarations (the JSX-structure
    /// authority).
    declarations: Vec<IndexedComponent>,
    /// The independent tree-sitter parse (the Phase 11 lane): its own
    /// declaration view plus which lane served the file. Persisted
    /// alongside the scanner facts.
    ts: TsScan,
}

/// The workspace index: the walked file set with their declarations.
#[derive(Debug, Default)]
pub(crate) struct WorkspaceIndex {
    /// The workspace root, when `initialize` named one.
    root: Option<PathBuf>,
    /// URI string → entry, covering every walked `.lintl.php` file plus
    /// every client-synced document (open files ride the index too).
    files: HashMap<String, Entry>,
}

impl WorkspaceIndex {
    /// Build the index for the workspace root `initialize` named (the
    /// first `workspaceFolders` entry's URI, else `rootUri`). No root —
    /// no walk (a single-file client sees the open documents only, like
    /// the v0.5 seam).
    ///
    /// The walk collects `.lintl.php` files — through the `lintl.json`
    /// `lsp.include` glob patterns when the workspace configures them
    /// (the monorepo override), else the default walk (dot-directories
    /// and the dependency directories skipped) — loads the on-disk cache,
    /// keeps entries whose mtime+size still match, parses the rest, and
    /// persists the refreshed map best-effort.
    pub(crate) fn for_root(root_uri: Option<&str>) -> Self {
        let Some(root) = root_uri.and_then(uri_to_path) else {
            return Self::default();
        };
        let cached = load_cache(&root).unwrap_or_default();
        let mut files: HashMap<String, Entry> = HashMap::new();
        let mut paths: Vec<PathBuf> = Vec::new();
        match workspace_include_patterns(&root) {
            Some(patterns) => walk_with_includes(&root, &patterns, &mut paths),
            None => walk(&root, &mut paths),
        }
        for path in paths {
            let Some(uri) = path_to_uri(&path) else {
                continue;
            };
            // Cache hit: the stored entry's mtime+size still match a fresh
            // metadata probe — the cached declarations still describe the
            // file, and no re-parse happens.
            if let Ok(meta) = fs::metadata(&path) {
                if let Some(entry) = cached.get(&uri) {
                    if entry.matches(&meta) {
                        // Cache hit: mtime+size are unchanged — the stored
                        // declarations still describe the file.
                        files.insert(uri, entry.to_entry(path));
                        continue;
                    }
                }
            }
            let Some((declarations, ts, facts)) = parse_from_disk(&path) else {
                continue; // unreadable or non-UTF8: not indexable
            };
            files.insert(
                uri,
                Entry {
                    path,
                    mtime: Some((facts.0, facts.1)),
                    size: Some(facts.2),
                    declarations,
                    ts,
                },
            );
        }
        let index = Self {
            root: Some(root.clone()),
            files,
        };
        index.persist();
        index
    }

    /// The indexed URIs, sorted (every workspace walk is deterministic
    /// regardless of the map's hash order).
    pub(crate) fn uris(&self) -> Vec<String> {
        let mut uris: Vec<String> = self.files.keys().cloned().collect();
        uris.sort();
        uris
    }

    /// The fresh text + declarations of one indexed file: the disk bytes
    /// are re-read, the entry's mtime+size re-validated — a stale entry is
    /// evicted and re-parsed from the fresh bytes ("stale entries are
    /// evicted by mtime+size on next access"). Buffer-synced entries
    /// (`mtime: None`) re-validate the same way: the disk is the truth
    /// for a closed file. Returns `None` when the URI is unindexed or the
    /// file became unreadable.
    pub(crate) fn fresh_text(&mut self, uri: &str) -> Option<(String, Vec<IndexedComponent>)> {
        let path = self.files.get(uri)?.path.clone();
        let text = fs::read_to_string(&path).ok()?;
        let fresh = fs::metadata(&path)
            .ok()
            .and_then(|meta| self.files.get(uri).map(|entry| entry.matches(&meta)))
            .unwrap_or(false);
        if fresh {
            let declarations = self
                .files
                .get(uri)
                .map(|entry| entry.declarations.clone())
                .unwrap_or_default();
            return Some((text, declarations));
        }
        let Some((declarations, ts, facts)) = parse_from_disk(&path) else {
            // The file became unreadable or vanished: nothing this URI can
            // contribute — evict the entry like any other stale fact.
            self.files.remove(uri);
            return None;
        };
        self.files.insert(
            uri.to_string(),
            Entry {
                path,
                mtime: Some((facts.0, facts.1)),
                size: Some(facts.2),
                declarations: declarations.clone(),
                ts,
            },
        );
        self.persist();
        Some((text, declarations))
    }

    /// Re-parse one client-synced document into the index: the synced text
    /// is the truth while the file is open, so the entry's declarations
    /// update immediately and its disk facts reset to unknown (the next
    /// disk access re-validates). Called on `didOpen`, `didChange` (full
    /// sync) and `didSave`.
    pub(crate) fn upsert_from_text(&mut self, uri: &str, text: &str) {
        let path = match self.files.get(uri) {
            Some(entry) => entry.path.clone(),
            None => match uri_to_path(uri) {
                Some(path) => path,
                None => return,
            },
        };
        let declarations = scan_declarations(text);
        let ts = tree_sitter::scan(text);
        self.files.insert(
            uri.to_string(),
            Entry {
                path,
                mtime: None,
                size: None,
                declarations,
                ts,
            },
        );
    }

    /// The merged declaration view of one workspace file (the tree-sitter
    /// lane unioned with the scanner's facts — [`tree_sitter::merge`]) on
    /// its freshness-validated text. This is the accessor the cross-file
    /// queries (definition, rename, references, symbols) read: a
    /// declaration only one lane sees still serves the workspace.
    pub(crate) fn merged_file(&mut self, uri: &str) -> Option<(String, Vec<IndexedComponent>)> {
        let (text, declarations) = self.fresh_text(uri)?;
        let merged = match self.files.get(uri) {
            Some(entry) => tree_sitter::merge(declarations, &entry.ts),
            None => declarations,
        };
        Some((text, merged))
    }

    /// Persist the whole index to `.lintl-cache/lsp-index.json`,
    /// best-effort: buffer-synced entries (`mtime: None`) are session-local
    /// facts and are NOT written — only disk-validated entries are.
    fn persist(&self) {
        let Some(root) = &self.root else {
            return;
        };
        let mut files = serde_json::Map::new();
        for (uri, entry) in &self.files {
            let Some((mtime, size)) = entry.mtime.zip(entry.size) else {
                continue;
            };
            files.insert(
                uri.clone(),
                json!({
                    "mtime": mtime.0,
                    "nanos": mtime.1,
                    "size": size,
                    "declarations": declarations_json(&entry.declarations),
                    "ts": entry.ts.to_json(),
                }),
            );
        }
        let cache_dir = root.join(".lintl-cache");
        if fs::create_dir_all(&cache_dir).is_err() {
            return;
        }
        let body = json!({"version": CACHE_VERSION, "files": Value::Object(files)});
        let _ = lock_write(
            &cache_dir.join("lsp-index.json"),
            body.to_string().as_bytes(),
        );
    }
}

/// One file's freshness facts: `(mtime seconds, nanos, size)`.
type Facts = (u64, u32, u64);

/// The entry's freshness facts against a fresh metadata probe.
impl Entry {
    fn matches(&self, meta: &fs::Metadata) -> bool {
        match (self.mtime, self.size) {
            (Some(mtime), Some(size)) => {
                file_facts_from_meta(meta).is_some_and(|facts| facts == (mtime.0, mtime.1, size))
            }
            _ => false,
        }
    }
}

/// One disk parse's both lanes' views plus the freshness facts they were
/// parsed under.
type DiskParse = (Vec<IndexedComponent>, TsScan, Facts);

/// Parse one file's declarations from the disk bytes, returning both
/// lanes' views (scanner + tree-sitter) plus the freshness facts they
/// were parsed under.
fn parse_from_disk(path: &Path) -> Option<DiskParse> {
    let text = fs::read_to_string(path).ok()?;
    let facts = file_facts(path)?;
    let (mtime, size) = facts;
    Some((
        scan_declarations(&text),
        tree_sitter::scan(&text),
        (mtime.0, mtime.1, size),
    ))
}

/// Scan one source's component declarations into the index's shape — the
/// same `ScanContext` table the compiler validates against. Shared with
/// the open-document arm of the server's workspace accessor.
pub(crate) fn scan_declarations(text: &str) -> Vec<IndexedComponent> {
    let parsed = lintl_core::scan::scan(text);
    parsed
        .context
        .declared_signatures()
        .filter_map(|(name, _signature)| {
            let declaration = parsed.context.declaration_of(name)?;
            Some(IndexedComponent {
                name: declaration.name.clone(),
                offset: declaration.offset,
                params: declaration.params.clone(),
            })
        })
        .collect()
}

/// A file's freshness facts: `Some((mtime secs, nanos, size))`, or `None`
/// when the file or its metadata is unreadable.
fn file_facts(path: &Path) -> Option<((u64, u32), u64)> {
    let meta = fs::metadata(path).ok()?;
    let (mtime, size) = file_facts_from_meta(&meta).map(|facts| ((facts.0, facts.1), facts.2))?;
    Some((mtime, size))
}

/// The freshness facts from one metadata probe.
fn file_facts_from_meta(meta: &fs::Metadata) -> Option<Facts> {
    let modified = meta.modified().ok()?;
    let since_epoch = modified.duration_since(UNIX_EPOCH).ok()?;
    Some((
        since_epoch.as_secs(),
        since_epoch.subsec_nanos(),
        meta.len(),
    ))
}

/// Load the on-disk cache map (URI → cached entry), `None` when the cache
/// is absent, unreadable, or of another shape version — the caller then
/// re-parses everything.
fn load_cache(root: &Path) -> Option<HashMap<String, CachedEntry>> {
    let body = fs::read_to_string(root.join(".lintl-cache").join("lsp-index.json")).ok()?;
    let value: Value = serde_json::from_str(&body).ok()?;
    if value.get("version").and_then(Value::as_u64) != Some(CACHE_VERSION) {
        return None;
    }
    let files = value.get("files")?.as_object()?;
    let mut out = HashMap::new();
    for (uri, entry) in files {
        let mtime = entry.get("mtime")?.as_u64()?;
        let nanos = u32::try_from(entry.get("nanos")?.as_u64()?).ok()?;
        let size = entry.get("size")?.as_u64()?;
        let declarations = entry.get("declarations")?.as_array()?;
        let declarations: Vec<IndexedComponent> =
            declarations_from_json(&Value::Array(declarations.clone()));
        let ts = TsScan::from_json(entry.get("ts")?)?;
        out.insert(
            uri.clone(),
            CachedEntry {
                mtime: (mtime, nanos),
                size,
                declarations,
                ts,
            },
        );
    }
    Some(out)
}

/// The cache-loaded shape of one entry (no disk path yet — it is
/// re-anchored on hit).
#[derive(Debug, Clone)]
struct CachedEntry {
    mtime: (u64, u32),
    size: u64,
    declarations: Vec<IndexedComponent>,
    ts: TsScan,
}

impl CachedEntry {
    /// Whether the cached freshness facts still match a fresh probe.
    fn matches(&self, meta: &fs::Metadata) -> bool {
        file_facts_from_meta(meta)
            .is_some_and(|facts| facts == (self.mtime.0, self.mtime.1, self.size))
    }

    /// Anchor a cache-loaded entry to its file's path (the cache stores
    /// URIs; the entry needs the disk side).
    fn to_entry(&self, path: PathBuf) -> Entry {
        Entry {
            path,
            mtime: Some(self.mtime),
            size: Some(self.size),
            declarations: self.declarations.clone(),
            ts: self.ts.clone(),
        }
    }
}

/// The declarations as the cache file's JSON rows (shared with the
/// tree-sitter lane's persisted scan).
pub(crate) fn declarations_json(declarations: &[IndexedComponent]) -> Value {
    Value::Array(
        declarations
            .iter()
            .map(|declaration| {
                json!({
                    "name": declaration.name,
                    "offset": declaration.offset,
                    "params": declaration
                        .params
                        .iter()
                        .map(|(name, offset)| json!([name, offset]))
                        .collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

/// The declarations a cache row carries; an empty list for an
/// absent/malformed row (the lane re-parses the file on the next access).
pub(crate) fn declarations_from_json(value: &Value) -> Vec<IndexedComponent> {
    let Some(rows) = value.as_array() else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|declaration| {
            Some(IndexedComponent {
                name: declaration.get("name")?.as_str()?.to_string(),
                offset: usize::try_from(declaration.get("offset")?.as_u64()?).ok()?,
                params: declaration
                    .get("params")?
                    .as_array()?
                    .iter()
                    .filter_map(|param| {
                        let pair = param.as_array()?;
                        Some((
                            pair.first()?.as_str()?.to_string(),
                            usize::try_from(pair.get(1)?.as_u64()?).ok()?,
                        ))
                    })
                    .collect(),
            })
        })
        .collect()
}

/// Recursively collect the `.lintl.php` files under `root`, sorted per
/// directory for a deterministic walk. Dot-directories (`.git`,
/// `.lintl-cache`, …) and the dependency directories (`vendor`,
/// `node_modules`) are skipped: vendored fixture sources are not workspace
/// components.
fn walk(root: &Path, paths: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let mut entries: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    for path in entries {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        let Some(name) = name else { continue };
        if name.starts_with('.') || name == "vendor" || name == "node_modules" {
            continue;
        }
        if path.is_dir() {
            walk(&path, paths);
        } else if name.ends_with(".lintl.php") {
            paths.push(path);
        }
    }
}

/// The `lintl.json` `lsp.include` patterns: `{"lsp": {"include":
/// ["packages/*/src/**/*.lintl.php"]}}` — read relative to the workspace
/// root. `None` when the file, the `lsp` object or the list is absent or
/// unparseable (the default walk); a present-but-EMPTY list indexes
/// nothing (the configuration says what to walk, and it says nothing).
/// Non-string entries are ignored; an all-non-string list is empty.
fn workspace_include_patterns(root: &Path) -> Option<Vec<String>> {
    let body = fs::read_to_string(root.join("lintl.json")).ok()?;
    let value: Value = serde_json::from_str(&body).ok()?;
    let include = value.get("lsp")?.get("include")?.as_array()?;
    let patterns: Vec<String> = include
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    Some(patterns)
}

/// Whether `relative` (a `/`-separated path, root-relative) matches one
/// `lsp.include` glob: `**` spans directory levels, `*` matches any run
/// of characters within one segment, `?` exactly one. Matching is
/// byte-wise and case-sensitive (the walk is deterministic; patterns name
/// the components as written on disk).
fn glob_matches(pattern: &str, relative: &str) -> bool {
    let pattern: Vec<&str> = pattern.split('/').collect();
    let path: Vec<&str> = relative.split('/').collect();
    segments_match(&pattern, &path)
}

/// Segment-wise glob matching: `**` matches zero or more whole segments
/// (the recursion either skips it or consumes one path segment), any
/// other pattern segment must match its path segment exactly.
fn segments_match(pattern: &[&str], path: &[&str]) -> bool {
    match (pattern.first(), path.first()) {
        (None, None) => true,
        (Some(&"**"), _) => {
            segments_match(&pattern[1..], path)
                || (!path.is_empty() && segments_match(pattern, &path[1..]))
        }
        (Some(pattern_segment), Some(path_segment)) => {
            segment_matches(pattern_segment, path_segment)
                && segments_match(&pattern[1..], &path[1..])
        }
        _ => false,
    }
}

/// One path segment's wildcard match: `*` (zero or more bytes), `?`
/// (exactly one byte), everything else byte-equal. No escaping — the
/// include list names source paths, not regexes.
fn segment_matches(pattern: &str, segment: &str) -> bool {
    fn inner(pattern: &[u8], segment: &[u8]) -> bool {
        match (pattern.first(), segment.first()) {
            (None, None) => true,
            (Some(b'*'), _) => {
                inner(&pattern[1..], segment)
                    || (!segment.is_empty() && inner(pattern, &segment[1..]))
            }
            (Some(b'?'), Some(_)) => inner(&pattern[1..], &segment[1..]),
            (Some(a), Some(b)) if a == b => inner(&pattern[1..], &segment[1..]),
            _ => false,
        }
    }
    inner(pattern.as_bytes(), segment.as_bytes())
}

/// Recursively collect the `.lintl.php` files under `root` whose
/// root-relative path matches ANY of the `lsp.include` glob patterns —
/// the monorepo override (the Phase 9 §f backlog): only the configured
/// patterns walk in. The dependency and dot-directory skips stay (they
/// are properties of the walk, not of the selection — an explicit pattern
/// cannot re-include vendored dependencies), the `.lintl.php` suffix
/// requirement stays (the index parses Lintl sources), and the walk
/// stays sorted per directory for determinism.
fn walk_with_includes(root: &Path, patterns: &[String], paths: &mut Vec<PathBuf>) {
    walk_includes_at(root, root, patterns, paths);
}

/// The per-directory recursion of [`walk_with_includes`].
fn walk_includes_at(root: &Path, dir: &Path, patterns: &[String], paths: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    for path in entries {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        let Some(name) = name else { continue };
        if name.starts_with('.') || name == "vendor" || name == "node_modules" {
            continue;
        }
        if path.is_dir() {
            walk_includes_at(root, &path, patterns, paths);
        } else if name.ends_with(".lintl.php") {
            let relative: Vec<String> = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .components()
                .map(|component| component.as_os_str().to_string_lossy().into_owned())
                .collect();
            let relative = relative.join("/");
            if patterns
                .iter()
                .any(|pattern| glob_matches(pattern, &relative))
            {
                paths.push(path);
            }
        }
    }
}

/// The filesystem path of a `file://` workspace URI (minimal percent
/// decoding on the path portion; the query/fragment split a file URI never
/// carries ends the path). `None` for non-`file` schemes — only real
/// workspace directories are indexable.
pub(crate) fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    // A URI authority (file://host/…) is not a local path shape we index.
    let path = rest.split(['?', '#']).next()?;
    let decoded = percent_decode(path)?;
    Some(PathBuf::from(decoded))
}

/// The `file://` URI of a filesystem path, minimally percent-encoding the
/// bytes a URI path must escape (everything outside the unreserved set and
/// `/`), so spaces and non-ASCII names round-trip through `parse_uri`.
pub(crate) fn path_to_uri(path: &Path) -> Option<String> {
    use std::fmt::Write as _;

    let raw = path.to_str()?;
    let mut uri = String::with_capacity(raw.len() + 8);
    uri.push_str("file://");
    for byte in raw.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                uri.push(byte as char);
            }
            _ => {
                let _ = write!(uri, "%{byte:02X}");
            }
        }
    }
    Some(uri)
}

/// Minimal percent decoding (`%XX` → byte, pass-through otherwise); `None`
/// on a malformed escape.
fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = bytes.get(index + 1..index + 3)?;
            let value = u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?;
            out.push(value);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::{path_to_uri, uri_to_path, WorkspaceIndex};
    use std::fs;
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("lintl-lsp-index-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("creates scratch dir");
        dir
    }

    const DECLARING: &str = concat!(
        "<?php\n\n",
        "function Badge(string $label): Html\n{\n    return <b>{$label}</b>;\n}\n"
    );

    /// The buffer-synced edit the buffer-sync test drives (`Badge` →
    /// `Edited`).
    const EDITED: &str = "<?php\nfunction Edited(): Html\n{\n    return <em />;\n}\n";

    #[test]
    fn file_uri_round_trips_through_minimal_encoding() {
        let plain = PathBuf::from("/tmp/ws/Salute.lintl.php");
        let uri = path_to_uri(&plain).expect("uri");
        assert_eq!(uri, "file:///tmp/ws/Salute.lintl.php");
        assert_eq!(uri_to_path(&uri), Some(plain));

        let spaced = PathBuf::from("/tmp/my ws/Salute.lintl.php");
        let uri = path_to_uri(&spaced).expect("uri");
        assert_eq!(uri, "file:///tmp/my%20ws/Salute.lintl.php");
        assert_eq!(uri_to_path(&uri), Some(spaced));
    }

    #[test]
    fn non_file_schemes_are_not_indexable() {
        assert_eq!(uri_to_path("untitled:Untitled-1"), None);
        assert_eq!(uri_to_path("https://example.com/x.lintl.php"), None);
    }

    #[test]
    fn a_rootless_initialize_indexes_nothing() {
        let index = WorkspaceIndex::for_root(None);
        assert!(index.uris().is_empty(), "no root — no walk");
    }

    #[test]
    fn the_walk_indexes_every_lintl_source_with_declarations() {
        let root = scratch("walk");
        fs::create_dir_all(root.join("src/deep")).expect("dirs");
        fs::write(root.join("src/Badge.lintl.php"), DECLARING).expect("writes");
        fs::write(root.join("src/deep/Page.lintl.php"), DECLARING).expect("writes");
        fs::write(root.join("README.md"), "not a source").expect("writes");
        fs::write(root.join("src/skip.txt"), DECLARING).expect("writes");

        let mut index = WorkspaceIndex::for_root(Some(&super::path_to_uri(&root).expect("uri")));
        let uris = index.uris();
        assert_eq!(uris.len(), 2, "only .lintl.php sources walk in: {uris:?}");
        let (text, badge) = index
            .merged_file(&super::path_to_uri(&root.join("src/Badge.lintl.php")).expect("uri"))
            .expect("indexed");
        assert_eq!(text, DECLARING);
        assert_eq!(badge.len(), 1);
        assert_eq!(badge[0].name, "Badge");
        assert_eq!(
            badge[0].params,
            vec![(
                "label".to_string(),
                DECLARING.find("$label").expect("$offset")
            )]
        );
    }

    #[test]
    fn vendored_and_hidden_directories_do_not_walk_in() {
        let root = scratch("vendored");
        fs::create_dir_all(root.join("vendor/acme")).expect("dirs");
        fs::create_dir_all(root.join(".git/hooks")).expect("dirs");
        fs::write(root.join("vendor/acme/Remote.lintl.php"), DECLARING).expect("writes");
        fs::write(root.join(".git/hooks/Fake.lintl.php"), DECLARING).expect("writes");
        fs::write(root.join("Local.lintl.php"), DECLARING).expect("writes");

        let index = WorkspaceIndex::for_root(Some(&super::path_to_uri(&root).expect("uri")));
        assert_eq!(
            index.uris().len(),
            1,
            "only the local source: {:?}",
            index.uris()
        );
    }

    #[test]
    fn fresh_text_validates_mtime_and_size_and_reparses_stale_entries() {
        let root = scratch("stale");
        let source = root.join("Badge.lintl.php");
        fs::write(&source, DECLARING).expect("writes");
        let uri = super::path_to_uri(&source).expect("uri");
        let mut index = WorkspaceIndex::for_root(Some(&super::path_to_uri(&root).expect("uri")));

        let (text, declarations) = index.fresh_text(&uri).expect("fresh");
        assert_eq!(declarations.len(), 1);
        assert_eq!(declarations[0].name, "Badge");
        assert_eq!(text, DECLARING, "the disk bytes come back for hit mapping");

        // The file changes on disk (new declarations): the stale entry is
        // evicted by mtime+size and re-parsed on the next access.
        fs::write(
            &source,
            concat!(
                "<?php\n\n",
                "function Renamed(string $label): Html\n{\n    return <b>{$label}</b>;\n}\n",
                "function Extra(): Html\n{\n    return <i />;\n}\n"
            ),
        )
        .expect("rewrites");
        let (_text, declarations) = index.fresh_text(&uri).expect("fresh after change");
        let names: Vec<&str> = declarations.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(
            names,
            ["Renamed", "Extra"],
            "the stale entry was evicted: {declarations:?}"
        );
    }

    #[test]
    fn the_on_disk_cache_survives_a_session_and_honors_changes() {
        let root = scratch("cache");
        let source = root.join("Badge.lintl.php");
        fs::write(&source, DECLARING).expect("writes");
        let uri = super::path_to_uri(&source).expect("uri");

        // Session 1: the walk persists the cache file.
        let index = WorkspaceIndex::for_root(Some(&super::path_to_uri(&root).expect("uri")));
        assert!(
            root.join(".lintl-cache/lsp-index.json").is_file(),
            "the cache persists"
        );
        drop(index);

        // Session 2 over the unchanged workspace: the cache round-trips —
        // declarations come back without a re-parse.
        let mut index = WorkspaceIndex::for_root(Some(&super::path_to_uri(&root).expect("uri")));
        let (_text, badge) = index.merged_file(&uri).expect("cache-hit entry");
        assert_eq!(badge[0].name, "Badge");

        // Session 3 after an edit: the stale cache entry is evicted.
        fs::write(
            &source,
            "<?php\nfunction Later(): Html\n{\n    return <u />;\n}\n",
        )
        .expect("rewrites");
        let mut index = WorkspaceIndex::for_root(Some(&super::path_to_uri(&root).expect("uri")));
        let (_text, badge) = index.merged_file(&uri).expect("refreshed entry");
        assert_eq!(
            badge[0].name, "Later",
            "mtime+size eviction crosses sessions"
        );
    }

    #[test]
    fn buffer_synced_entries_update_immediately_and_revalidate_on_disk_access() {
        let root = scratch("buffer");
        let source = root.join("Badge.lintl.php");
        fs::write(&source, DECLARING).expect("writes");
        let uri = super::path_to_uri(&source).expect("uri");
        let mut index = WorkspaceIndex::for_root(Some(&super::path_to_uri(&root).expect("uri")));

        // A client sync (didChange/didSave full sync): the synced text
        // parses into the entry immediately — both lanes ride the synced
        // text. The observable at the index level: the synced text's
        // declarations serve the merged view once the disk matches it
        // (the buffer-is-truth-while-open contract is the SERVER's — the
        // documents arm answers from the live buffer, covered by the
        // lsp.rs scenarios).
        index.upsert_from_text(&uri, EDITED);
        fs::write(&source, EDITED).expect("writes the synced text");
        let edited = index.merged_file(&uri).expect("entry").1;
        assert_eq!(
            edited[0].name, "Edited",
            "the synced text re-parses immediately"
        );

        // The disk changes back behind the synced entry: a fresh
        // (closed-file) access re-validates from the disk and restores
        // the disk truth.
        fs::write(&source, DECLARING).expect("rewrites the disk");
        let (_text, declarations) = index.fresh_text(&uri).expect("fresh");
        assert_eq!(
            declarations[0].name, "Badge",
            "the disk re-validates the entry"
        );
    }

    #[test]
    fn the_tree_sitter_lane_persists_with_the_entry_and_survives_a_session() {
        // The Phase 11 lane: every entry carries its tree-sitter scan, and
        // the merged view (scanner ∪ tree-sitter) rides the on-disk cache
        // into the next session — a cache-hit entry still serves the
        // tree-sitter facts without a re-parse.
        let root = scratch("ts-lane");
        let source = root.join("Badge.lintl.php");
        fs::write(&source, DECLARING).expect("writes");
        let uri = super::path_to_uri(&source).expect("uri");

        let mut index = WorkspaceIndex::for_root(Some(&super::path_to_uri(&root).expect("uri")));
        let (text, declarations) = index.merged_file(&uri).expect("merged");
        assert_eq!(text, DECLARING);
        assert_eq!(declarations.len(), 1, "the healthy pair dedupes");
        let cache_body =
            fs::read_to_string(root.join(".lintl-cache/lsp-index.json")).expect("cache exists");
        assert!(
            cache_body.contains("\"lane\":\"tree-sitter\""),
            "the lane rides the cache row"
        );
        drop(index);

        let mut index = WorkspaceIndex::for_root(Some(&super::path_to_uri(&root).expect("uri")));
        let (_text, declarations) = index.merged_file(&uri).expect("cache-hit merged");
        assert_eq!(declarations.len(), 1);
        assert_eq!(declarations[0].name, "Badge");
    }

    #[test]
    fn a_fallback_laned_file_still_serves_the_scanner_facts_through_the_merge() {
        // A broken declaration header: tree-sitter sees no usable
        // declarations (the fallback lane), the scanner tolerates the
        // malformed parameter list — the merged view keeps the file's
        // component alive, across a session boundary too.
        let root = scratch("ts-fallback");
        let broken = "<?php\nfunction Badge(***): Html\n{\n    return <b>{$label}</b>;\n}\n";
        let source = root.join("Badge.lintl.php");
        fs::write(&source, broken).expect("writes");
        let uri = super::path_to_uri(&source).expect("uri");

        let mut index = WorkspaceIndex::for_root(Some(&super::path_to_uri(&root).expect("uri")));
        let cache_body =
            fs::read_to_string(root.join(".lintl-cache/lsp-index.json")).expect("cache exists");
        assert!(
            cache_body.contains("\"lane\":\"scanner-fallback\""),
            "the fallback lane rides the cache row"
        );
        let (_text, declarations) = index.merged_file(&uri).expect("merged");
        assert_eq!(
            declarations.len(),
            1,
            "the scanner's declaration survives the fallback"
        );
        assert_eq!(declarations[0].name, "Badge");
        drop(index);

        let mut index = WorkspaceIndex::for_root(Some(&super::path_to_uri(&root).expect("uri")));
        let (_text, declarations) = index.merged_file(&uri).expect("cache-hit merged");
        assert_eq!(declarations[0].name, "Badge");
    }

    #[test]
    fn globs_match_segments_stars_and_double_stars() {
        use super::glob_matches;
        assert!(glob_matches(
            "packages/*/src/**/*.lintl.php",
            "packages/ui/src/Badge.lintl.php"
        ));
        assert!(glob_matches(
            "packages/*/src/**/*.lintl.php",
            "packages/app/src/deep/Page.lintl.php"
        ));
        assert!(glob_matches(
            "**/*.lintl.php",
            "anywhere/deep/Tree.lintl.php"
        ));
        assert!(
            !glob_matches(
                "packages/*/src/**/*.lintl.php",
                "packages/ui/lib/Badge.lintl.php"
            ),
            "a literal segment must match exactly"
        );
        assert!(
            !glob_matches(
                "packages/*/src/*.lintl.php",
                "packages/app/src/deep/Page.lintl.php"
            ),
            "a single `*` stays within one segment"
        );
        assert!(
            !glob_matches("packages/ui/**", "packages/app/src/Page.lintl.php"),
            "a literal prefix pins the subtree"
        );
        assert!(glob_matches("?.lintl.php", "a.lintl.php"));
        assert!(!glob_matches("?.lintl.php", "ab.lintl.php"));
    }

    #[test]
    fn the_monorepo_include_list_indexes_only_the_configured_patterns() {
        // The Phase 9 §f backlog: a monorepo with vendored component
        // sources needs an explicit include list. The fixture mirrors it:
        // real sources in packages/{ui,app}/src, vendored sources in
        // vendor/, and a stray root source none of the patterns name.
        let root = scratch("monorepo");
        fs::create_dir_all(root.join("packages/ui/src")).expect("dirs");
        fs::create_dir_all(root.join("packages/app/src/deep")).expect("dirs");
        fs::create_dir_all(root.join("vendor/acme")).expect("dirs");
        fs::write(root.join("packages/ui/src/Badge.lintl.php"), DECLARING).expect("writes");
        fs::write(root.join("packages/app/src/deep/Page.lintl.php"), DECLARING).expect("writes");
        fs::write(root.join("vendor/acme/Vendored.lintl.php"), DECLARING).expect("writes");
        fs::write(root.join("Stray.lintl.php"), DECLARING).expect("writes");
        fs::write(
            root.join("lintl.json"),
            r#"{"lsp": {"include": ["packages/*/src/**/*.lintl.php"]}}"#,
        )
        .expect("writes the config");

        let index = WorkspaceIndex::for_root(Some(&super::path_to_uri(&root).expect("uri")));
        let uris = index.uris();
        assert_eq!(
            uris.len(),
            2,
            "only the configured patterns walk in: {uris:?}"
        );
        let badge_uri =
            super::path_to_uri(&root.join("packages/ui/src/Badge.lintl.php")).expect("uri");
        assert!(
            uris.contains(&badge_uri),
            "the ui package's source is indexed: {uris:?}"
        );
        let page_uri =
            super::path_to_uri(&root.join("packages/app/src/deep/Page.lintl.php")).expect("uri");
        assert!(
            uris.contains(&page_uri),
            "the app package's nested source is indexed: {uris:?}"
        );
        assert!(
            !uris.iter().any(|uri| uri.contains("vendor")),
            "vendored sources stay out even though `**` would match them: {uris:?}"
        );
        assert!(
            !uris.iter().any(|uri| uri.contains("Stray")),
            "sources outside the patterns stay out: {uris:?}"
        );
    }

    #[test]
    fn a_missing_or_broken_include_config_keeps_the_default_walk() {
        // No lintl.json, an unparseable one, and an `lsp` object without
        // `include`: the default walk indexes every non-vendored source.
        let root = scratch("include-default");
        fs::write(root.join("Badge.lintl.php"), DECLARING).expect("writes");
        fs::write(root.join("lintl.json"), "{ not json").expect("writes broken config");
        let index = WorkspaceIndex::for_root(Some(&super::path_to_uri(&root).expect("uri")));
        assert_eq!(index.uris().len(), 1, "broken config: the default walk ran");

        let root = scratch("include-default-2");
        fs::write(root.join("Badge.lintl.php"), DECLARING).expect("writes");
        fs::write(root.join("lintl.json"), r#"{"lsp": {}}"#).expect("writes config");
        let index = WorkspaceIndex::for_root(Some(&super::path_to_uri(&root).expect("uri")));
        assert_eq!(
            index.uris().len(),
            1,
            "no include key: the default walk ran"
        );

        // A present-but-EMPTY include list indexes nothing: the config
        // says what to walk, and it says nothing.
        let root = scratch("include-empty");
        fs::write(root.join("Badge.lintl.php"), DECLARING).expect("writes");
        fs::write(root.join("lintl.json"), r#"{"lsp": {"include": []}}"#).expect("writes config");
        let index = WorkspaceIndex::for_root(Some(&super::path_to_uri(&root).expect("uri")));
        assert!(
            index.uris().is_empty(),
            "an empty include list is the literal configuration: {:?}",
            index.uris()
        );
    }
}
