//! The `lintel` CLI contracts (architecture doc §6.7, task-card T-10
//! subset): `build` compiles to stdout/`--out`/`--in-place`, `check` is
//! the diagnostics-only CI gate, directories walk in sorted order, usage
//! errors return [`lintel_cli::CliError`] instead of touching the sinks,
//! and exit codes follow the usage text (0 clean, 1 any failure, 2 usage).
//!
//! The tests drive [`lintel_cli::run`] — the same engine `src/main.rs`
//! wires to the process sinks — against scratch directories under the
//! system temp dir.

use std::{fs, path::PathBuf};

const CLEAN_SOURCE: &str = "<?php\nreturn <b class=\"x\">{$name}</b>;\n";
const ERROR_SOURCE: &str = "<?php\nreturn <button onclick={$handler}>go</button>;\n";
const WARN_SOURCE: &str = "<?php\nreturn <button onclick=\"track('cta')\">go</button>;\n";

/// A fresh scratch directory for one test; recreated on each run.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lintel-cli-{}-{name}", std::process::id()));
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
        Ok(code) => panic!("expected a usage error, got exit {code}"),
    }
}

fn write_source(dir: &std::path::Path, name: &str, content: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, content).expect("writes source");
    path
}

#[test]
fn build_compiles_a_component_to_stdout() {
    let dir = scratch("build-stdout");
    let source = write_source(&dir, "Page.lintel.php", CLEAN_SOURCE);
    let (code, stdout, stderr) = run_cli(&["build", source.to_str().expect("utf8 path")]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stdout.contains("new \\Lintel\\Html("), "{stdout}");
    assert!(stdout.contains("class=\"x\""), "{stdout}");
    assert_eq!(stderr, "");
}

#[test]
fn build_out_dir_writes_named_php_files() {
    let dir = scratch("build-out-dir");
    let source = write_source(&dir, "Page.lintel.php", CLEAN_SOURCE);
    let out_dir = dir.join("dist");
    let (code, _, stderr) = run_cli(&[
        "build",
        source.to_str().expect("utf8 path"),
        "--out",
        out_dir.to_str().expect("utf8 path"),
    ]);
    assert_eq!(code, 0, "stderr: {stderr}");
    let compiled = out_dir.join("Page.php");
    let php = fs::read_to_string(&compiled).expect("compiled file exists");
    assert!(php.contains("new \\Lintel\\Html("), "{php}");
}

#[test]
fn build_in_place_writes_a_sibling_php_file() {
    let dir = scratch("build-in-place");
    let source = write_source(&dir, "Page.lintel.php", CLEAN_SOURCE);
    let (code, _, stderr) = run_cli(&["build", source.to_str().expect("utf8 path"), "--in-place"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    let compiled = dir.join("Page.php");
    let php = fs::read_to_string(&compiled).expect("sibling file exists");
    assert!(php.contains("return new \\Lintel\\Html("), "{php}");
    assert_eq!(
        fs::read_to_string(&source).expect("source intact"),
        CLEAN_SOURCE,
        "the build never overwrites its input"
    );
}

#[test]
fn build_fails_renders_diagnostics_and_writes_nothing() {
    let dir = scratch("build-error");
    let source = write_source(&dir, "Bad.lintel.php", ERROR_SOURCE);
    let out_dir = dir.join("dist");
    let (code, stdout, stderr) = run_cli(&[
        "build",
        source.to_str().expect("utf8 path"),
        "--out",
        out_dir.to_str().expect("utf8 path"),
    ]);
    assert_eq!(code, 1);
    assert_eq!(stdout, "");
    assert!(stderr.contains("error[LNT0301]"), "{stderr}");
    assert!(stderr.contains("help:"), "{stderr}");
    assert!(!out_dir.join("Bad.php").exists(), "no output on failure");
}

#[test]
fn build_warns_but_succeeds() {
    let dir = scratch("build-warn");
    let source = write_source(&dir, "Warn.lintel.php", WARN_SOURCE);
    let (code, stdout, stderr) = run_cli(&["build", source.to_str().expect("utf8 path")]);
    assert_eq!(code, 0, "warnings do not fail the build");
    assert!(stderr.contains("warning[LNT0302]"), "{stderr}");
    assert!(stdout.contains("new \\Lintel\\Html("), "{stdout}");
}

#[test]
fn build_streams_with_the_stream_target() {
    let dir = scratch("build-stream");
    let source = write_source(&dir, "Page.lintel.php", CLEAN_SOURCE);
    let (code, stdout, stderr) = run_cli(&[
        "build",
        "--target=stream",
        source.to_str().expect("utf8 path"),
    ]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stdout.contains("static function () use ($name): \\Generator"),
        "{stdout}"
    );
}

#[test]
fn build_accepts_the_rfc_target_as_a_separate_word() {
    let dir = scratch("build-rfc");
    let source = write_source(&dir, "Page.lintel.php", CLEAN_SOURCE);
    let (code, stdout, stderr) = run_cli(&[
        "build",
        "--target",
        "rfc",
        source.to_str().expect("utf8 path"),
    ]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stdout.contains("\\Markup\\Element"), "{stdout}");
}

#[test]
fn check_is_quiet_when_clean_and_warns_without_failing() {
    let dir = scratch("check");
    let clean = write_source(&dir, "Clean.lintel.php", CLEAN_SOURCE);
    let warned = write_source(&dir, "Warn.lintel.php", WARN_SOURCE);
    let (code, stdout, stderr) = run_cli(&["check", clean.to_str().expect("utf8 path")]);
    assert_eq!(code, 0);
    assert_eq!(stdout, "");
    assert_eq!(stderr, "", "clean check is quiet: {stderr}");
    let (code, _, stderr) = run_cli(&["check", warned.to_str().expect("utf8 path")]);
    assert_eq!(code, 0, "warnings pass the CI gate");
    assert!(stderr.contains("warning[LNT0302]"), "{stderr}");
}

#[test]
fn check_fails_on_error_severity_diagnostics() {
    let dir = scratch("check-error");
    let source = write_source(&dir, "Bad.lintel.php", ERROR_SOURCE);
    let (code, stdout, stderr) = run_cli(&["check", source.to_str().expect("utf8 path")]);
    assert_eq!(code, 1);
    assert_eq!(stdout, "");
    assert!(stderr.contains("error[LNT0301]"), "{stderr}");
}

#[test]
fn directories_walk_recursively_in_path_order() {
    let dir = scratch("walk");
    let nested = dir.join("z-nested");
    fs::create_dir_all(&nested).expect("creates nested dir");
    write_source(&dir, "b.lintel.php", "<?php\nreturn <b>B-MARK</b>;\n");
    write_source(&nested, "a.lintel.php", "<?php\nreturn <i>A-MARK</i>;\n");
    write_source(&dir, "ignored.txt", "not a source");
    let path = dir.to_str().expect("utf8 path");
    let (code, stdout, stderr) = run_cli(&["build", path]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(!stdout.contains("not a source"), "{stdout}");
    // Full paths sort: `b.lintel.php` (at the root) precedes
    // `z-nested/a.lintel.php`, whatever their file names.
    let b = stdout.find("B-MARK").expect("b.lintel.php compiled");
    let a = stdout.find("A-MARK").expect("nested a.lintel.php compiled");
    assert!(b < a, "walked sources compile in path order: {stdout}");
    // The walk is deterministic across runs.
    let (code, stdout_again, _) = run_cli(&["build", path]);
    assert_eq!(code, 0);
    assert_eq!(stdout, stdout_again);
}

#[test]
fn check_fails_when_any_walked_file_fails() {
    let dir = scratch("walk-error");
    write_source(&dir, "Good.lintel.php", CLEAN_SOURCE);
    write_source(&dir, "Bad.lintel.php", ERROR_SOURCE);
    let (code, _, stderr) = run_cli(&["check", dir.to_str().expect("utf8 path")]);
    assert_eq!(code, 1);
    assert!(stderr.contains("error[LNT0301]"), "{stderr}");
}

#[test]
fn sources_without_the_lintel_suffix_get_a_compiled_name() {
    let dir = scratch("compiled-name");
    let source = write_source(&dir, "plain.php", CLEAN_SOURCE);
    let (code, _, stderr) = run_cli(&["build", source.to_str().expect("utf8 path"), "--in-place"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    let compiled = dir.join("plain.php.compiled.php");
    assert!(compiled.exists(), "the build never overwrites its input");
}

#[test]
fn usage_errors_cover_flags_paths_and_modes() {
    let dir = scratch("usage");
    let source = write_source(&dir, "Page.lintel.php", CLEAN_SOURCE);
    let path = source.to_str().expect("utf8 path");
    assert_eq!(usage_error(&[]), "missing command: build, check or stats");
    assert!(usage_error(&["compile", path]).contains("unknown command `compile`"));
    assert!(usage_error(&["build", "--wat", path]).contains("unknown flag `--wat`"));
    assert!(usage_error(&["build", "--target=go", path]).contains("unknown target `go`"));
    assert!(usage_error(&["build", path, "--target"]).contains("--target needs a value"));
    assert!(usage_error(&["build", path, "--out"]).contains("--out needs a directory argument"));
    assert!(
        usage_error(&["build", path, "--in-place", "--out", "dist"]).contains("mutually exclusive")
    );
    assert!(usage_error(&["check", path, "--out", "dist"]).contains("--out is a `build` flag"));
    assert!(usage_error(&["check", path, "--in-place"]).contains("--in-place is a `build` flag"));
    assert!(usage_error(&["build"]).contains("needs at least one path"));
    assert!(usage_error(&["check", "/nonexistent/lintel/path"]).contains("no such path"));
    // No usage error ever writes to the sinks.
    let mut sink = Vec::new();
    let mut null = Vec::new();
    let args: Vec<String> = ["build", "--wat", path]
        .iter()
        .copied()
        .map(String::from)
        .collect();
    assert!(lintel_cli::run(&args, &mut sink, &mut null).is_err());
    assert_eq!(sink, Vec::<u8>::new());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn help_and_version_exit_zero() {
    let (code, stdout, _) = run_cli(&["--help"]);
    assert_eq!(code, 0);
    assert!(stdout.starts_with("lintel —"), "{stdout}");
    let (code, stdout, _) = run_cli(&["build", "-h"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("USAGE:"), "{stdout}");
    let (code, stdout, _) = run_cli(&["--version"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("lintel 0.1.0"), "{stdout}");
}

/// A writer that always fails: the sink-injection contract lets tests (and
/// embedders) drive write-error propagation without a real filesystem.
struct BrokenSink;

impl std::io::Write for BrokenSink {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("sink is broken"))
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn version_write_errors_propagate_like_help_write_errors() {
    let args: Vec<String> = ["--version"].iter().copied().map(String::from).collect();
    let mut broken = BrokenSink;
    let mut null = Vec::new();
    let message = match lintel_cli::run(&args, &mut broken, &mut null) {
        Err(problem) => problem.0,
        Ok(code) => panic!("a broken stdout sink must fail the run, got exit {code}"),
    };
    assert!(message.contains("I/O error"), "{message}");
}

#[test]
fn json_format_emits_one_object_per_diagnostic() {
    let dir = scratch("json-error");
    let source = write_source(&dir, "Bad.lintel.php", ERROR_SOURCE);
    let (code, stdout, stderr) = run_cli(&[
        "check",
        source.to_str().expect("utf8 path"),
        "--format=json",
    ]);
    assert_eq!(code, 1);
    assert_eq!(stdout, "");
    let lines: Vec<&str> = stderr.lines().collect();
    assert_eq!(lines.len(), 1, "one JSON object per line: {stderr}");
    assert!(
        lines[0].starts_with("{\"code\":\"LNT0301\",\"severity\":\"error\""),
        "{stderr}"
    );
    assert!(lines[0].contains("\"line\":2"), "{stderr}");
    assert!(
        lines[0].ends_with("\"docs\":\"https://lintel.sh/e/LNT0301\"}"),
        "{stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn json_format_keeps_clean_files_silent() {
    let dir = scratch("json-clean");
    let source = write_source(&dir, "Clean.lintel.php", CLEAN_SOURCE);
    let (code, stdout, stderr) = run_cli(&[
        "check",
        source.to_str().expect("utf8 path"),
        "--format=json",
    ]);
    assert_eq!(code, 0);
    assert_eq!(stdout, "");
    assert_eq!(stderr, "");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn json_format_reports_warnings_without_failing_the_gate() {
    let dir = scratch("json-warn");
    let source = write_source(&dir, "Warn.lintel.php", WARN_SOURCE);
    let (code, _, stderr) = run_cli(&[
        "check",
        source.to_str().expect("utf8 path"),
        "--format=json",
    ]);
    assert_eq!(code, 0, "warnings stay advisory in json format");
    assert!(stderr.contains("\"code\":\"LNT0302\""), "{stderr}");
    assert!(stderr.contains("\"severity\":\"warning\""), "{stderr}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn build_json_format_writes_php_to_stdout_and_json_to_stderr() {
    let dir = scratch("json-build");
    let source = write_source(&dir, "Warn.lintel.php", WARN_SOURCE);
    let (code, stdout, stderr) = run_cli(&[
        "build",
        source.to_str().expect("utf8 path"),
        "--format=json",
    ]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stdout.contains("new \\Lintel\\Html("), "{stdout}");
    assert!(stderr.contains("\"code\":\"LNT0302\""), "{stderr}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn format_accepts_a_separate_word_and_rejects_unknown_values() {
    let dir = scratch("json-usage");
    let source = write_source(&dir, "Page.lintel.php", CLEAN_SOURCE);
    let path = source.to_str().expect("utf8 path");
    let (code, _, stderr) = run_cli(&["check", "--format", "json", path]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stderr, "");
    assert!(usage_error(&["check", "--format=xml", path]).contains("unknown format `xml`"));
    assert!(usage_error(&["build", path, "--format"]).contains("--format needs a value"));
    let _ = fs::remove_dir_all(&dir);
}

/// A file with one declared component (a required and an optional
/// parameter) and two invocations — the second from a component declared
/// in another file.
const STATS_SOURCE: &str = "\
<?php
function Card(string $title, int $count = 0): Html {
    return <div class=\"card\">{$title}</div>;
}
echo <Card title=\"hi\">text</Card>;
echo <Grid rows={$rows} />;
";

#[test]
fn stats_prints_declared_and_invoked_components_as_text() {
    let dir = scratch("stats-text");
    let source = write_source(&dir, "Page.lintel.php", STATS_SOURCE);
    let (code, stdout, stderr) = run_cli(&["stats", source.to_str().expect("utf8 path")]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stderr, "", "stats is not a linter: no diagnostics here");
    // One section per file: the path header, then the declared table and
    // the invocation list (`!` marks a required parameter; `...` marks the
    // variadic tail).
    assert!(stdout.contains("Page.lintel.php\n"), "{stdout}");
    assert!(
        stdout.contains("  declared Card(title!, count)\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("  invoke <Card> at 5:6 props: title; children\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("  invoke <Grid> at 6:6 props: rows; undeclared\n"),
        "{stdout}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn stats_json_is_one_object_per_file_with_the_file_field() {
    let dir = scratch("stats-json");
    let source = write_source(&dir, "Page.lintel.php", STATS_SOURCE);
    let (code, stdout, stderr) = run_cli(&[
        "stats",
        "--format=json",
        source.to_str().expect("utf8 path"),
    ]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stderr, "");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 1, "one JSON object per file: {stdout}");
    assert!(
        lines[0].starts_with("{\"file\":\""),
        "the file field leads: {}",
        lines[0]
    );
    assert!(lines[0].contains("\"name\":\"Card\""), "{}", lines[0]);
    assert!(
        lines[0].contains("\"declared_in_file\":true"),
        "{}",
        lines[0]
    );
    assert!(
        lines[0].contains("\"declared_in_file\":false"),
        "{}",
        lines[0]
    );
    assert!(lines[0].contains("\"variadic\":false"), "{}", lines[0]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn stats_reports_parse_errors_and_fails_the_run() {
    let dir = scratch("stats-parse-error");
    let bad = write_source(&dir, "Bad.lintel.php", "<?php\nreturn <b>unclosed;\n");
    let (code, stdout, stderr) = run_cli(&["stats", bad.to_str().expect("utf8 path")]);
    assert_eq!(code, 1, "a parse error fails the stats run");
    assert_eq!(stdout, "", "a failed file prints no report section");
    assert!(stderr.contains("error[LNT0101]"), "{stderr}");
    // The JSON format renders the same diagnostics as JSON lines on stderr.
    let (code, stdout, stderr) =
        run_cli(&["stats", "--format=json", bad.to_str().expect("utf8 path")]);
    assert_eq!(code, 1);
    assert_eq!(stdout, "");
    assert!(stderr.contains("\"code\":\"LNT0101\""), "{stderr}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn stats_is_not_a_linter() {
    // `ERROR_SOURCE` fails `lintel check` (the dynamic-handler `LNT0301`
    // error); `stats` runs the scanner only and reports the file's shape.
    let dir = scratch("stats-not-a-linter");
    let source = write_source(&dir, "Handlers.lintel.php", ERROR_SOURCE);
    let (code, stdout, stderr) = run_cli(&["stats", source.to_str().expect("utf8 path")]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stderr, "");
    assert!(stdout.contains("Handlers.lintel.php\n"), "{stdout}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn stats_walks_directories_in_sorted_order() {
    let dir = scratch("stats-dirs");
    let sub = dir.join("sub");
    fs::create_dir_all(&sub).expect("creates subdir");
    write_source(&dir, "Zeta.lintel.php", STATS_SOURCE);
    write_source(&sub, "Alpha.lintel.php", STATS_SOURCE);
    let (code, stdout, stderr) = run_cli(&["stats", dir.to_str().expect("utf8 path")]);
    assert_eq!(code, 0, "stderr: {stderr}");
    let zeta = stdout.find("Zeta.lintel.php").expect("zeta section");
    let alpha = stdout.find("Alpha.lintel.php").expect("alpha section");
    // The documented `collect_sources` contract sorts by **full path** for
    // deterministic output: byte-wise, `dir/Zeta…` (0x5A) precedes
    // `dir/sub/Alpha…` (0x73) even though the walk visits the subdir first.
    assert!(zeta < alpha, "sections are sorted by full path: {stdout}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn stats_usage_errors_follow_the_shared_contract() {
    assert!(usage_error(&["stats"]).contains("needs at least one path"));
    assert!(usage_error(&["stats", "--out", "dist", "x"]).contains("is a `build` flag"));
    assert!(usage_error(&["stats", "--format=yaml", "x"]).contains("unknown format"));
}

// ---------------------------------------------------------------------------
// `lintel rules` — the self-describing rule-registry listing
// (`docs/rule-registry.md`): compile-time knowledge, no paths, exit 0.
// ---------------------------------------------------------------------------

#[test]
fn rules_lists_the_builtin_registry_as_text() {
    let (code, stdout, stderr) = run_cli(&["rules"]);
    assert_eq!(code, 0, "the registry listing never fails: {stderr}");
    assert_eq!(stderr, "");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 8, "one line per built-in rule: {stdout}");
    assert!(
        lines[0].starts_with("builtin-shadow LNT0201 "),
        "registry order, `<id> <codes> <summary>`: {stdout}"
    );
    assert!(stdout.contains("void-children LNT0401 "), "{stdout}");
}

#[test]
fn rules_json_emits_one_stable_object_per_rule() {
    let (code, stdout, stderr) = run_cli(&["rules", "--format=json"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stderr, "");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 8, "one JSON object per rule: {stdout}");
    assert!(
        lines[0].starts_with("{\"id\":\"builtin-shadow\",\"codes\":[\"LNT0201\"],\"summary\":"),
        "stable field order, registry order: {stdout}"
    );
    assert!(lines[7].contains("void-children"), "{stdout}");
}

#[test]
fn rules_rejects_paths_unknown_flags_and_bad_formats() {
    let message = usage_error(&["rules", "src/"]);
    assert!(message.contains("takes no paths"), "{message}");
    let message = usage_error(&["rules", "--target=stream"]);
    assert!(message.contains("unknown flag"), "{message}");
    let message = usage_error(&["rules", "--format=xml"]);
    assert!(message.contains("unknown format"), "{message}");
}

#[test]
fn inline_out_flags_follow_the_build_only_guard() {
    // The `--out=DIR` spelling must reject exactly like the space form:
    // both are `build`-only, under every command (the inline form used to
    // slip past the guard and be silently ignored by check/stats).
    assert!(
        usage_error(&["check", "--out=dist", "x"]).contains("is a `build` flag"),
        "check rejects the inline form"
    );
    assert!(
        usage_error(&["stats", "--out=dist", "x"]).contains("is a `build` flag"),
        "stats rejects the inline form"
    );
    assert!(
        usage_error(&["check", "--out", "dist", "x"]).contains("is a `build` flag"),
        "check rejects the space form"
    );
}

#[test]
fn build_accepts_both_out_spellings() {
    // The guard fix must not disturb `build`: both spellings write the
    // compiled file into the target directory.
    let dir = scratch("out-spellings");
    let out = dir.join("dist");
    let source = write_source(&dir, "Page.lintel.php", CLEAN_SOURCE);
    let (code, stdout, stderr) = run_cli(&[
        "build",
        source.to_str().expect("utf8 path"),
        &format!("--out={}", out.display()),
    ]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "", "no stdout when writing to --out");
    let written = out.join("Page.php");
    let php = fs::read_to_string(&written).expect("the inline form wrote the file");
    assert!(php.contains("new \\Lintel\\Html("), "{php}");
    let _ = fs::remove_dir_all(&dir);
}
