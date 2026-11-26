//! The Phase 11 M2 acceptance: the `RoadRunner` progressive per-boundary
//! timing invariant, driven as a Rust integration test over the bench
//! lane's CLI entry (`bench/lib/progressive_invariant.php`).
//!
//! The test runs the benchmark (a three-boundary `<Suspense>` page
//! rendered through the real `RoadRunnerFlushTransport` with deterministic
//! per-boundary latencies) and asserts the progressive invariant the lane
//! itself enforces: **the placeholder for boundary N is flushed before the
//! resolved content for boundary N-1**, resolution stays in order for the
//! deterministic latencies, the `RoadRunner` wire order equals the flush
//! order, and the three timing facts exist and order correctly
//! (first-placeholder ≤ first-resolved ≤ total).
//!
//! The PHP binary comes from `PHP_BIN` (default `php` on `PATH`); when no
//! PHP is available the test prints a skip note naming the CI step that
//! runs it (the `php` job's rust-PHP-gates step sets `PHP_BIN=php`, and
//! the `bench` job runs the lane through `make bench`).

use std::path::PathBuf;
use std::process::Command;

/// Resolve the PHP binary: `PHP_BIN` overrides the default `php` on PATH.
fn php_bin() -> Option<PathBuf> {
    let name = std::env::var("PHP_BIN").unwrap_or_else(|_| "php".into());
    if name.contains('/') {
        let path = PathBuf::from(&name);
        return path.is_file().then_some(path);
    }
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|dir| dir.join(&name))
        .find(|candidate| candidate.is_file())
}

/// The repo root (the test lives at crates/lintl-cli/tests/).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .expect("the repo root")
}

use std::path::Path;

#[test]
fn the_rr_progressive_timing_invariant_holds_over_the_bench_lane() {
    let Some(php) = php_bin() else {
        eprintln!(
            "SKIP: the RR progressive-timing invariant test needs a PHP binary \
             (set PHP_BIN; CI: the php job's rust-PHP-gates step and the bench \
             job's `make bench` lane both run it)"
        );
        return;
    };

    // The lane requires the bench vendor tree (`bench/vendor/autoload.php`).
    // It is installed by the php job's `composer install --working-dir=bench`
    // and by the bench image build — NOT by the plain `rust` job, which only
    // has the toolchain. When the vendor tree is absent there is nothing to
    // drive, so skip (naming the steps that do install it) instead of letting
    // the PHP `require` fatal and red the unrelated rust job.
    let entry = repo_root().join("bench/lib/progressive_invariant.php");
    if !repo_root().join("bench/vendor/autoload.php").is_file() {
        eprintln!(
            "SKIP: the RR progressive-timing invariant test needs the bench \
             vendor tree (bench/vendor/autoload.php); CI installs it in the php \
             job (`composer install --working-dir=bench`) and the bench image \
             build, both of which run this lane"
        );
        return;
    }

    let output = Command::new(&php)
        .arg(&entry)
        .current_dir(repo_root())
        .output()
        .expect("runs the progressive invariant lane");
    assert!(
        output.status.success(),
        "the progressive invariant lane exits 0 (stderr: {})",
        String::from_utf8_lossy(&output.stderr)
    );

    let facts: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("the lane prints one JSON facts object");
    let invariants = facts
        .get("invariants")
        .and_then(serde_json::Value::as_object)
        .expect("the invariants object");
    for (name, value) in invariants {
        assert_eq!(
            value,
            &serde_json::json!(true),
            "the {name} invariant must hold"
        );
    }

    // The progressive rows: placeholder N before resolved N-1, per pair.
    let progressive = facts
        .get("progressive")
        .and_then(serde_json::Value::as_array)
        .expect("the per-pair progressive rows");
    assert_eq!(
        progressive.len(),
        2,
        "three boundaries — two (placeholder, resolved) pairs assert"
    );
    for row in progressive {
        assert_eq!(
            row.get("ok"),
            Some(&serde_json::json!(true)),
            "placeholder N flushed before resolved N-1: {row}"
        );
    }

    // The three timing facts exist, are positive, and order correctly.
    let ttfb_placeholder = facts
        .get("ttfb_placeholder_ns")
        .and_then(serde_json::Value::as_u64)
        .expect("the first-placeholder timing");
    let ttfb_resolved = facts
        .get("ttfb_resolved_ns")
        .and_then(serde_json::Value::as_u64)
        .expect("the first-resolved timing");
    let total = facts
        .get("total_ns")
        .and_then(serde_json::Value::as_u64)
        .expect("the total timing");
    assert!(ttfb_placeholder > 0, "the placeholder flushed after t0");
    assert!(
        ttfb_placeholder <= ttfb_resolved,
        "the first placeholder flushes before the first resolved chunk \
         ({ttfb_placeholder} ≤ {ttfb_resolved} ns)"
    );
    assert!(
        ttfb_resolved <= total,
        "the first resolved chunk flushes before the page completes \
         ({ttfb_resolved} ≤ {total} ns)"
    );

    // The wire shape: three boundaries' placeholder + resolved pairs rode
    // the RR lane, one frame per chunk.
    let chunks = facts
        .get("chunks")
        .and_then(serde_json::Value::as_u64)
        .expect("the chunk count");
    assert!(
        chunks >= 6,
        "three boundaries flush at least placeholder+resolved each: {chunks} chunks"
    );
}
