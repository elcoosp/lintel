//! The §10 transport seam, driven from the COMPILER side (the Phase 13 M2
//! card): the stream-target artifact a `lintl` build emits must stream
//! through the PHP runtime's flush transports over a real `php -S` HTTP
//! server — the same wire contract the PHP-side live lanes assert
//! (`X-Accel-Buffering: no`, the content type, the multi-chunk body in
//! source order). This closes the loop the live adapter lanes start: the
//! compiled bytes are the bytes the transports carry.
//!
//! Skips (with a note) when no PHP binary is available or the runtime's
//! vendor autoload is missing (php-less machines and the CI rust job);
//! CI's php job sets `PHP_BIN=php` and vendors the runtime, so it always
//! runs there.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use lintl_core::{compile, Options, Target};

/// A three-line page with a dynamic child: the stream target must yield
/// one chunk per source line around it (the flush points the transports
/// ride), never an `Html` value.
const PAGE: &str = concat!(
    "<?php\n",
    "$second = 'chunk two';\n",
    "return (\n",
    "  <main>\n",
    "    <h1>lintl transport seam</h1>\n",
    "    <p>{$second}</p>\n",
    "    <p>chunk three</p>\n",
    "  </main>\n",
    ");\n",
);

/// The worker the compiled artifact streams through: the artifact file
/// returns its root generator; `stream_via` pushes every chunk through the
/// FPM transport (headers + echo + flush), exactly the FPM lane's shape.
const WORKER: &str = r"<?php declare(strict_types=1);
require {AUTOLOAD};

$page = require {ARTIFACT};

use function Lintl\stream_via;

stream_via($page, new \Lintl\FpmFlushTransport());
";

/// Resolve the PHP binary: `PHP_BIN` overrides the default `php` on `PATH`.
fn php_bin() -> Option<PathBuf> {
    let name = std::env::var("PHP_BIN").unwrap_or_else(|_| "php".into());
    if name.contains('/') {
        let path = PathBuf::from(name);
        return path.is_file().then_some(path);
    }
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|dir| dir.join(&name))
        .find(|candidate| candidate.is_file())
}

/// The runtime's vendor autoload (the `FpmFlushTransport` + `stream_via`
/// home).
fn runtime_autoload() -> Option<PathBuf> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../php/runtime/vendor/autoload.php");
    path.canonicalize().ok().filter(|p| p.is_file())
}

/// Quote one path as a PHP single-quoted string literal (`var_export`
/// style).
fn php_quote(path: &Path) -> String {
    let quoted = path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('\'', "\\'");
    format!("'{quoted}'")
}

/// Fetch one URL, raw bytes (status line + headers + body) included.
fn fetch_raw(port: u16) -> Option<Vec<u8>> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    stream
        .write_all(
            format!(
                "GET /worker.php HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
            )
            .as_bytes(),
        )
        .ok()?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).ok()?;
    Some(raw)
}

/// Serve `docroot` with `php -S` on successive ports and return the first
/// raw response whose body carries the page marker (the way the PHP-side
/// live lanes poll a booting fixture server).
fn serve_and_fetch(php: &Path, docroot: &Path) -> Option<Vec<u8>> {
    let mut last_raw: Option<Vec<u8>> = None;
    for attempt in 0..10u16 {
        let port = 23500 + attempt;
        let mut child = Command::new(php)
            .args([
                "-S",
                &format!("127.0.0.1:{port}"),
                "-t",
                &docroot.to_string_lossy(),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn php -S");
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(200));
            if let Some(raw) = fetch_raw(port) {
                let text = String::from_utf8_lossy(&raw).to_string();
                if text.contains("lintl transport seam") {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Some(raw);
                }
                last_raw = Some(raw);
            }
        }
        let _ = child.kill();
        let _ = child.wait();
    }
    last_raw
}

#[test]
fn stream_target_artifact_streams_through_the_fpm_transport() {
    let Some(php) = php_bin() else {
        eprintln!("skip stream_transport_contract: no PHP binary (set PHP_BIN)");
        return;
    };
    let Some(autoload) = runtime_autoload() else {
        eprintln!("skip stream_transport_contract: php/runtime vendor autoload missing (composer install)");
        return;
    };

    // 1. The compiler half: the stream target lowers the page to a root
    //    generator — one yield per source line around the dynamic child
    //    (the flush points), no `Html` wrapper.
    let out = compile(
        PAGE,
        &Options {
            target: Target::Stream,
        },
    )
    .expect("the page compiles clean on the stream target");
    assert!(
        out.php.contains(": \\Generator"),
        "the artifact declares the generator the transports consume: {}",
        out.php
    );
    assert!(
        !out.php.contains("new \\Lintl\\Html("),
        "the stream artifact never wraps the root in an Html value: {}",
        out.php
    );
    let yields = out.php.matches("yield").count();
    assert!(
        yields >= 3,
        "the flush points exist: the artifact yields per source line, got {yields} yields: {}",
        out.php
    );

    // 2. The transport half: the artifact + a FpmFlushTransport worker
    //    served by `php -S`, fetched raw off the wire.
    let dir = std::env::temp_dir().join(format!(
        "lintl-stream-contract-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_millis()
    ));
    std::fs::create_dir_all(&dir).expect("create the docroot");
    let artifact = dir.join("page.php");
    std::fs::write(&artifact, &out.php).expect("write the compiled artifact");
    let worker = dir.join("worker.php");
    std::fs::write(
        &worker,
        WORKER
            .replace("{AUTOLOAD}", &php_quote(&autoload))
            .replace("{ARTIFACT}", &php_quote(&artifact)),
    )
    .expect("write the worker");

    let Some(raw) = serve_and_fetch(&php, &dir) else {
        let worker_bytes = std::fs::read(&worker).unwrap_or_default();
        panic!(
            "the php -S fixture server never served the streamed artifact; worker as written: {}",
            String::from_utf8_lossy(&worker_bytes)
        );
    };

    // 3. The wire contract, asserted on the bytes the compiled artifact
    //    produced through the FPM transport. (php -S closes instead of
    //    chunk-encoding a short response, so the wire-ordering proof stays
    //    with the progressive-timing lane; this lane asserts the contract
    //    the compiled artifact must satisfy.)
    let text = String::from_utf8_lossy(&raw).to_string();
    let (head, body) = text
        .split_once("\r\n\r\n")
        .expect("an HTTP response with a header block");
    let head = head.to_ascii_lowercase();
    assert!(
        head.contains("x-accel-buffering: no"),
        "the FPM runtime's buffering-off header rides the response: {head}"
    );
    assert!(
        head.contains("content-type: text/html"),
        "the content type rides the response: {head}"
    );
    for marker in [
        "<main>",
        "<h1>lintl transport seam</h1>",
        "<p>chunk two</p>",
        "<p>chunk three</p>",
        "</main>",
    ] {
        assert!(body.contains(marker), "the body carries {marker}: {body}");
    }
    let first = body.find("<h1>lintl transport seam</h1>").expect("the h1");
    let second = body.find("<p>chunk two</p>").expect("chunk two");
    let third = body.find("<p>chunk three</p>").expect("chunk three");
    assert!(
        first < second && second < third,
        "the chunks arrive in source order (the flush points preserved over the wire)"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
