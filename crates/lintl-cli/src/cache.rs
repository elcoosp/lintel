//! The M2 incremental build cache (architecture doc §9.3's performance
//! target "warm (cache hit): I/O-bound", and §17.1-Q2's "incremental
//! cache"): `lintl build` hashes every source (SHA-256 over the CONTENT —
//! mtime lies after a git checkout), keyed with the compile target, and
//! reuses the cached PHP artifact when the key is known, so a warm build
//! compiles nothing and only writes output.
//!
//! # Cache layout
//!
//! `<workspace>/.lintl-cache/` — the workspace root is the common
//! ancestor of the build inputs and `--out` (the same root the bare
//! `--manifest` default anchors to), so the cache lands with the sources
//! it describes from any invocation directory. One entry per source
//! content + target pair: `<hash>.php` (the compiled artifact) plus a
//! metadata sidecar `<hash>.json` (source path, target, options hash, and
//! the cold build's rendered warnings, which a warm build re-emits
//! verbatim so warm stderr matches cold). Two sources with identical
//! content share one entry — the compiled output is identical by the
//! compiler's purity contract (§5.2: `compile` is a pure function of the
//! source and options).
//!
//! # Bypass and lanes
//!
//! `lintl build --no-cache` skips lookup AND store (CI or debugging);
//! `check`/`analyze` never read the cache — they must always re-validate.
//! Cache I/O is best-effort: an unreadable or unwritable cache degrades to
//! recompiling everything, never to a failed build (the compiled output is
//! already in hand when the cache write happens).
//!
//! The `.lintl-cache/stats.json` sidecar records the last build's
//! counters (`compiled`, `cache_hits`, `bypassed`) — the observable the
//! integration tests and the cold/warm bench read.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use lintl_core::Target;

/// The cache directory name under the workspace root.
pub(crate) const CACHE_DIR: &str = ".lintl-cache";

/// The shared mutable cache files (the `stats.json` counters) are written
/// under an exclusive `flock(2)` (fs2: `flock` on Unix, `LockFileEx` on
/// Windows): a watch daemon and a concurrent `lintl build` both record
/// counters into the same `<workspace>/.lintl-cache/stats.json`, and an
/// interleaved `fs::write` would leave one reader a truncated mix. The
/// content-addressed artifacts (`<hash>.php`/`<hash>.json`) do NOT need
/// the lock — same key ⇒ same compiled bytes by the compiler's purity
/// contract (§5.2), so a racing write lands the identical content; the
/// lock is only for files whose CONTENT differs per writer.
///
/// Best-effort like every cache write: any failure degrades to the
/// unlocked/unwritten behavior, never to a failed build.
pub(crate) fn lock_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
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

/// The compile options hash: SHA-256 over the canonical option string.
/// The build path's output depends only on the target today; the hash
/// covers the whole option set so a future compile-affecting flag
/// invalidates entries by construction.
fn options_hash(target_name: &str) -> String {
    hex(&Sha256::digest(target_name.as_bytes()))
}

/// The canonical target name the key and sidecar carry.
fn canonical_target(target: Target) -> &'static str {
    match target {
        Target::String => "string",
        Target::Stream => "stream",
        Target::Rfc => "rfc",
    }
}

/// The cache: the `.lintl-cache` directory under one workspace root.
pub(crate) struct BuildCache {
    dir: PathBuf,
}

/// One cache hit: the compiled PHP and the cold build's rendered warnings
/// in both sink shapes — the text render and the JSON lines — so a warm
/// build's stderr matches the cold one byte for byte in either
/// `--format`.
pub(crate) struct CacheHit {
    pub(crate) php: String,
    pub(crate) warnings_text: Vec<String>,
    pub(crate) warnings_json: Vec<String>,
}

/// The last build's counters, written to `.lintl-cache/stats.json` — the
/// observable the cache tests and the cold/warm bench read.
pub(crate) struct CacheStats {
    pub(crate) compiled: usize,
    pub(crate) cache_hits: usize,
    pub(crate) bypassed: bool,
}

impl BuildCache {
    /// The cache rooted at `workspace` (its `.lintl-cache` directory).
    pub(crate) fn at(workspace: &Path) -> Self {
        Self {
            dir: workspace.join(CACHE_DIR),
        }
    }

    /// The cache key for one source: a SHA-256 content hash keyed with the
    /// compile target — string/stream/rfc artifacts differ for the same
    /// source, so the target rides the key.
    pub(crate) fn key(source: &str, target: Target) -> String {
        let target_name = canonical_target(target);
        let mut hasher = Sha256::new();
        hasher.update(source.as_bytes());
        hasher.update([0_u8]);
        hasher.update(target_name.as_bytes());
        hex(&hasher.finalize())
    }

    /// Look one key up: the artifact bytes plus the sidecar's target and
    /// options must all agree with the request, else the entry is a miss
    /// (a mismatching sidecar is treated like a missing entry — the
    /// paranoid half of the target-aware key).
    pub(crate) fn lookup(&self, key: &str, target: Target) -> Option<CacheHit> {
        let php = fs::read_to_string(self.dir.join(format!("{key}.php"))).ok()?;
        let sidecar = fs::read_to_string(self.dir.join(format!("{key}.json"))).ok()?;
        let value: Value = serde_json::from_str(&sidecar).ok()?;
        let target_name = canonical_target(target);
        if value.get("target")?.as_str()? != target_name {
            return None;
        }
        if value.get("options")?.as_str()? != options_hash(target_name) {
            return None;
        }
        let warnings_text = value
            .get("warnings_text")?
            .as_array()?
            .iter()
            .map(|warning| warning.as_str().unwrap_or_default().to_string())
            .collect();
        let warnings_json = value
            .get("warnings_json")?
            .as_array()?
            .iter()
            .map(|warning| warning.as_str().unwrap_or_default().to_string())
            .collect();
        Some(CacheHit {
            php,
            warnings_text,
            warnings_json,
        })
    }

    /// Store one compiled artifact under its key, best-effort: a failure
    /// degrades the next build to a recompile, never fails this one.
    pub(crate) fn store(
        &self,
        key: &str,
        source: &Path,
        target: Target,
        php: &str,
        warnings_text: &[String],
        warnings_json: &[String],
    ) {
        if fs::create_dir_all(&self.dir).is_err() {
            return;
        }
        let target_name = canonical_target(target);
        let sidecar = json!({
            "source": source.display().to_string(),
            "target": target_name,
            "options": options_hash(target_name),
            "warnings_text": warnings_text,
            "warnings_json": warnings_json,
        });
        let _ = fs::write(self.dir.join(format!("{key}.php")), php);
        let _ = fs::write(self.dir.join(format!("{key}.json")), sidecar.to_string());
    }

    /// Record the build's counters to `.lintl-cache/stats.json` under an
    /// exclusive flock (a watch daemon and a concurrent build share this
    /// file), best-effort (the counters are observability, not contract).
    pub(crate) fn record(&self, stats: &CacheStats) {
        if fs::create_dir_all(&self.dir).is_err() {
            return;
        }
        let body = json!({
            "compiled": stats.compiled,
            "cache_hits": stats.cache_hits,
            "bypassed": stats.bypassed,
        });
        let _ = lock_write(&self.dir.join("stats.json"), format!("{body}\n").as_bytes());
    }
}

/// The lowercase hex form of a SHA-256 digest.
fn hex(digest: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{BuildCache, CacheStats};
    use lintl_core::Target;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn cache(name: &str) -> (PathBuf, BuildCache) {
        let workspace =
            std::env::temp_dir().join(format!("lintl-cache-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&workspace);
        fs::create_dir_all(&workspace).expect("creates workspace");
        (workspace.clone(), BuildCache::at(&workspace))
    }

    const SOURCE: &str =
        "<?php\nfunction Badge(string $label): Html\n{\n    return <b>{$label}</b>;\n}\n";

    #[test]
    fn the_key_is_a_content_hash_keyed_with_the_target() {
        let first = BuildCache::key(SOURCE, Target::String);
        let second = BuildCache::key(SOURCE, Target::String);
        assert_eq!(first, second, "the same content + target is one key");
        assert_eq!(first.len(), 64, "a SHA-256 hex digest: {first}");
        let stream = BuildCache::key(SOURCE, Target::Stream);
        assert_ne!(first, stream, "a different target is a different key");
        let changed = BuildCache::key(&SOURCE.replace("Badge", "Pill"), Target::String);
        assert_ne!(first, changed, "changed content is a different key");
    }

    #[test]
    fn store_then_lookup_round_trips_with_warnings() {
        let (workspace, cache) = cache("round-trip");
        let key = BuildCache::key(SOURCE, Target::String);
        let source = PathBuf::from("/ws/src/Badge.lintl.php");
        let text = vec!["a rendered warning\n".to_string()];
        let json = vec!["{\"code\":\"LNL0104\"}".to_string()];
        cache.store(&key, &source, Target::String, "<?php\n", &text, &json);
        let hit = cache.lookup(&key, Target::String).expect("hit");
        assert_eq!(hit.php, "<?php\n");
        assert_eq!(hit.warnings_text, text, "warm text stderr matches cold");
        assert_eq!(hit.warnings_json, json, "warm json stderr matches cold");
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn a_mismatching_sidecar_is_a_miss() {
        let (workspace, cache) = cache("mismatch");
        let key = BuildCache::key(SOURCE, Target::String);
        cache.store(
            &key,
            Path::new("/ws/src/Badge.lintl.php"),
            Target::String,
            "<?php\n",
            &[],
            &[],
        );
        // The stream target's key differs by construction — a stream
        // lookup on a string entry is a miss even before the sidecar
        // check.
        assert!(cache
            .lookup(&BuildCache::key(SOURCE, Target::Stream), Target::Stream)
            .is_none());
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn stats_record_the_counters() {
        let (workspace, cache) = cache("stats");
        cache.record(&CacheStats {
            compiled: 2,
            cache_hits: 5,
            bypassed: false,
        });
        let body = fs::read_to_string(workspace.join(".lintl-cache/stats.json")).expect("stats");
        let value: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
        assert_eq!(value["compiled"], 2);
        assert_eq!(value["cache_hits"], 5);
        assert_eq!(value["bypassed"], false);
        let _ = fs::remove_dir_all(workspace);
    }
}
