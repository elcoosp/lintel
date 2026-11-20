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

use lintel_cli::{init, init_demo, DEMO_SLUGS};

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

/// The per-demo execution assertions (card T-15: each demo must EXECUTE
/// under PHP with asserted output, not merely compile).
#[test]
fn all_five_t15_demos_lint_and_execute_under_php() {
    let Some(bin) = php_bin() else {
        eprintln!("skip php_demo: no PHP binary (set PHP_BIN)");
        return;
    };
    let runtime =
        Path::new(&env!("CARGO_MANIFEST_DIR")).join("../../php/runtime/src/functions.php");
    let dir = std::env::temp_dir().join(format!("lintel-demos-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    for (n, slug) in DEMO_SLUGS.iter().enumerate() {
        let demo = n + 1;
        let demo_dir = dir.join(format!("demo{demo}"));
        let report = init_demo(&demo_dir, Some(demo)).expect("init scaffolds the demo");
        assert!(report.compiled.to_string_lossy().contains(slug));

        // php -l on the compiled demo.
        assert_lints(&bin, &report.compiled);

        // Execute: require the runtime, then the compiled demo, capture echo.
        let rendered = demo_dir.join("rendered.html");
        let runner = demo_dir.join("runner.php");
        std::fs::write(
            &runner,
            format!(
                "<?php\nrequire {};\nob_start();\nrequire {};\nfile_put_contents({}, ob_get_clean());\n",
                php_quote(&runtime),
                php_quote(&report.compiled.clone()),
                php_quote(&rendered),
            ),
        )
        .expect("write runner");
        let output = Command::new(&bin).arg(&runner).output().expect("spawn php");
        assert!(
            output.status.success(),
            "demo {demo} failed to execute under PHP:\n{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        let html = std::fs::read_to_string(&rendered).expect("rendered demo output");
        assert_demo_output(demo, &html);
    }

    let _ = std::fs::remove_dir_all(&dir);
}

/// Per-demo output assertions.
fn assert_demo_output(demo: usize, html: &str) {
    match demo {
        1 => {
            assert!(
                html.contains("<h2 class=\"card__title\">Badges</h2>"),
                "{html}"
            );
            assert!(
                html.contains("<span class=\"badge badge--accent\">new</span>"),
                "{html}"
            );
            assert!(
                html.contains("<span class=\"badge badge--plain\">stable</span>"),
                "{html}"
            );
        }
        2 => {
            // Every payload is escaped as text; hostile URLs are blocked.
            assert!(
                html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"),
                "{html}"
            );
            assert!(html.contains("about:invalid#blocked"), "{html}");
            assert!(!html.contains("<script>alert(1)</script>"), "{html}");
            assert!(!html.contains("\" onmouseover=\""), "{html}");
        }
        3 => {
            assert!(html.contains("10,000 rows, one pass"), "{html}");
            assert_eq!(html.matches("<tr>").count(), 10_000, "10k rows rendered");
            assert!(html.contains("<td>10000</td><td>row 10000</td>"), "{html}");
        }
        4 => {
            assert!(html.contains("hx-post=\"/todos\""), "{html}");
            assert!(html.contains("todo--done"), "{html}");
            assert!(html.contains("hx-target=\"closest li\""), "{html}");
            assert!(
                html.contains("<script src=\"https://unpkg.com/htmx.org@2\" defer></script>"),
                "{html}"
            );
        }
        5 => {
            assert!(html.contains("LNT0301"), "{html}");
            assert!(html.contains("help: onclick"), "{html}");
            assert_eq!(html.matches("<article").count(), 5, "five error cards");
        }
        _ => unreachable!("five demos"),
    }
}
