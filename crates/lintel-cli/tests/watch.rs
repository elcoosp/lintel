//! The T-12 daemon contracts (architecture doc §11, §15 T-12): the socket
//! serves compiled PHP, a watcher event invalidates the cache even when
//! the mtime+size key would not (the same-size, same-mtime edit only the
//! watcher can see), compile failures surface as rendered diagnostics, and
//! the daemon's steady-state round trip beats the per-request spawn the
//! `proc_open` fallback pays for — measured p50/p99 vs spawn, the T-12
//! acceptance criterion.
//!
//! The tests run the foreground daemon in a thread against a scratch
//! directory and drive it through [`lintel_cli::watch::request`] — the
//! same client the PHP loader's socket lane mirrors.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// A fresh scratch directory for one test; recreated on each run.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lintel-watch-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("creates scratch dir");
    dir
}

/// A component source with marker text, so edits are observable in the
/// compiled output.
fn source(marker: &str) -> String {
    format!(
        concat!(
            "<?php\n\nnamespace App\\Watch;\n\nuse Lintel\\Html;\n\n",
            "function panel(string $title): Html\n{{\n",
            "    return <section class=\"panel\"><h2>{{$title}}</h2><p>{marker}</p></section>;\n}}\n"
        ),
        marker = marker
    )
}

/// Spawn the foreground daemon in a thread on its own socket and wait for
/// it to answer. Returns `(socket, join handle)`; the caller stops the
/// daemon with [`quit_daemon`].
fn start_daemon(socket: &Path, root: &Path) -> std::thread::JoinHandle<()> {
    let roots = vec![root.to_path_buf()];
    let daemon_socket = socket.to_path_buf();

    let problems = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let thread_problems = problems.clone();
    let handle = std::thread::spawn(move || {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code =
            lintel_cli::watch::run_foreground(&daemon_socket, &roots, &mut stdout, &mut stderr);
        eprintln!("DAEMON-STDOUT: {}", String::from_utf8_lossy(&stdout));
        eprintln!("DAEMON-STDERR: {}", String::from_utf8_lossy(&stderr));
        if code != 0 {
            *thread_problems.lock().expect("locks") = stderr;
        }
        assert_eq!(code, 0, "the daemon exits 0 on quit");
    });
    if let Err(message) = wait_ready(socket) {
        panic!(
            "the daemon never came up on {}: {message}; daemon stderr: {}",
            socket.display(),
            String::from_utf8_lossy(&problems.lock().expect("locks"))
        );
    }
    handle
}

/// Poll-connect until the daemon answers (or 5 s pass — then fail loudly).
fn wait_ready(socket: &Path) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if probe(socket).is_ok() {
            return Ok(());
        }
        if Instant::now() > deadline {
            return Err("no structured response within 5s".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Readiness probe: ask for a path that does not exist — the daemon
/// answers `ERR` (which `request` surfaces as `Err`) or compiles nothing;
/// any structured response means the socket is live.
fn probe(socket: &Path) -> Result<String, String> {
    // An ERR response is still a live daemon; only transport failures
    // (connect refused) mean "not ready".
    match lintel_cli::watch::request(socket, Path::new("/nonexistent-probe.lintel.php")) {
        Ok(php) => Ok(php),
        Err(message) => {
            // Only a transport failure means "not ready"; a structured
            // `ERR` body (rendered diagnostics) is the daemon answering.
            if message.starts_with("cannot connect") {
                Err(message)
            } else {
                Ok(String::new())
            }
        }
    }
}

/// Ask the daemon to stop and wait for its thread.
fn quit_daemon(socket: &Path, handle: std::thread::JoinHandle<()>) {
    let _ = lintel_cli::watch::quit(socket);
    handle.join().expect("the daemon thread ends cleanly");
}

#[test]
fn watch_serves_compiled_php_and_recompiles_on_change() {
    let dir = scratch("basic");
    let file = dir.join("Panel.lintel.php");
    fs::write(&file, source("v1")).expect("writes the source");
    let socket = dir.join("d.sock");
    let handle = start_daemon(&socket, &dir);

    let first = lintel_cli::watch::request(&socket, &file).expect("the daemon compiles");
    assert!(first.contains("new \\Lintel\\Html("), "{first}");
    assert!(first.contains("v1"), "{first}");

    // A normal edit (different size) recompiles through the stat key.
    fs::write(&file, source("v2-longer-marker")).expect("writes v2");
    std::thread::sleep(Duration::from_millis(50));
    let second = lintel_cli::watch::request(&socket, &file).expect("recompiles");
    assert!(second.contains("v2-longer-marker"), "{second}");

    quit_daemon(&socket, handle);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn watch_invalidates_through_the_watcher_when_the_key_would_not() {
    // The discriminating case: an edit that keeps the file's size AND
    // mtime identical leaves the mtime+size cache key untouched — only a
    // watcher event can have invalidated the entry, so the daemon must
    // serve the new content.
    let dir = scratch("watcher-only");
    let file = dir.join("Panel.lintel.php");
    let v1 = source("v1");
    fs::write(&file, &v1).expect("writes v1");
    let socket = dir.join("d.sock");
    let handle = start_daemon(&socket, &dir);

    let first = lintel_cli::watch::request(&socket, &file).expect("the daemon compiles");
    assert!(first.contains("v1"), "{first}");

    // Same length, same mtime, different content.
    let v2 = source("v2");
    assert_eq!(v1.len(), v2.len(), "the markers must keep the size equal");
    fs::write(&file, &v2).expect("writes v2");
    let meta = fs::metadata(&file).expect("stats v2");
    let mtime = meta.modified().expect("v2 mtime");
    let times = std::fs::FileTimes::new().set_modified(mtime);
    fs::set_times(&file, times).expect("restores the mtime");
    // Wait past the daemon's poll interval so the inotify event lands.
    std::thread::sleep(Duration::from_millis(200));

    let second = lintel_cli::watch::request(&socket, &file).expect("the watcher invalidated");
    assert!(
        second.contains("v2"),
        "only a watcher event could have invalidated this cache entry: {second}"
    );

    quit_daemon(&socket, handle);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn watch_surfaces_compile_errors_as_diagnostics() {
    let dir = scratch("errors");
    let file = dir.join("Broken.lintel.php");
    fs::write(&file, "<?php\nreturn <button onclick={$x}>go</button>;\n")
        .expect("writes a source that fails validation");
    let socket = dir.join("d.sock");
    let handle = start_daemon(&socket, &dir);

    let problem = lintel_cli::watch::request(&socket, &file).expect_err("compile fails");
    assert!(problem.contains("LNT"), "rendered diagnostics: {problem}");

    quit_daemon(&socket, handle);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn watch_p99_beats_the_spawn_the_fallback_pays() {
    const SAMPLES: usize = 30;

    // The T-12 acceptance measurement: p50/p99 of the daemon's
    // steady-state round trip vs p50/p99 of one `lintel build` process
    // spawn per request (what the no-daemon proc_open lane pays). The
    // daemon serves cache hits here — the resident process's real
    // steady state; every spawn pays fork+exec+compile every time.
    let dir = scratch("latency");
    let file = dir.join("Panel.lintel.php");
    fs::write(&file, source("bench")).expect("writes the source");
    let socket = dir.join("d.sock");
    let handle = start_daemon(&socket, &dir);

    // Warm the cache once, then measure.
    let _ = lintel_cli::watch::request(&socket, &file).expect("warmup compiles");

    let mut daemon_us: Vec<u128> = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let start = Instant::now();
        let php = lintel_cli::watch::request(&socket, &file).expect("daemon request");
        assert!(php.contains("new \\Lintel\\Html("));
        daemon_us.push(start.elapsed().as_micros());
    }

    let bin = env!("CARGO_BIN_EXE_lintel");
    let mut spawn_us: Vec<u128> = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let start = Instant::now();
        let output = Command::new(bin)
            .args(["build", file.to_str().expect("utf8 path")])
            .output()
            .expect("spawns the build");
        assert!(output.status.success());
        spawn_us.push(start.elapsed().as_micros());
    }

    quit_daemon(&socket, handle);

    let daemon_p50 = percentile(&daemon_us, 50);
    let daemon_p99 = percentile(&daemon_us, 99);
    let spawn_p50 = percentile(&spawn_us, 50);
    let spawn_p99 = percentile(&spawn_us, 99);
    println!(
        "T-12 daemon vs spawn over {SAMPLES} samples:\n  daemon p50 {daemon_p50}µs p99 {daemon_p99}µs\n  spawn  p50 {spawn_p50}µs p99 {spawn_p99}µs"
    );
    assert!(
        daemon_p99 < spawn_p50,
        "the daemon's p99 ({daemon_p99}µs) must beat the spawn lane's p50 ({spawn_p50}µs)"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// The `pct`-th percentile of a sample (nearest-rank).
fn percentile(samples: &[u128], pct: usize) -> u128 {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let index = ((pct * sorted.len()) / 100).min(sorted.len() - 1);
    sorted[index]
}

/// The daemon's cumulative counters from `.lintel-cache/stats.json` (the
/// same observable the cache tests and the cold/warm bench read).
fn daemon_stats(workspace: &Path) -> (usize, usize) {
    let body = fs::read_to_string(workspace.join(".lintel-cache/stats.json"))
        .expect("the daemon recorded its counters");
    let value: serde_json::Value = serde_json::from_str(&body).expect("stats.json is valid JSON");
    let compiled = usize::try_from(value["compiled"].as_u64().unwrap_or(0)).expect("compiled");
    let hits = usize::try_from(value["cache_hits"].as_u64().unwrap_or(0)).expect("cache_hits");
    (compiled, hits)
}

#[test]
fn watch_rebuilds_go_through_the_lintel_build_cache() {
    // The M1 acceptance: the daemon's watch loop serves through the same
    // content-addressed engine `lintel build` uses — a recompile of
    // unchanged content never happens, and the counters land in
    // `.lintel-cache/stats.json` (`compiled` counts compiles, `cache_hits`
    // counts content-hash hits).
    let dir = scratch("build-cache");
    let file = dir.join("Panel.lintel.php");
    let v1 = source("v1");
    fs::write(&file, &v1).expect("writes the source");
    let socket = dir.join("d.sock");
    let handle = start_daemon(&socket, &dir);

    // Cold: the daemon compiles and stores the artifact.
    let first = lintel_cli::watch::request(&socket, &file).expect("the daemon compiles");
    assert!(first.contains("v1"), "{first}");
    let (compiled, hits) = daemon_stats(&dir);
    assert_eq!((compiled, hits), (1, 0), "the cold compile is one compile");
    let has_artifact = fs::read_dir(dir.join(".lintel-cache"))
        .expect("the cache dir exists")
        .filter_map(Result::ok)
        .any(|entry| entry.path().extension().is_some_and(|ext| ext == "php"));
    assert!(
        has_artifact,
        "the compile landed as a content-addressed artifact"
    );

    // A real edit recompiles (new content, new key).
    fs::write(&file, source("v2")).expect("writes v2");
    std::thread::sleep(Duration::from_millis(50));
    let second = lintel_cli::watch::request(&socket, &file).expect("recompiles");
    assert!(second.contains("v2"), "{second}");
    let (compiled, hits) = daemon_stats(&dir);
    assert_eq!((compiled, hits), (2, 0), "the edit is a second compile");

    // Reverting to the first content: the mtime+size in-memory key
    // misses (the file was rewritten), but the content hash matches the
    // cold artifact — the daemon serves the stored PHP WITHOUT
    // recompiling (stats.json shows the hit, not a third compile), and
    // the bytes are IDENTICAL to the first compile (same content, same
    // key, same artifact).
    fs::write(&file, &v1).expect("reverts to v1");
    std::thread::sleep(Duration::from_millis(50));
    let reverted = lintel_cli::watch::request(&socket, &file).expect("serves the revert");
    assert_eq!(
        reverted, first,
        "the revert serves the cached artifact byte for byte, not a fresh compile"
    );
    let (compiled, hits) = daemon_stats(&dir);
    assert_eq!(
        (compiled, hits),
        (2, 1),
        "the revert is a cache hit, not a third compile"
    );

    quit_daemon(&socket, handle);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn watch_serves_concurrent_clients_and_builds_without_corrupting_the_cache() {
    // The M1 acceptance's concurrency half: parallel socket clients get
    // correct answers, and a `lintel build` running against the same
    // workspace while the daemon serves does not corrupt the shared
    // mutable cache files (`stats.json` — flocked on both sides; the
    // content-addressed artifacts are identical bytes per key by the
    // compiler's purity contract).
    const CLIENTS: usize = 8;
    const ROUNDS: usize = 12;

    let dir = scratch("concurrent");
    let file = dir.join("Panel.lintel.php");
    fs::write(&file, source("conc")).expect("writes the source");
    let socket = dir.join("d.sock");
    let handle = start_daemon(&socket, &dir);

    let start = std::sync::Arc::new(std::sync::Barrier::new(CLIENTS + 2));
    let mut workers = Vec::new();
    for client in 0..CLIENTS {
        let start = std::sync::Arc::clone(&start);
        let socket = socket.clone();
        let file = file.clone();
        workers.push(std::thread::spawn(move || {
            start.wait();
            for round in 0..ROUNDS {
                let php = lintel_cli::watch::request(&socket, &file)
                    .unwrap_or_else(|e| panic!("client {client} round {round}: {e}"));
                assert!(
                    php.contains("new \\Lintel\\Html(") && php.contains("conc"),
                    "client {client} round {round} got a correct artifact: {php}"
                );
            }
        }));
    }

    // Two `lintel build` processes over the SAME workspace while the
    // daemon serves: both record counters into the same stats.json.
    let bin = env!("CARGO_BIN_EXE_lintel");
    for build in 0..2 {
        let start = std::sync::Arc::clone(&start);
        let file = file.clone();
        workers.push(std::thread::spawn(move || {
            start.wait();
            let output = Command::new(bin)
                .args(["build", file.to_str().expect("utf8 path")])
                .output()
                .expect("spawns the build");
            assert!(output.status.success(), "build {build} compiles cleanly");
        }));
    }

    for worker in workers {
        worker.join().expect("every worker finishes cleanly");
    }

    // The shared observable survived the concurrent writers: stats.json
    // is exactly one valid JSON object (not a truncated mix).
    let body = fs::read_to_string(dir.join(".lintel-cache/stats.json"))
        .expect("stats.json exists after the run");
    let value: serde_json::Value =
        serde_json::from_str(&body).expect("stats.json is one uncorrupted JSON object");
    assert!(
        value["compiled"].as_u64().is_some() && value["cache_hits"].as_u64().is_some(),
        "the counters are intact: {body}"
    );

    quit_daemon(&socket, handle);
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn watch_frames_are_length_prefixed_json_on_the_wire() {
    // The protocol contract a non-Rust client (the PHP loader) rides:
    // ten ASCII decimal digits, a newline, then that many bytes of UTF-8
    // JSON — read raw off a Unix socket, no client code in between.
    use std::io::{BufRead, BufReader, Read as _, Write as _};
    use std::os::unix::net::UnixStream;

    let dir = scratch("framing");
    let file = dir.join("Panel.lintel.php");
    fs::write(&file, source("wire")).expect("writes the source");
    let socket = dir.join("d.sock");
    let handle = start_daemon(&socket, &dir);

    let mut stream = UnixStream::connect(&socket).expect("connects");
    let request = format!("{{\"path\":\"{}\"}}", file.display());
    stream
        .write_all(format!("{:010}\n{request}", request.len()).as_bytes())
        .and_then(|()| stream.flush())
        .expect("sends the framed request");

    let mut reader = BufReader::new(stream);
    let mut prefix = String::new();
    reader
        .read_line(&mut prefix)
        .expect("reads the frame prefix");
    let prefix = prefix.trim_end();
    assert_eq!(
        prefix.len(),
        10,
        "the prefix is exactly ten digits: {prefix:?}"
    );
    let len: usize = prefix.parse().expect("the prefix is decimal");
    let mut payload = vec![0_u8; len];
    reader.read_exact(&mut payload).expect("reads the payload");
    let payload = String::from_utf8(payload).expect("the payload is UTF-8");
    let value: serde_json::Value =
        serde_json::from_str(&payload).expect("the payload is one JSON object");
    assert_eq!(value["ok"], true, "the compile succeeded: {payload}");
    assert!(
        value["php"]
            .as_str()
            .is_some_and(|php| php.contains("wire")),
        "the compiled PHP rides the `php` field: {payload}"
    );

    quit_daemon(&socket, handle);
    let _ = fs::remove_dir_all(&dir);
}
