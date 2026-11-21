//! The T-09 fuzz target (§13.3): arbitrary bytes into `compile()` on every
//! target AND — since the Phase 12 M2 card — into the LSP server session
//! that drives the tree-sitter lane. Invariants: never panic, never loop
//! (libFuzzer's own timeout), valid PHP in → valid PHP out (sampled `php
//! -l`: when the input's own PHP parses, every target's output must too —
//! the compiler is not a PHP parser, so garbage non-JSX bytes pass through
//! verbatim by the §3 principle), and the LSP session (initialize →
//! didOpen → shutdown → exit, the merged scanner+tree-sitter declaration
//! view with the scanner-fallback lane on malformed input) ends cleanly on
//! any input. The 60-second local smoke is `make fuzz`; CI's nightly lane
//! runs the ten-minute acceptance over the seeded corpus
//! (`crates/lintel-core/tests/fuzz_corpus.rs`).

#![no_main]
use libfuzzer_sys::fuzz_target;
use lintel_core::{compile, Options, Target};

/// Frame one client message the LSP JSON-RPC loop speaks:
/// `Content-Length: N`, CRLF CRLF, then N bytes.
fn framed(body: &str) -> Vec<u8> {
    format!("Content-Length: {}\r\n\r\n{body}", body.len()).into_bytes()
}

fuzz_target!(|data: &[u8]| {
    let Ok(source) = std::str::from_utf8(data) else {
        return;
    };
    for target in [Target::String, Target::Stream, Target::Rfc] {
        let Ok(output) = compile(source, &Options { target }) else {
            // Diagnostics are the compile-failure contract; no panic, no loop.
            continue;
        };
        // Sampled php -l (a process spawn per case is far too slow to fuzz
        // with): every 20th case is validated end-to-end. The invariant is
        // **valid PHP in → valid PHP out**: the compiler is not a PHP
        // parser — non-JSX bytes pass through verbatim (§3's sacred
        // principle), so a source whose plain PHP is garbage compiles and
        // emits the same garbage (the corpus found that boundary in the
        // first minute of the Phase 12 smoke: `function T(---…---$p)`).
        // What must never happen is the compiler CORRUPTING valid PHP:
        // when the input itself passes php -l, every target's output must
        // pass too.
        if data.len() % 20 == 0 {
            if let Some(bin) = std::env::var_os("PHP_BIN") {
                let dir = std::env::temp_dir().join("lintel-fuzz-php");
                let _ = std::fs::create_dir_all(&dir);
                let input_file = dir.join(format!("input-{}.php", std::process::id()));
                std::fs::write(&input_file, source).expect("write fuzz input");
                let input_ok = std::process::Command::new(&bin)
                    .arg("-l")
                    .arg(&input_file)
                    .output()
                    .expect("spawn php -l")
                    .status
                    .success();
                if input_ok {
                    let file = dir.join(format!("case-{}.php", std::process::id()));
                    std::fs::write(&file, &output.php).expect("write fuzz case");
                    let status = std::process::Command::new(&bin)
                        .arg("-l")
                        .arg(&file)
                        .output()
                        .expect("spawn php -l");
                    assert!(
                        status.status.success(),
                        "valid input emitted php that fails -l:\n{}{}",
                        String::from_utf8_lossy(&status.stdout),
                        String::from_utf8_lossy(&status.stderr),
                    );
                }
            }
        }
    }
    // The tree-sitter lane (Phase 12 M2): a full server session over the
    // fuzz input — initialize (no workspace walk), didOpen (the merged
    // scanner+tree-sitter declaration view, falling back to the scanner
    // lane when the grammar cannot recover the declarations), shutdown,
    // exit. Invariants: never panic, never loop, clean exit code — the
    // scanner-fallback lane must survive arbitrary bytes, not just the
    // shapes its unit tests probe.
    let initialize = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "capabilities": {}, "rootUri": null },
    });
    let did_open = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": "file:///fuzz/Case.lintel.php",
                "languageId": "php",
                "version": 1,
                "text": source,
            },
        },
    });
    let shutdown = serde_json::json!({ "jsonrpc": "2.0", "id": 2, "method": "shutdown" });
    let exit = serde_json::json!({ "jsonrpc": "2.0", "method": "exit" });
    let mut input: Vec<u8> = Vec::new();
    for body in [initialize, did_open, shutdown, exit] {
        input.extend_from_slice(&framed(&body.to_string()));
    }
    let mut output = Vec::new();
    let code = lintel_lsp::run(&mut std::io::Cursor::new(input.as_slice()), &mut output);
    assert_eq!(code, 0, "the LSP session ends cleanly on any input");
});
