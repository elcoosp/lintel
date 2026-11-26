//! PHP acceptance gate (task-card T-05 closure, in service of T-07): every
//! compiled golden output must be **syntactically valid PHP** — `php -l`
//! runs on the exact bytes [`compile`] emits, on all three targets. The
//! `rfc` target's output is syntax-valid but references the future native
//! RFC's `\Markup\*` classes, which no current runtime defines, so only
//! string/stream outputs are *executed* (the `lintl init` demo gate).
//!
//! The PHP binary comes from `PHP_BIN` (default `php` on `PATH`). When no
//! PHP is available the gate prints a skip note so PHP-less machines still
//! pass `cargo test`; CI's `php` job sets `PHP_BIN=php` so the gate always
//! runs there.

use std::path::{Path, PathBuf};
use std::process::Command;

use lintl_core::{compile, Options, Target};

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

/// Run `php -l` on one file; `Err` carries the full output for the test.
fn lint_php(bin: &Path, file: &Path) -> Result<(), String> {
    let output = Command::new(bin)
        .arg("-l")
        .arg(file)
        .output()
        .map_err(|error| format!("spawning {:?}: {error}", bin.display()))?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "php -l {} failed:\n--- stdout ---\n{}\n--- stderr ---\n{}",
        file.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    ))
}

/// Compile `src` on `target` and `php -l` the exact emitted bytes.
fn assert_emitted_php_lints(name: &str, src: &str, target: Target) {
    let Some(bin) = php_bin() else {
        eprintln!("skip php_gate/{name}: no PHP binary (set PHP_BIN)");
        return;
    };
    let php = match compile(src, &Options { target }) {
        Ok(out) => out.php,
        Err(problems) => panic!("{name}: unexpected diagnostics: {problems:?}"),
    };
    let dir = std::env::temp_dir().join(format!("lintl-php-gate-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp gate dir");
    let file = dir.join(format!("{name}.php"));
    std::fs::write(&file, &php).expect("write compiled output");
    let result = lint_php(&bin, &file);
    let _ = std::fs::remove_dir_all(&dir);
    if let Err(report) = result {
        panic!("{name}: emitted PHP does not lint:\n{report}\n--- emitted ---\n{php}");
    }
}

/// Golden sources, kept byte-identical to `tests/golden.rs` (separate test
/// binaries cannot share consts).
const BADGE_SRC: &str = "\
<?php
function Badge(string $label, bool $verified = false): Html {
    return (
        <span class=\"badge\">
            {$label}
            {$verified && <i class=\"icon-check\" />}
        </span>
    );
}
";

const PAGE_SRC: &str = "\
<?php
function Page(string $name): Html {
    return <Card title={$name} flagged>
        <p>Hello</p>
    </Card>;
}
";

const MULTILINE_SRC: &str = "\
<?php
function Page(string $user): Html {
    return <>
        <h1>Hi</h1>
        {match ($kind) {
            'a' => <b>one</b>,
            default => <i>two</i>,
        }}
        <img src={$avatar} alt=\"\" />
    </>;
}
";

/// Exercises every runtime helper the compiler can emit on a dynamic
/// attribute path: `u`, `cls`, `sty`, `a`, `attrs` (spread) and `srcset`.
const DYNAMIC_ATTRS_SRC: &str = "\
<?php
function Row(string $id, $href, $cls, $sty, $attrs, $variants): Html {
    return <tr id={$id}>
        <td><a href={$href} class={$cls} style={$sty} {...$attrs}><img srcset={$variants} src={$href} title=\"static\" /></a></td>
    </tr>;
}
";

#[test]
fn string_target_goldens_are_valid_php() {
    assert_emitted_php_lints("badge_string", BADGE_SRC, Target::String);
    assert_emitted_php_lints("page_component_string", PAGE_SRC, Target::String);
    assert_emitted_php_lints(
        "fragment_fold_string",
        "<?php\nreturn <>{'a & b'}<br/><p>x</p></>;\n",
        Target::String,
    );
    assert_emitted_php_lints(
        "integer_literal_string",
        "<?php\nreturn <b>{42}</b>;\n",
        Target::String,
    );
    assert_emitted_php_lints(
        "comparison_operator_string",
        "<?php\n$smaller = $a < $b;\n",
        Target::String,
    );
    assert_emitted_php_lints("multiline_component_string", MULTILINE_SRC, Target::String);
}

#[test]
fn string_target_dynamic_helpers_are_valid_php() {
    assert_emitted_php_lints("dynamic_attrs_string", DYNAMIC_ATTRS_SRC, Target::String);
}

#[test]
fn stream_target_outputs_are_valid_php() {
    assert_emitted_php_lints("badge_stream", BADGE_SRC, Target::Stream);
    assert_emitted_php_lints("page_component_stream", PAGE_SRC, Target::Stream);
    assert_emitted_php_lints("dynamic_attrs_stream", DYNAMIC_ATTRS_SRC, Target::Stream);
}

#[test]
fn rfc_target_outputs_are_syntactically_valid_php() {
    // Syntax-only by design: `\Markup\*` is the future native RFC runtime,
    // undefined under today's PHP — execution is out of scope here.
    assert_emitted_php_lints("badge_rfc", BADGE_SRC, Target::Rfc);
    assert_emitted_php_lints("page_component_rfc", PAGE_SRC, Target::Rfc);
}

#[test]
fn transcribed_runtime_lints_clean() {
    let Some(bin) = php_bin() else {
        eprintln!("skip php_gate/runtime: no PHP binary (set PHP_BIN)");
        return;
    };
    let runtime =
        Path::new(&env!("CARGO_MANIFEST_DIR")).join("../../php/runtime/src/functions.php");
    if let Err(report) = lint_php(&bin, &runtime) {
        panic!("php/runtime/src/functions.php does not lint:\n{report}");
    }
}
