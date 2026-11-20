//! `lintel init` acceptance closure (task-card T-10, in service of T-07):
//! the demo must be *runnable*, not just compilable — the compiled
//! `hello.php` passes `php -l` and **executes** under PHP with its rendered
//! HTML asserted.
//!
//! The PHP binary comes from `PHP_BIN` (default `php` on `PATH`); when no
//! PHP is available the test prints a skip note so PHP-less machines still
//! pass `cargo test`, and CI's `php` job sets `PHP_BIN=php` so it always
//! runs there.

use std::path::{Path, PathBuf};
use std::process::Command;

use lintel_cli::init;

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

/// Quote one path as a PHP single-quoted string literal.
fn php_quote(path: &Path) -> String {
    let quoted = path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('\'', "\\'");
    format!("'{quoted}'")
}

/// Run `php -l` on one file and panic with the output on failure.
fn assert_lints(bin: &Path, file: &Path) {
    let output = Command::new(bin)
        .arg("-l")
        .arg(file)
        .output()
        .expect("spawn php -l");
    assert!(
        output.status.success(),
        "php -l {} failed:\n{}{}",
        file.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn init_demo_lints_and_executes_under_php() {
    let Some(bin) = php_bin() else {
        eprintln!("skip php_demo: no PHP binary (set PHP_BIN)");
        return;
    };
    let dir = std::env::temp_dir().join(format!("lintel-php-demo-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let report = init(&dir).expect("init scaffolds the demo");

    // The compiled demo is syntactically valid PHP.
    assert_lints(&bin, &report.compiled);

    // Execute it: a runner file requires the §6.8 runtime first (the
    // `\Lintel\` helpers the compiled demo calls), then the demo, capturing
    // everything it echoes.
    let runtime =
        Path::new(&env!("CARGO_MANIFEST_DIR")).join("../../php/runtime/src/functions.php");
    let rendered = dir.join("rendered.html");
    let runner = dir.join("runner.php");
    std::fs::write(
        &runner,
        format!(
            "<?php\nrequire {};\nob_start();\nrequire {};\nfile_put_contents({}, ob_get_clean());\n",
            php_quote(&runtime),
            php_quote(&report.compiled),
            php_quote(&rendered),
        ),
    )
    .expect("write runner");
    let output = Command::new(&bin).arg(&runner).output().expect("spawn php");
    assert!(
        output.status.success(),
        "the init demo failed to execute under PHP:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let html = std::fs::read_to_string(&rendered).expect("rendered demo output");

    // The rendered HTML is the demo page — runtime internals never leak.
    assert!(html.contains("<div class=\"hello\">"), "{html}");
    assert!(html.contains("<h1>Hello, Lintel!</h1>"), "{html}");
    assert!(
        html.contains("<a href=\"https://lintel.sh\">links</a>"),
        "{html}"
    );
    assert!(html.contains("</div>"), "{html}");
    assert!(
        !html.contains('\\'),
        "runtime symbols leaked into the page: {html}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
