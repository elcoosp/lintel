//! The T-12 daemon (architecture doc §6.7 sketch, §15 T-12; hardened in
//! the Phase 10 card, `docs/daemon-protocol.md`): `lintel watch` keeps one
//! resident compiler process behind a socket, so the dev loader's
//! per-request compile skips the process spawn.
//!
//! # The wire
//!
//! One request per connection, both directions framed the same way:
//! ten ASCII decimal digits, a newline, then exactly that many bytes of
//! UTF-8 JSON (the §6.7 sketch `{"path":…} → {"php":…}|{"diag":[…]}`, made
//! real). A compile failure is an *answer* (`{"ok":false,"diag":[…]}`,
//! one rendered diagnostic per entry), not a transport failure; a
//! malformed frame closes the connection and the daemon serves on. The
//! client retries a connect failure with a short backoff — the
//! reconnection window a restarting daemon needs — and the PHP loader
//! falls back to its `proc_open` lane on the final failure.
//!
//! # The cache
//!
//! The daemon serves from an in-memory mtime+size map (a hit re-verifies
//! the stat key; a missed event self-heals), and every miss consults the
//! workspace's content-addressed build cache — the same
//! `<workspace>/.lintel-cache/` engine `lintel build` uses, keyed by a
//! SHA-256 of the source CONTENT with the target — so a watch-triggered
//! rebuild of unchanged content never recompiles and a reverted edit
//! serves instantly. The daemon's counters accumulate in
//! `.lintel-cache/stats.json` (flocked, like every shared mutable cache
//! file) after each disk-cache consult. Watcher events under
//! `.lintel-cache/` never invalidate entries: the cache's own writes must
//! not churn the in-memory map.
//!
//! The design is deliberately **two-threaded, not three**: the daemon's
//! own loop is a plain *blocking* `accept` — a compile request is answered
//! the moment it connects, with no poll latency (the p50 the T-12
//! acceptance measures) — and cache invalidation runs on `notify`'s own
//! event thread through a shared cache, so the daemon spawns no threads of
//! its own.
//!
//! Unix runs a `UnixListener`; Windows falls back to a loopback TCP socket
//! (the same protocol, with the chosen ephemeral port written to
//! `<socket-path>.port`). The Windows lane is verified by CI: the
//! `windows-daemon` job runs the whole watch suite over the TCP fallback
//! on a `windows-latest` runner (the Phase 11 M3 card closed the carried
//! DEFERRED-TO-CI/MAINTAINER note — this sandbox has no Windows runner).

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use lintel_core::{compile, Options, Target};
use serde_json::{json, Value};

use crate::cache::{self, BuildCache};

/// How long one client waits for the daemon's answer before giving up (a
/// stuck compile must not wedge the loader).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// The frame prefix: ten ASCII decimal digits, then a newline, then the
/// payload. Fixed width so the reader can `read_exact` the prefix and
/// then the body without any delimiter inside the payload.
const FRAME_PREFIX_WIDTH: usize = 10;

/// The payload cap both sides accept: a larger or non-decimal prefix is a
/// malformed frame (the connection closes; the client treats it as a
/// transport failure).
const MAX_PAYLOAD: usize = 64 * 1024 * 1024;

/// The client's connect retries and backoff — the window a restarting
/// daemon needs to rebind its socket; once connected, a protocol failure
/// is final for that request.
const CONNECT_ATTEMPTS: usize = 3;
const CONNECT_BACKOFF_MS: [u64; 3] = [0, 50, 250];

/// The default socket path for a background `lintel watch` in the current
/// directory: one socket per working directory, hash-namespaced so parallel
/// projects never collide.
#[must_use]
pub fn default_socket() -> PathBuf {
    let cwd = fs::canonicalize(".").unwrap_or_else(|_| PathBuf::from("."));
    let mut hasher = DefaultHasher::new();
    cwd.hash(&mut hasher);
    std::env::temp_dir().join(format!("lintel-watch-{:016x}.sock", hasher.finish()))
}

/// Encode one framed message: a ten-digit zero-padded byte length, a
/// newline, then the payload bytes. A payload over [`MAX_PAYLOAD`] is a
/// protocol error (both sides refuse to send it).
fn encode_frame(payload: &str) -> Result<Vec<u8>, String> {
    let bytes = payload.as_bytes();
    if bytes.len() > MAX_PAYLOAD {
        return Err(format!(
            "payload of {} bytes exceeds the {MAX_PAYLOAD}-byte frame cap",
            bytes.len()
        ));
    }
    let mut out = Vec::with_capacity(bytes.len() + FRAME_PREFIX_WIDTH + 1);
    let _ = write!(out, "{:0width$}", bytes.len(), width = FRAME_PREFIX_WIDTH);
    out.push(b'\n');
    out.extend_from_slice(bytes);
    Ok(out)
}

/// Read one framed message off `reader`: the ten-digit prefix, the
/// separator newline, then the payload. Any deviation — a short or
/// non-decimal prefix, a missing separator, a length over the cap, a
/// truncated body — is a protocol error string.
fn read_frame(reader: &mut impl BufRead) -> Result<String, String> {
    let mut prefix = [0_u8; FRAME_PREFIX_WIDTH];
    reader
        .read_exact(&mut prefix)
        .map_err(|e| format!("cannot read the frame prefix: {e}"))?;
    let prefix = std::str::from_utf8(&prefix)
        .map_err(|_| "the frame prefix is not ASCII digits".to_string())?;
    let len: usize = prefix
        .parse()
        .map_err(|_| format!("malformed frame prefix: {prefix:?}"))?;
    if len > MAX_PAYLOAD {
        return Err(format!(
            "frame length {len} exceeds the {MAX_PAYLOAD}-byte cap"
        ));
    }
    let mut separator = [0_u8; 1];
    reader
        .read_exact(&mut separator)
        .map_err(|e| format!("cannot read the frame separator: {e}"))?;
    if separator[0] != b'\n' {
        return Err("malformed frame: the prefix is not newline-terminated".to_string());
    }
    let mut payload = vec![0_u8; len];
    reader
        .read_exact(&mut payload)
        .map_err(|e| format!("cannot read the frame payload: {e}"))?;
    String::from_utf8(payload).map_err(|e| format!("the frame payload is not UTF-8: {e}"))
}

/// The request JSON for one compile: `{"path": "<abs path>"}`.
fn encode_request(path: &Path) -> String {
    json!({ "path": path.display().to_string() }).to_string()
}

/// One compiled file: the stat key it was compiled under and the result
/// (compiled PHP, or one rendered diagnostic string per problem).
struct CacheEntry {
    // The native mtime, NOT seconds-truncated: two edits inside the same
    // wall-clock second (a same-size rewrite — the cache-invalidation tests'
    // discriminating case, and a real editor's fast save) must produce
    // different keys, or the daemon serves the stale first artifact. The
    // nanosecond-resolution `SystemTime` macOS/Linux/Windows all report
    // keeps rapid rewrites distinct; the watcher event is the fast path,
    // the stat key the self-heal.
    mtime: SystemTime,
    size: u64,
    result: Result<String, Vec<String>>,
}

/// The daemon's cumulative disk-cache counters (the same shapes
/// `.lintel-cache/stats.json` records for `lintel build`).
#[derive(Default)]
struct CacheCounters {
    compiled: usize,
    cache_hits: usize,
}

/// The compile cache shared between the daemon's serving loop and the
/// watcher's invalidation thread (the one `notify` runs): requests read and
/// insert, watcher events drop.
type SharedCache = Arc<Mutex<HashMap<PathBuf, CacheEntry>>>;

/// Run the daemon in the foreground: bind the socket, watch the paths,
/// serve requests until a client sends `quit`. Returns the process exit
/// code (0 unless the daemon could not start).
pub fn run_foreground(
    socket: &Path,
    paths: &[PathBuf],
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let bound = Daemon::bind(socket, paths);
    let daemon = match bound {
        Ok(daemon) => daemon,
        Err(problem) => {
            let _ = writeln!(stderr, "lintel watch: {problem}");
            return 1;
        }
    };
    let roots = daemon.roots.len();
    let socket_display = daemon.socket.display();
    let _ = writeln!(
        stdout,
        "lintel watch: daemon listening on {socket_display} ({roots} watch root(s)); \
         a client's quit frame stops it"
    );
    let mut daemon = daemon;
    daemon.serve()
}

/// Spawn the daemon as a detached background process and wait for its
/// socket to answer, then print the socket path. `foreground_args` are the
/// watch arguments minus the `--foreground` flag. Returns the process exit
/// code (0 ready, 1 the daemon never came up).
pub fn spawn_background(
    socket: &Path,
    foreground_args: &[String],
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(e) => {
            let _ = writeln!(stderr, "lintel watch: cannot locate the binary: {e}");
            return 1;
        }
    };
    let mut command = std::process::Command::new(exe);
    command
        .args(foreground_args)
        .arg("--foreground")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    detach(&mut command);
    let Ok(mut child) = command.spawn() else {
        let _ = writeln!(stderr, "lintel watch: could not spawn the daemon process");
        return 1;
    };
    // Readiness: poll-connect until the socket accepts. The probe opens
    // the connection and closes it WITHOUT sending anything — the daemon
    // treats EOF as a probe and serves on (a quit frame here would stop
    // the daemon the moment it came up).
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(stream) = connect(socket) {
            drop(stream);
            let _ = writeln!(stdout, "lintel watch: daemon ready on {}", socket.display());
            return 0;
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            let _ = writeln!(
                stderr,
                "lintel watch: the daemon never came up on {} within 5s",
                socket.display()
            );
            return 1;
        }
        if let Ok(Some(status)) = child.try_wait() {
            let _ = writeln!(
                stderr,
                "lintel watch: the daemon exited immediately ({status})"
            );
            return 1;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Detach the child from the terminal's process group so closing the shell
/// does not take the daemon down (Unix); a no-op elsewhere.
fn detach(command: &mut std::process::Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        // CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        use std::os::windows::process::CommandExt as _;
        command.creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS);
    }
}

/// The daemon: its socket listener, its watcher, its in-memory cache, the
/// workspace's content-addressed build cache and the cumulative counters
/// the `.lintel-cache/stats.json` observability file records. The serving
/// loop and the watcher's invalidation thread (notify's own) are the only
/// execution contexts; the daemon spawns no threads itself.
struct Daemon {
    listener: Listener,
    watcher: Box<dyn WatcherDrop>,
    socket: PathBuf,
    roots: Vec<PathBuf>,
    cache: SharedCache,
    build_cache: BuildCache,
    counters: Arc<Mutex<CacheCounters>>,
    quit: bool,
}

/// `notify`'s watcher type hidden behind a local trait so `Daemon` can own
/// it boxed without naming the platform's `RecommendedWatcher` here.
trait WatcherDrop: notify::Watcher {}

impl<W: notify::Watcher> WatcherDrop for W {}

/// The listening socket, Unix or (on Windows) loopback TCP.
enum Listener {
    #[cfg(unix)]
    Unix(std::os::unix::net::UnixListener),
    #[cfg(windows)]
    Tcp(std::net::TcpListener),
}

impl Listener {
    /// Bind the transport for `socket`: a Unix domain socket on Unix (a
    /// stale socket file from a killed daemon is removed first), or a
    /// loopback TCP listener on Windows with the ephemeral port persisted
    /// beside the socket path.
    ///
    /// # Errors
    ///
    /// A bind failure returns its message.
    fn bind(socket: &Path) -> Result<Self, String> {
        #[cfg(unix)]
        {
            if socket.exists() {
                let _ = fs::remove_file(socket);
            }
            std::os::unix::net::UnixListener::bind(socket)
                .map(Self::Unix)
                .map_err(|e| format!("cannot bind {}: {e}", socket.display()))
        }
        #[cfg(windows)]
        {
            let listener = std::net::TcpListener::bind("127.0.0.1:0")
                .map_err(|e| format!("cannot bind the loopback TCP fallback: {e}"))?;
            let port = listener
                .local_addr()
                .map_err(|e| format!("cannot read the bound port: {e}"))?
                .port();
            fs::write(socket.with_extension("port"), port.to_string())
                .map_err(|e| format!("cannot write the port file: {e}"))?;
            Ok(Self::Tcp(listener))
        }
    }

    /// Accept one connection, blocking until one arrives (this runs on
    /// the daemon's main loop, so a blocked accept costs nothing).
    ///
    /// # Errors
    ///
    /// Propagates the accept error (the caller stops on all but
    /// `Interrupted`).
    fn accept(&self) -> std::io::Result<Stream> {
        match self {
            #[cfg(unix)]
            Self::Unix(listener) => listener.accept().map(|(stream, _)| Stream::Unix(stream)),
            #[cfg(windows)]
            Self::Tcp(listener) => listener.accept().map(|(stream, _)| Stream::Tcp(stream)),
        }
    }
}

/// A connected client stream, transport-agnostic.
enum Stream {
    #[cfg(unix)]
    Unix(std::os::unix::net::UnixStream),
    #[cfg(windows)]
    Tcp(std::net::TcpStream),
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            #[cfg(unix)]
            Self::Unix(stream) => stream.read(buf),
            #[cfg(windows)]
            Self::Tcp(stream) => stream.read(buf),
        }
    }
}

impl Stream {
    /// Bound how long a client waits for the daemon's answer.
    fn set_read_timeout(&mut self, timeout: Duration) -> std::io::Result<()> {
        let timeout = Some(timeout);
        match self {
            #[cfg(unix)]
            Self::Unix(stream) => stream.set_read_timeout(timeout),
            #[cfg(windows)]
            Self::Tcp(stream) => stream.set_read_timeout(timeout),
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            #[cfg(unix)]
            Self::Unix(stream) => stream.write(buf),
            #[cfg(windows)]
            Self::Tcp(stream) => stream.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            #[cfg(unix)]
            Self::Unix(stream) => stream.flush(),
            #[cfg(windows)]
            Self::Tcp(stream) => stream.flush(),
        }
    }
}

/// Connect to the daemon's socket as a client (the same transport the
/// daemon bound) — the shared client entry the tests use and the PHP
/// loader's socket lane mirrors.
fn connect(socket: &Path) -> Result<Stream, String> {
    #[cfg(unix)]
    {
        std::os::unix::net::UnixStream::connect(socket)
            .map(Stream::Unix)
            .map_err(|e| format!("cannot connect to {}: {e}", socket.display()))
    }
    #[cfg(windows)]
    {
        let port: u16 = fs::read_to_string(socket.with_extension("port"))
            .map_err(|e| format!("cannot read the port file: {e}"))?
            .trim()
            .parse()
            .map_err(|e| format!("the port file is not a port number: {e}"))?;
        std::net::TcpStream::connect(("127.0.0.1", port))
            .map(Stream::Tcp)
            .map_err(|e| format!("cannot connect to 127.0.0.1:{port}: {e}"))
    }
}

/// Connect with the reconnection window: a failed *connect* retries with
/// a short backoff (a restarting daemon rebinds within it); every attempt's
/// failure is a `cannot connect to …` message, so the caller surfaces one
/// consistent shape and the loader falls back to its spawn lane.
fn connect_with_retries(socket: &Path) -> Result<Stream, String> {
    let mut last = String::new();
    for backoff in CONNECT_BACKOFF_MS {
        if backoff > 0 {
            std::thread::sleep(Duration::from_millis(backoff));
        }
        match connect(socket) {
            Ok(stream) => return Ok(stream),
            Err(e) => last = e,
        }
    }
    Err(format!(
        "{last} (after {CONNECT_ATTEMPTS} connect attempts)"
    ))
}

/// One request over the client protocol: compile `file` through the daemon
/// — `Ok` carries the compiled PHP, `Err` the rendered diagnostics (one
/// string per diagnostic, newline-joined). The connect carries the
/// reconnection window (`connect_with_retries`); a failure AFTER the
/// daemon accepted is final for this request.
///
/// # Errors
///
/// A connect, write, read or protocol failure returns its message.
pub fn request(socket: &Path, file: &Path) -> Result<String, String> {
    let mut stream = connect_with_retries(socket)?;
    let _ = stream.set_read_timeout(REQUEST_TIMEOUT);
    let frame = encode_frame(&encode_request(file))?;
    stream
        .write_all(&frame)
        .and_then(|()| stream.flush())
        .map_err(|e| format!("cannot send the request: {e}"))?;
    let mut reader = BufReader::new(stream);
    let payload = read_frame(&mut reader)?;
    decode_response(&payload)
}

/// Ask the daemon to stop: the framed `{"quit":true}` request. The daemon
/// answers before it exits; a transport failure is surfaced (the daemon
/// may already be gone).
///
/// # Errors
///
/// A connect or write failure returns its message.
pub fn quit(socket: &Path) -> Result<(), String> {
    let mut stream = connect_with_retries(socket)?;
    let _ = stream.set_read_timeout(REQUEST_TIMEOUT);
    let frame = encode_frame(&json!({ "quit": true }).to_string())?;
    stream
        .write_all(&frame)
        .and_then(|()| stream.flush())
        .map_err(|e| format!("cannot send the quit request: {e}"))?;
    let mut reader = BufReader::new(stream);
    // The quit answer is best-effort: a daemon that already stopped may
    // close before the read completes.
    let _ = read_frame(&mut reader);
    Ok(())
}

/// Decode one framed response payload: `{"ok":true,"php":…}` carries the
/// compiled PHP; `{"ok":false,"diag":[…]}` carries the rendered
/// diagnostics (joined with newlines — the shape the PHP loader's
/// `RuntimeException` surfaces).
fn decode_response(payload: &str) -> Result<String, String> {
    let value: Value =
        serde_json::from_str(payload).map_err(|e| format!("malformed response payload: {e}"))?;
    match value.get("ok").and_then(Value::as_bool) {
        Some(true) => value
            .get("php")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| "a success response is missing `php`".to_string()),
        Some(false) => match value.get("diag").and_then(Value::as_array) {
            // A failure response is Err — the diagnostics join the shape
            // the PHP loader's `RuntimeException` surfaces.
            Some(entries) => Err(entries
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("\n")),
            None => Err("a failure response is missing `diag`".to_string()),
        },
        None => Err("the response is missing `ok`".to_string()),
    }
}

impl Daemon {
    /// Bind the socket (a stale socket file is removed first), start the
    /// watcher over every path, and anchor the content-addressed build
    /// cache at the watch roots' common ancestor — the workspace whose
    /// `.lintel-cache/` describes these sources from any invocation
    /// directory.
    ///
    /// # Errors
    ///
    /// A bind or watcher failure returns its message.
    fn bind(socket: &Path, paths: &[PathBuf]) -> Result<Daemon, String> {
        if paths.is_empty() {
            return Err("no paths to watch: pass the project directories or files".into());
        }
        let cache: SharedCache = Arc::new(Mutex::new(HashMap::new()));
        let counters: Arc<Mutex<CacheCounters>> = Arc::new(Mutex::new(CacheCounters::default()));
        // Invalidation runs on notify's own event thread: a watcher event
        // drops the cache entries of the paths it mentions (created,
        // modified or removed alike) and the next request recompiles.
        // Paths under `.lintel-cache/` never invalidate: the daemon's own
        // cache writes must not churn the map.
        let mut watcher: Box<dyn WatcherDrop> = Box::new(
            notify::recommended_watcher({
                let cache = Arc::clone(&cache);
                move |event: notify::Result<notify::Event>| {
                    let Ok(event) = event else { return };
                    if let Ok(mut guard) = cache.lock() {
                        for path in event.paths {
                            if is_cache_path(&path) {
                                continue;
                            }
                            guard.remove(&path);
                        }
                    }
                }
            })
            .map_err(|e| format!("cannot start the file watcher: {e}"))?,
        );
        for path in paths {
            let mode = if path.is_dir() {
                notify::RecursiveMode::Recursive
            } else {
                notify::RecursiveMode::NonRecursive
            };
            watcher
                .watch(path, mode)
                .map_err(|e| format!("cannot watch {}: {e}", path.display()))?;
        }
        let listener = Listener::bind(socket)?;
        Ok(Daemon {
            listener,
            watcher,
            socket: socket.to_path_buf(),
            roots: paths.to_vec(),
            cache,
            build_cache: BuildCache::at(&watch_workspace_root(paths)),
            counters,
            quit: false,
        })
    }

    /// The daemon's main loop: a plain blocking accept — a compile request
    /// is served the moment it connects — until a client sends the quit
    /// frame or the listener fails for good. Cleanup removes the socket
    /// file whichever way the loop ends.
    fn serve(&mut self) -> i32 {
        while !self.quit {
            match self.listener.accept() {
                Ok(stream) => self.serve_stream(stream),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                // Any other accept failure ends the daemon: the socket is
                // gone (unlinked by cleanup) or the fd is unusable. A lost
                // watcher event self-heals through the mtime+size key.
                Err(_) => break,
            }
        }
        self.cleanup();
        0
    }

    /// Serve one connection: read one framed request, compile, answer,
    /// close. Marks the daemon for shutdown when the client sent the quit
    /// frame.
    fn serve_stream(&mut self, mut stream: Stream) {
        let quit = self.handle_request(&mut stream);
        let _ = stream.flush();
        self.quit = quit;
    }

    /// Read one framed request from `stream` and write the framed answer.
    /// A transport failure or an empty (EOF) probe means the client
    /// vanished or was only checking readiness — serve on. Returns
    /// whether the client sent the quit frame.
    fn handle_request(&mut self, stream: &mut Stream) -> bool {
        let payload = {
            let mut reader = BufReader::new(&mut *stream);
            read_frame(&mut reader)
        };
        let Ok(payload) = payload else {
            return false;
        };
        let request: Value = match serde_json::from_str(&payload) {
            Ok(value) => value,
            Err(e) => {
                let body = json!({
                    "ok": false,
                    "diag": [format!("lintel watch: malformed request payload: {e}")],
                })
                .to_string();
                Self::answer(stream, &body);
                return false;
            }
        };
        if request.get("quit").and_then(Value::as_bool) == Some(true) {
            Self::answer(stream, &json!({ "ok": true, "quit": true }).to_string());
            return true;
        }
        let Some(path) = request.get("path").and_then(Value::as_str) else {
            let body = json!({
                "ok": false,
                "diag": ["lintel watch: the request names neither `path` nor `quit`"],
            })
            .to_string();
            Self::answer(stream, &body);
            return false;
        };
        let body = match self.compiled(Path::new(path)) {
            Ok(php) => json!({ "ok": true, "php": php }).to_string(),
            Err(diag) => json!({ "ok": false, "diag": diag }).to_string(),
        };
        Self::answer(stream, &body);
        false
    }

    /// Write one framed answer; a write failure means the client vanished
    /// — serve on.
    fn answer(stream: &mut Stream, payload: &str) {
        if let Ok(frame) = encode_frame(payload) {
            let _ = stream.write_all(&frame).and_then(|()| stream.flush());
        }
    }

    /// The compiled PHP for `file`: an in-memory hit when the stat key
    /// matches, then a content-addressed build-cache hit (no recompile),
    /// then a compile whose artifact lands in both caches. Compile
    /// failures are cached in memory too — an unchanged broken file must
    /// not recompile on every request — but never enter the build cache
    /// (the same rule the CLI's `build` lane follows: only successes are
    /// content-addressed).
    fn compiled(&self, file: &Path) -> Result<String, Vec<String>> {
        let (mtime, size) = match fs::metadata(file) {
            Ok(meta) => {
                // The native mtime (nanosecond resolution on macOS/Linux/
                // Windows): keep it whole, not seconds-truncated, so two
                // edits inside the same second get distinct keys.
                let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                (mtime, meta.len())
            }
            Err(e) => return Err(vec![format!("cannot stat {}: {e}", file.display())]),
        };
        if let Ok(guard) = self.cache.lock() {
            if let Some(entry) = guard.get(file) {
                if entry.mtime == mtime && entry.size == size {
                    return entry.result.clone();
                }
            }
        }
        let src = fs::read_to_string(file)
            .map_err(|e| vec![format!("cannot read {}: {e}", file.display())])?;
        // The build cache: the same content-addressed engine `lintel build`
        // uses — a watch-triggered rebuild of unchanged content never
        // recompiles, a reverted edit serves instantly.
        let key = BuildCache::key(&src, Target::String);
        if let Some(hit) = self.build_cache.lookup(&key, Target::String) {
            self.bump_and_record(|counters| counters.cache_hits += 1);
            let result = Ok(hit.php);
            self.remember(file, mtime, size, &result);
            return result;
        }
        let result = match compile(
            &src,
            &Options {
                target: Target::String,
            },
        ) {
            Ok(out) => {
                let warnings_text: Vec<String> = out
                    .warnings
                    .iter()
                    .map(|warning| warning.render(&src))
                    .collect();
                let warnings_json: Vec<String> = out
                    .warnings
                    .iter()
                    .map(|warning| warning.to_json(&src))
                    .collect();
                self.build_cache.store(
                    &key,
                    file,
                    Target::String,
                    &out.php,
                    &warnings_text,
                    &warnings_json,
                );
                Ok(out.php)
            }
            Err(problems) => Err(problems
                .iter()
                .map(|p| p.render(&src))
                .collect::<Vec<String>>()),
        };
        self.bump_and_record(|counters| counters.compiled += 1);
        self.remember(file, mtime, size, &result);
        result
    }

    /// Cache one result in the in-memory stat-key map (the disk cache
    /// already holds compile successes by the time this runs).
    fn remember(
        &self,
        file: &Path,
        mtime: SystemTime,
        size: u64,
        result: &Result<String, Vec<String>>,
    ) {
        if let Ok(mut guard) = self.cache.lock() {
            guard.insert(
                file.to_path_buf(),
                CacheEntry {
                    mtime,
                    size,
                    result: result.clone(),
                },
            );
        }
    }

    /// Bump one counter and record the cumulative totals to
    /// `.lintel-cache/stats.json` (flocked — a watch daemon and a
    /// concurrent build share the file). Best-effort: a failure degrades
    /// to unrecorded counters, never to a failed request.
    fn bump_and_record(&self, bump: impl FnOnce(&mut CacheCounters)) {
        if let Ok(mut guard) = self.counters.lock() {
            bump(&mut guard);
            self.build_cache.record(&cache::CacheStats {
                compiled: guard.compiled,
                cache_hits: guard.cache_hits,
                bypassed: false,
            });
        }
    }

    /// Remove the socket file (and the Windows port file) so a later
    /// daemon can bind cleanly.
    fn cleanup(&mut self) {
        for path in &self.roots {
            let _ = self.watcher.unwatch(path);
        }
        let _ = fs::remove_file(&self.socket);
        #[cfg(windows)]
        let _ = fs::remove_file(self.socket.with_extension("port"));
    }
}

/// Whether `path` lives under a `.lintel-cache` directory: the daemon's
/// own cache writes (artifacts, sidecars, `stats.json`) must not churn
/// the in-memory map — a store or a stats record fires watcher events
/// over the watched roots, and dropping entries for them would defeat
/// the in-memory tier entirely.
fn is_cache_path(path: &Path) -> bool {
    path.components()
        .any(|component| component.as_os_str() == cache::CACHE_DIR)
}

/// The workspace root the daemon's build cache anchors at: the watch
/// roots' common ancestor (the same shape `lintel build` computes from
/// its sources and destination — the cache lands with the sources it
/// describes from any invocation directory).
fn watch_workspace_root(paths: &[PathBuf]) -> PathBuf {
    paths
        .iter()
        .map(|path| std::path::absolute(path).unwrap_or_else(|_| path.clone()))
        .map(|root| {
            if root.is_file() {
                root.parent().map(Path::to_path_buf).unwrap_or(root)
            } else {
                root
            }
        })
        .reduce(|a, b| crate::common_ancestor(&a, &b))
        .unwrap_or_else(|| PathBuf::from("."))
}
