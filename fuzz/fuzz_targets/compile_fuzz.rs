//! The T-09 fuzz target (§13.3): arbitrary bytes into `compile()` on every
//! target. Invariants: never panic, never loop (libFuzzer's own timeout), and
//! — sampled, when `PHP_BIN` is set — the output of a successfully compiling
//! input passes `php -l`. The 60-second local smoke is `make fuzz`; CI's
//! nightly lane runs the ten-minute acceptance.

#![no_main]
use libfuzzer_sys::fuzz_target;
use lintel_core::{compile, Options, Target};

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
        // with): every 20th successful compile is validated end-to-end.
        if data.len() % 20 == 0 {
            if let Some(bin) = std::env::var_os("PHP_BIN") {
                let dir = std::env::temp_dir().join("lintel-fuzz-php");
                let _ = std::fs::create_dir_all(&dir);
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
});
