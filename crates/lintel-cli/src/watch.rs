//! The T-12 watch daemon (architecture doc §11, §15 T-12): `lintel watch`
//! keeps one resident compiler process behind a socket, so the dev loader's
//! per-request compile skips the process spawn.
//!
//! The daemon watches the given paths with the `notify` crate, maintains an
//! in-memory **mtime+size** cache of compiled `.lintel.php` files (the same
//! key the dev loader persists to disk, §21.2), and serves one request per
//! connection:
//!
//! ```text
//! client -> /abs/path/to/Page.lintel.php\n
//! daemon <- OK <len>\n<compiled php bytes>      on success
//! daemon <- ERR <len>\n<rendered diagnostics>   on a failed compile
//! client -> quit\n                              stops the daemon
//! ```
//!
//! The design is deliberately **two-threaded, not three**: the daemon's own
//! loop is a plain *blocking* `accept` — a compile request is answered the
//! moment it connects, with no poll latency (the p50 the T-12 acceptance
//! measures) — and cache invalidation runs on `notify`'s own event thread
//! through a shared cache, so the daemon spawns no threads of its own.
//!
//! On a watcher event the cache entry for the changed path is dropped; the
//! next request recompiles lazily (§15: "invalidate cache, recompile, and
//! serve the new PHP on the next socket request"). A cache hit requires the
//! file's current mtime and size to match the compiled entry's, so an edit
//! the watcher missed still recompiles — the cache is an optimization,
//! never a staleness source.
//!
//! Unix runs a `UnixListener`; Windows falls back to a loopback TCP socket
//! (the same protocol, with the chosen ephemeral port written to
//! `<socket-path>.port`). The Windows lane compiles everywhere, but its
//! socket tests are DEFERRED-TO-CI/MAINTAINER: no Windows runner exists in
//! this sandbox — the acceptance card names this exact limit for T-12.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lintel_core::{compile, Diagnostic, Options, Target};

/// How long one client waits for the daemon's answer before giving up (a
/// stuck compile must not wedge the loader).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

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

/// One compiled file: the stat key it was compiled under and the result
/// (compiled PHP, or the rendered diagnostics that failed it).
struct CacheEntry {
    mtime: u64,
    size: u64,
    result: Result<String, String>,
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
         a client's `quit` stops it"
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
    // Readiness: poll-connect until the daemon answers (or the child dies).
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(mut stream) = connect(socket) {
            // The daemon answers connections with nothing until a
            // request line arrives; an immediate close is fine.
            let _ = stream.write_all(b"quit\n");
            let _ = stream.flush();
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

/// The daemon: its socket listener, its watcher and its shared cache. The
/// serving loop and the watcher's invalidation thread (notify's own) are
/// the only execution contexts; the daemon spawns no threads itself.
struct Daemon {
    listener: Listener,
    watcher: Box<dyn WatcherDrop>,
    socket: PathBuf,
    roots: Vec<PathBuf>,
    cache: SharedCache,
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

/// One request over the client protocol: compile `file` through the daemon
/// — `Ok` carries the compiled PHP, `Err` the rendered diagnostics.
///
/// # Errors
///
/// A connect, write, read or protocol failure returns its message.
pub fn request(socket: &Path, file: &Path) -> Result<String, String> {
    let mut stream = connect(socket)?;
    let _ = stream.set_read_timeout(REQUEST_TIMEOUT);
    stream
        .write_all(format!("{}\n", file.display()).as_bytes())
        .and_then(|()| stream.flush())
        .map_err(|e| format!("cannot send the request: {e}"))?;
    let mut reader = BufReader::new(stream);
    let mut header = String::new();
    reader
        .read_line(&mut header)
        .map_err(|e| format!("cannot read the response header: {e}"))?;
    let header = header.trim_end();
    let Some((tag, len)) = header.split_once(' ') else {
        return Err(format!("malformed response header: {header:?}"));
    };
    let Ok(len) = len.parse::<usize>() else {
        return Err(format!("malformed response length: {header:?}"));
    };
    let mut body = vec![0_u8; len];
    reader
        .read_exact(&mut body)
        .map_err(|e| format!("cannot read the response body: {e}"))?;
    let body =
        String::from_utf8(body).map_err(|e| format!("the response body is not UTF-8: {e}"))?;
    match tag {
        "OK" => Ok(body),
        "ERR" => Err(body),
        other => Err(format!("unknown response tag: {other:?}")),
    }
}

impl Daemon {
    /// Bind the socket (a stale socket file is removed first) and start
    /// the watcher over every path.
    ///
    /// # Errors
    ///
    /// A bind or watcher failure returns its message.
    fn bind(socket: &Path, paths: &[PathBuf]) -> Result<Daemon, String> {
        if paths.is_empty() {
            return Err("no paths to watch: pass the project directories or files".into());
        }
        let cache: SharedCache = Arc::new(Mutex::new(HashMap::new()));
        // Invalidation runs on notify's own event thread: a watcher event
        // drops the cache entries of the paths it mentions (created,
        // modified or removed alike) and the next request recompiles.
        let mut watcher: Box<dyn WatcherDrop> = Box::new(
            notify::recommended_watcher({
                let cache = Arc::clone(&cache);
                move |event: notify::Result<notify::Event>| {
                    let Ok(event) = event else { return };
                    if let Ok(mut guard) = cache.lock() {
                        for path in event.paths {
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
            quit: false,
        })
    }

    /// The daemon's main loop: a plain blocking accept — a compile request
    /// is served the moment it connects — until a client sends `quit` or
    /// the listener fails for good. Cleanup removes the socket file
    /// whichever way the loop ends.
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

    /// Serve one connection: read the request line, compile, answer,
    /// close. Marks the daemon for shutdown when the client sent `quit`.
    fn serve_stream(&mut self, mut stream: Stream) {
        let quit = self.handle_request(&mut stream);
        let _ = stream.flush();
        self.quit = quit;
    }

    /// Read one request line from `stream` and write the framed answer.
    /// A transport failure means the client vanished — serve on. Returns
    /// whether the client sent `quit`.
    fn handle_request(&mut self, stream: &mut Stream) -> bool {
        let mut line = String::new();
        let read = {
            let mut reader = BufReader::new(&mut *stream);
            reader.read_line(&mut line)
        };
        let Ok(read) = read else {
            return false;
        };
        if read == 0 {
            // A readiness probe opened and closed the connection.
            return false;
        }
        let request = line.trim();
        if request == "quit" {
            return true;
        }
        let file = PathBuf::from(request);
        let body = self.compiled(&file);
        let (tag, bytes) = match &body {
            Ok(php) => ("OK", php.as_bytes()),
            Err(diagnostics) => ("ERR", diagnostics.as_bytes()),
        };
        let Ok(header) = write_header(tag, bytes.len()) else {
            return false;
        };
        // A failed write means the client vanished — serve on. Only the
        // client's `quit` line ends the daemon.
        let _ = stream
            .write_all(header.as_bytes())
            .and_then(|()| stream.write_all(bytes));
        false
    }

    /// The compiled PHP for `file`: a cache hit when the stat key matches,
    /// a (re)compile otherwise. Compile failures are cached too — an
    /// unchanged broken file must not recompile on every request.
    fn compiled(&self, file: &Path) -> Result<String, String> {
        let (mtime, size) = match fs::metadata(file) {
            Ok(meta) => {
                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_secs());
                (mtime, meta.len())
            }
            Err(e) => return Err(format!("cannot stat {}: {e}", file.display())),
        };
        if let Ok(guard) = self.cache.lock() {
            if let Some(entry) = guard.get(file) {
                if entry.mtime == mtime && entry.size == size {
                    return entry.result.clone();
                }
            }
        }
        let src =
            fs::read_to_string(file).map_err(|e| format!("cannot read {}: {e}", file.display()))?;
        let result = match compile(
            &src,
            &Options {
                target: Target::String,
            },
        ) {
            Ok(out) => Ok(out.php),
            Err(problems) => Err(render_all(&problems, &src)),
        };
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
        result
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

/// The framed response header: `<TAG> <len>` terminated by a newline.
fn write_header(tag: &str, len: usize) -> Result<String, std::fmt::Error> {
    let mut header = String::new();
    writeln!(header, "{tag} {len}")?;
    Ok(header)
}

/// Render every diagnostic the way the CLI's text format does, so the ERR
/// body is directly readable by a developer.
fn render_all(problems: &[Diagnostic], src: &str) -> String {
    let mut out = String::new();
    for problem in problems {
        let _ = write!(out, "{}", problem.render(src));
    }
    out
}
