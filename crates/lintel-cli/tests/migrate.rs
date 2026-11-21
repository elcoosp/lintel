//! The Year-3 RFC-convergence migration contracts (`lintel migrate`,
//! architecture doc §17.1 Year 3): the rewrite is IN PLACE — a
//! `.lintel.php` source comes out as the same PHP file carrying
//! `new \Markup\Element(...)` calls instead of JSX, line-preserved.
//! `--dry-run` prints the unified diff and writes nothing; `--backup`
//! keeps the original bytes beside the rewrite; a file that fails to
//! compile or whose components no migrated source declares is refused
//! untouched, and the exit code says so.

use std::fs;
use std::path::{Path, PathBuf};

/// A scratch directory unique to one test.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lintel-migrate-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("creates scratch dir");
    dir
}

/// `run` with string args, collecting both sinks as UTF-8.
fn run_cli(argv: &[&str]) -> (i32, String, String) {
    let invocation: Vec<String> = argv.iter().copied().map(String::from).collect();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = lintel_cli::run(&invocation, &mut stdout, &mut stderr).expect("arguments parse");
    (
        code,
        String::from_utf8(stdout).expect("stdout is utf8"),
        String::from_utf8(stderr).expect("stderr is utf8"),
    )
}

/// `run` for an invocation expected to be rejected as a usage error.
fn usage_error(argv: &[&str]) -> String {
    let invocation: Vec<String> = argv.iter().copied().map(String::from).collect();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    match lintel_cli::run(&invocation, &mut stdout, &mut stderr) {
        Err(problem) => problem.0,
        Ok(_) => panic!("expected a usage error: {argv:?}"),
    }
}

/// Write `content` to `dir/name` and return the path.
fn write_source(dir: &Path, name: &str, content: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, content).expect("writes the source");
    path
}

/// The migration fixture: one declared component plus a same-file
/// invocation — the shape a real component directory has.
const FIXTURE: &str = concat!(
    "<?php\n\n",
    "function Card(string $title): Html\n{\n",
    "    return <b class=\"card\">{$title}</b>;\n}\n\n",
    "echo <Card title=\"Hi\" />;\n"
);

/// The RFC-parity lines the rewrite must produce for [`FIXTURE`]'s JSX
/// lines — the card's "output files match the expected RFC-parity PHP".
fn expected_rfc_lines() -> Vec<String> {
    vec![
        "    return new \\Markup\\Element('b', ['class' => 'card'], [$title]);".to_string(),
        "echo Card(title: 'Hi');".to_string(),
    ]
}

#[test]
fn migrate_rewrites_sources_in_place_to_rfc_parity() {
    let dir = scratch("rewrite");
    let source = write_source(&dir, "Card.lintel.php", FIXTURE);
    let (code, stdout, stderr) = run_cli(&["migrate", source.to_str().expect("utf8 path")]);
    assert_eq!(code, 0, "the rewrite succeeds: {stdout}{stderr}");
    let rewritten = fs::read_to_string(&source).expect("the source is readable");
    // The JSX is gone; the RFC lowering and the named-argument invocation
    // are in.
    assert!(!rewritten.contains("<Card"), "{rewritten}");
    assert!(!rewritten.contains("<b class=\"card\">"), "{rewritten}");
    for expected in expected_rfc_lines() {
        assert!(
            rewritten.contains(&expected),
            "the expected RFC-parity line is missing: {expected}\n{rewritten}"
        );
    }
    // Line preservation (§5.2): the rewrite is the same line count.
    assert_eq!(
        FIXTURE.lines().count(),
        rewritten.lines().count(),
        "the rewrite preserves lines: {rewritten}"
    );
    // The report names the file and the changed-line count.
    assert!(
        stdout.contains("migrated") && stdout.contains("Card.lintel.php"),
        "{stdout}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn migrate_dry_run_prints_the_diff_and_writes_nothing() {
    let dir = scratch("dry-run");
    let source = write_source(&dir, "Card.lintel.php", FIXTURE);
    let (code, stdout, stderr) =
        run_cli(&["migrate", source.to_str().expect("utf8 path"), "--dry-run"]);
    assert_eq!(code, 0, "{stdout}{stderr}");
    // The diff headers and the changed lines are on stdout.
    assert!(stdout.contains("--- a/"), "the old header prints: {stdout}");
    assert!(stdout.contains("+++ b/"), "the new header prints: {stdout}");
    assert!(stdout.contains("@@ -"), "the hunk header prints: {stdout}");
    assert!(
        stdout.contains("-    return <b class=\"card\">{$title}</b>;"),
        "the removed JSX line prints: {stdout}"
    );
    assert!(
        stdout.contains("+    return new \\Markup\\Element('b', ['class' => 'card'], [$title]);"),
        "the added RFC line prints: {stdout}"
    );
    assert!(
        stdout.contains("would migrate"),
        "the per-file report prints: {stdout}"
    );
    // The file on disk is untouched.
    let after = fs::read_to_string(&source).expect("the source is readable");
    assert_eq!(after, FIXTURE, "--dry-run writes nothing");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn migrate_dry_run_diff_headers_carry_the_function_context() {
    // The Phase 12 M1 acceptance: each hunk header carries the nearest
    // enclosing function signature (`@@ -10,5 +10,5 @@ function Badge(…)`),
    // so a migration review shows which component is being rewritten.
    let dir = scratch("diff-headers");
    // Two components, each with JSX the RFC lowering rewrites, plus the
    // two invocation lines — with enough unchanged comment lines between
    // the three changed regions that the diff emits SEPARATE hunks (the
    // hunk merge window is five ops), each carrying its own function
    // header.
    let fixture = concat!(
        "<?php\n\n",
        "function Badge(string $label, bool $verified = false): Html\n{\n",
        "    return <span class=\"badge\">{$label}</span>;\n}\n\n",
        "// --- the badge gallery keeps component bodies apart so the dry-run\n",
        "// --- diff emits separate hunks, each carrying its own header ---\n\n",
        "function Card(string $title): Html\n{\n",
        "    return <b class=\"card\">{$title}</b>;\n}\n\n",
        "// --- the card gallery keeps the invocation block apart from the\n",
        "// --- Card body, so the echo hunk names the nearest declaration ---\n\n",
        "echo <Badge label=\"Hi\" />;\n",
        "echo <Card title=\"Yo\" />;\n"
    );
    let source = write_source(&dir, "Gallery.lintel.php", fixture);
    let (code, stdout, stderr) =
        run_cli(&["migrate", source.to_str().expect("utf8 path"), "--dry-run"]);
    assert_eq!(code, 0, "{stdout}{stderr}");
    // The two function-level context headers, verbatim signatures.
    assert!(
        stdout.contains("@@ function Badge(string $label, bool $verified = false): Html"),
        "the Badge hunk header carries the signature: {stdout}"
    );
    assert!(
        stdout.contains("@@ function Card(string $title): Html"),
        "the Card hunk header carries the signature: {stdout}"
    );
    // The trailing echo lines sit after the last declaration; git's
    // funcname convention names the nearest function above them.
    assert!(
        stdout
            .matches("@@ function Card(string $title): Html")
            .count()
            >= 2,
        "the invocation hunk names the nearest declaration above it: {stdout}"
    );
    // A change before any declaration carries no context (the `<?php`
    // prologue hunk, when the compiler rewrites nothing there, does not
    // exist here — the file's first change is inside Badge).
    assert!(
        !stdout.contains("@@ @@\n"),
        "no empty context suffix: {stdout}"
    );
    // --dry-run writes nothing.
    let after = fs::read_to_string(&source).expect("the source is readable");
    assert_eq!(after, fixture, "--dry-run writes nothing");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn migrate_backup_keeps_the_original_bytes_beside_the_rewrite() {
    let dir = scratch("backup");
    let source = write_source(&dir, "Card.lintel.php", FIXTURE);
    let (code, _stdout, stderr) =
        run_cli(&["migrate", source.to_str().expect("utf8 path"), "--backup"]);
    assert_eq!(code, 0, "{stderr}");
    let backup = dir.join("Card.lintel.php.bak");
    assert!(backup.is_file(), "the .bak file exists: {dir:?}");
    let original = fs::read_to_string(&backup).expect("the backup is readable");
    assert_eq!(original, FIXTURE, "the backup carries the original bytes");
    let rewritten = fs::read_to_string(&source).expect("the source is readable");
    assert!(!rewritten.contains("<Card"), "{rewritten}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn migrate_refuses_a_parse_error_and_leaves_the_file_untouched() {
    let dir = scratch("parse-error");
    let broken = concat!(
        "<?php\n\n",
        "function Broken(): Html\n{\n",
        "    return <div>oops;\n}\n"
    );
    let source = write_source(&dir, "Broken.lintel.php", broken);
    let (code, stdout, stderr) = run_cli(&["migrate", source.to_str().expect("utf8 path")]);
    assert_eq!(code, 1, "a refused file fails the run: {stdout}{stderr}");
    let after = fs::read_to_string(&source).expect("the source is readable");
    assert_eq!(after, broken, "a refused file is untouched");
    assert!(
        stderr.contains("LNT0101") || stderr.contains("unclosed element"),
        "the compiler diagnostic surfaces: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn migrate_refuses_components_no_migrated_source_declares() {
    let dir = scratch("unresolved");
    // `Page` invokes `<Grid />` — declared in NEITHER file: the rewrite
    // would leave a call the migrated set cannot resolve.
    let page = concat!(
        "<?php\n\n",
        "function Page(): Html\n{\n",
        "    return <div><Grid rows={3} /></div>;\n}\n"
    );
    let source = write_source(&dir, "Page.lintel.php", page);
    let (code, stdout, stderr) = run_cli(&["migrate", source.to_str().expect("utf8 path")]);
    assert_eq!(
        code, 1,
        "an unresolved component fails the run: {stdout}{stderr}"
    );
    let after = fs::read_to_string(&source).expect("the source is readable");
    assert_eq!(after, page, "a refused file is untouched");
    assert!(
        stderr.contains("Grid") && stderr.contains("no migrated source"),
        "the refusal names the component: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);

    // The same invocation RESOLVES once the declaring file migrates too:
    // the set-wide name pass clears it.
    let dir = scratch("resolved-cross-file");
    let grid = concat!(
        "<?php\n\n",
        "function Grid(int $rows): Html\n{\n",
        "    return <table>{$rows}</table>;\n}\n"
    );
    let page_source = write_source(&dir, "Page.lintel.php", page);
    let grid_source = write_source(&dir, "Grid.lintel.php", grid);
    let (code, stdout, stderr) = run_cli(&[
        "migrate",
        page_source.to_str().expect("utf8 path"),
        grid_source.to_str().expect("utf8 path"),
    ]);
    assert_eq!(code, 0, "both files resolve: {stdout}{stderr}");
    let rewritten = fs::read_to_string(&page_source).expect("readable");
    assert!(
        rewritten.contains("Grid(rows: 3)") || rewritten.contains("new \\Markup\\Element('table'"),
        "the cross-file invocation migrates: {rewritten}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn migrate_already_migrated_sources_report_without_a_diff() {
    let dir = scratch("already");
    // The RFC-parity file round-trips: the compiler passes it through.
    let rfc = concat!(
        "<?php\n\n",
        "function Card(string $title): Html\n{\n",
        "    return new \\Markup\\Element('b', ['class' => 'card'], [$title]);\n}\n"
    );
    let source = write_source(&dir, "Card.lintel.php", rfc);
    let (code, stdout, stderr) = run_cli(&["migrate", source.to_str().expect("utf8 path")]);
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(
        stdout.contains("already migrated"),
        "the no-op report prints: {stdout}"
    );
    assert!(
        !stdout.contains("--- a/"),
        "no diff prints for an already-migrated file: {stdout}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn migrate_rejects_build_only_output_flags_and_to_values() {
    let problem = usage_error(&["migrate", "x.lintel.php", "--in-place"]);
    assert!(
        problem.contains("--in-place") && problem.contains("build"),
        "{problem}"
    );
    let problem = usage_error(&["migrate", "x.lintel.php", "--out", "dir"]);
    assert!(problem.contains("--out"), "{problem}");
    let problem = usage_error(&["migrate", "x.lintel.php", "--to=php"]);
    assert!(problem.contains("--to=native"), "{problem}");
    // The `--dry-run`/`--backup` flags are migrate-only.
    let problem = usage_error(&["build", "x.lintel.php", "--dry-run"]);
    assert!(problem.contains("--dry-run"), "{problem}");
    let problem = usage_error(&["check", "x.lintel.php", "--backup"]);
    assert!(problem.contains("--backup"), "{problem}");
}
