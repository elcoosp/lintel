//! The `lintl` CLI contracts (architecture doc §6.7, task-card T-10
//! subset): `build` compiles to stdout/`--out`/`--in-place`, `check` is
//! the diagnostics-only CI gate, directories walk in sorted order, usage
//! errors return [`lintl_cli::CliError`] instead of touching the sinks,
//! and exit codes follow the usage text (0 clean, 1 any failure, 2 usage).
//!
//! The tests drive [`lintl_cli::run`] — the same engine `src/main.rs`
//! wires to the process sinks — against scratch directories under the
//! system temp dir.

use std::{fs, path::PathBuf};

const CLEAN_SOURCE: &str = "<?php\nreturn <b class=\"x\">{$name}</b>;\n";
const ERROR_SOURCE: &str = "<?php\nreturn <button onclick={$handler}>go</button>;\n";
const WARN_SOURCE: &str = "<?php\nreturn <button onclick=\"track('cta')\">go</button>;\n";

/// A fresh scratch directory for one test; recreated on each run.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lintl-cli-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("creates scratch dir");
    dir
}

/// `run` with string args, collecting both sinks as UTF-8.
fn run_cli(argv: &[&str]) -> (i32, String, String) {
    let invocation: Vec<String> = argv.iter().copied().map(String::from).collect();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = lintl_cli::run(&invocation, &mut stdout, &mut stderr).expect("arguments parse");
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
    match lintl_cli::run(&invocation, &mut stdout, &mut stderr) {
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
    let source = write_source(&dir, "Page.lintl.php", CLEAN_SOURCE);
    let (code, stdout, stderr) = run_cli(&["build", source.to_str().expect("utf8 path")]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stdout.contains("new \\Lintl\\Html("), "{stdout}");
    assert!(stdout.contains("class=\"x\""), "{stdout}");
    assert_eq!(stderr, "");
}

#[test]
fn build_out_dir_writes_named_php_files() {
    let dir = scratch("build-out-dir");
    let source = write_source(&dir, "Page.lintl.php", CLEAN_SOURCE);
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
    assert!(php.contains("new \\Lintl\\Html("), "{php}");
}

#[test]
fn build_in_place_writes_a_sibling_php_file() {
    let dir = scratch("build-in-place");
    let source = write_source(&dir, "Page.lintl.php", CLEAN_SOURCE);
    let (code, _, stderr) = run_cli(&["build", source.to_str().expect("utf8 path"), "--in-place"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    let compiled = dir.join("Page.php");
    let php = fs::read_to_string(&compiled).expect("sibling file exists");
    assert!(php.contains("return new \\Lintl\\Html("), "{php}");
    assert_eq!(
        fs::read_to_string(&source).expect("source intact"),
        CLEAN_SOURCE,
        "the build never overwrites its input"
    );
}

#[test]
fn build_fails_renders_diagnostics_and_writes_nothing() {
    let dir = scratch("build-error");
    let source = write_source(&dir, "Bad.lintl.php", ERROR_SOURCE);
    let out_dir = dir.join("dist");
    let (code, stdout, stderr) = run_cli(&[
        "build",
        source.to_str().expect("utf8 path"),
        "--out",
        out_dir.to_str().expect("utf8 path"),
    ]);
    assert_eq!(code, 1);
    assert_eq!(stdout, "");
    assert!(stderr.contains("error[LNL0301]"), "{stderr}");
    assert!(stderr.contains("help:"), "{stderr}");
    assert!(!out_dir.join("Bad.php").exists(), "no output on failure");
}

#[test]
fn build_warns_but_succeeds() {
    let dir = scratch("build-warn");
    let source = write_source(&dir, "Warn.lintl.php", WARN_SOURCE);
    let (code, stdout, stderr) = run_cli(&["build", source.to_str().expect("utf8 path")]);
    assert_eq!(code, 0, "warnings do not fail the build");
    assert!(stderr.contains("warning[LNL0302]"), "{stderr}");
    assert!(stdout.contains("new \\Lintl\\Html("), "{stdout}");
}

#[test]
fn build_streams_with_the_stream_target() {
    let dir = scratch("build-stream");
    let source = write_source(&dir, "Page.lintl.php", CLEAN_SOURCE);
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
    let source = write_source(&dir, "Page.lintl.php", CLEAN_SOURCE);
    let (code, stdout, stderr) = run_cli(&[
        "build",
        "--target",
        "rfc",
        source.to_str().expect("utf8 path"),
    ]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stdout.contains("\\Markup\\Element"), "{stdout}");
}

/// T-19 launch honesty (§16 post #3): `lintl migrate --to=native` must
/// exist today — in all three spellings (`--to=native`, `--to native`,
/// and the flag-free default). Since the Year-3 migration card, migrate
/// REWRITES the source in place with the RFC lowering instead of
/// printing it; the in-place contracts themselves live in tests/migrate.rs.
#[test]
fn migrate_accepts_native_in_all_spellings_and_rewrites_in_place() {
    let dir = scratch("migrate-native");
    for argv in [
        vec!["migrate", "PLACEHOLDER"],
        vec!["migrate", "PLACEHOLDER", "--to=native"],
        vec!["migrate", "PLACEHOLDER", "--to", "native"],
    ] {
        let source = write_source(&dir, "Page.lintl.php", CLEAN_SOURCE);
        let path = source.to_str().expect("utf8 path");
        let argv: Vec<String> = argv
            .iter()
            .map(|a| a.replace("PLACEHOLDER", path))
            .collect();
        let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
        let (code, stdout, stderr) = run_cli(&argv);
        assert_eq!(code, 0, "argv {argv:?}: stderr: {stderr}");
        let rewritten = std::fs::read_to_string(&source).expect("the source is readable");
        assert!(rewritten.contains("\\Markup\\Element"), "{rewritten}");
        assert!(!rewritten.contains("new \\Lintl\\Html("), "{rewritten}");
        assert!(
            stdout.contains("migrated"),
            "the rewrite reports itself: {stdout}"
        );
        let _ = std::fs::remove_file(&source);
    }
}

/// The alias is honest about its scope: only `native` exists today, and a
/// target override belongs to `build`.
#[test]
fn migrate_rejects_other_to_values_and_target_overrides() {
    let dir = scratch("migrate-reject");
    let source = write_source(&dir, "Page.lintl.php", CLEAN_SOURCE);
    let path = source.to_str().expect("utf8 path");
    for argv in [
        vec!["migrate", path, "--to=html"],
        vec!["migrate", path, "--to", "blade"],
        vec!["migrate", path, "--target=string"],
    ] {
        let problem = usage_error(&argv);
        assert!(problem.contains("migrate"), "{problem}");
    }
}

#[test]
fn check_is_quiet_when_clean_and_warns_without_failing() {
    let dir = scratch("check");
    let clean = write_source(&dir, "Clean.lintl.php", CLEAN_SOURCE);
    let warned = write_source(&dir, "Warn.lintl.php", WARN_SOURCE);
    let (code, stdout, stderr) = run_cli(&["check", clean.to_str().expect("utf8 path")]);
    assert_eq!(code, 0);
    assert_eq!(stdout, "");
    assert_eq!(stderr, "", "clean check is quiet: {stderr}");
    let (code, _, stderr) = run_cli(&["check", warned.to_str().expect("utf8 path")]);
    assert_eq!(code, 0, "warnings pass the CI gate");
    assert!(stderr.contains("warning[LNL0302]"), "{stderr}");
}

#[test]
fn check_fails_on_error_severity_diagnostics() {
    let dir = scratch("check-error");
    let source = write_source(&dir, "Bad.lintl.php", ERROR_SOURCE);
    let (code, stdout, stderr) = run_cli(&["check", source.to_str().expect("utf8 path")]);
    assert_eq!(code, 1);
    assert_eq!(stdout, "");
    assert!(stderr.contains("error[LNL0301]"), "{stderr}");
}

#[test]
fn directories_walk_recursively_in_path_order() {
    let dir = scratch("walk");
    let nested = dir.join("z-nested");
    fs::create_dir_all(&nested).expect("creates nested dir");
    write_source(&dir, "b.lintl.php", "<?php\nreturn <b>B-MARK</b>;\n");
    write_source(&nested, "a.lintl.php", "<?php\nreturn <i>A-MARK</i>;\n");
    write_source(&dir, "ignored.txt", "not a source");
    let path = dir.to_str().expect("utf8 path");
    let (code, stdout, stderr) = run_cli(&["build", path]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(!stdout.contains("not a source"), "{stdout}");
    // Full paths sort: `b.lintl.php` (at the root) precedes
    // `z-nested/a.lintl.php`, whatever their file names.
    let b = stdout.find("B-MARK").expect("b.lintl.php compiled");
    let a = stdout.find("A-MARK").expect("nested a.lintl.php compiled");
    assert!(b < a, "walked sources compile in path order: {stdout}");
    // The walk is deterministic across runs.
    let (code, stdout_again, _) = run_cli(&["build", path]);
    assert_eq!(code, 0);
    assert_eq!(stdout, stdout_again);
}

#[test]
fn check_fails_when_any_walked_file_fails() {
    let dir = scratch("walk-error");
    write_source(&dir, "Good.lintl.php", CLEAN_SOURCE);
    write_source(&dir, "Bad.lintl.php", ERROR_SOURCE);
    let (code, _, stderr) = run_cli(&["check", dir.to_str().expect("utf8 path")]);
    assert_eq!(code, 1);
    assert!(stderr.contains("error[LNL0301]"), "{stderr}");
}

#[test]
fn sources_without_the_lintl_suffix_get_a_compiled_name() {
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
    let source = write_source(&dir, "Page.lintl.php", CLEAN_SOURCE);
    let path = source.to_str().expect("utf8 path");
    assert_eq!(
        usage_error(&[]),
        "missing command: build, check, analyze, stats, rules, init, watch or migrate"
    );
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
    assert!(usage_error(&["check", "/nonexistent/lintl/path"]).contains("no such path"));
    // No usage error ever writes to the sinks.
    let mut sink = Vec::new();
    let mut null = Vec::new();
    let args: Vec<String> = ["build", "--wat", path]
        .iter()
        .copied()
        .map(String::from)
        .collect();
    assert!(lintl_cli::run(&args, &mut sink, &mut null).is_err());
    assert_eq!(sink, Vec::<u8>::new());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn help_and_version_exit_zero() {
    let (code, stdout, _) = run_cli(&["--help"]);
    assert_eq!(code, 0);
    assert!(stdout.starts_with("lintl —"), "{stdout}");
    let (code, stdout, _) = run_cli(&["build", "-h"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("USAGE:"), "{stdout}");
    let (code, stdout, _) = run_cli(&["--version"]);
    assert_eq!(code, 0);
    // The version rides CARGO_PKG_VERSION (the one workspace version, T-18):
    // a hardcoded string here would break every release bump.
    assert!(
        stdout.contains(concat!("lintl ", env!("CARGO_PKG_VERSION"))),
        "{stdout}"
    );
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
    let message = match lintl_cli::run(&args, &mut broken, &mut null) {
        Err(problem) => problem.0,
        Ok(code) => panic!("a broken stdout sink must fail the run, got exit {code}"),
    };
    assert!(message.contains("I/O error"), "{message}");
}

#[test]
fn json_format_emits_one_object_per_diagnostic() {
    let dir = scratch("json-error");
    let source = write_source(&dir, "Bad.lintl.php", ERROR_SOURCE);
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
        lines[0].starts_with("{\"code\":\"LNL0301\",\"severity\":\"error\""),
        "{stderr}"
    );
    assert!(lines[0].contains("\"line\":2"), "{stderr}");
    assert!(
        lines[0].ends_with("\"docs\":\"https://elcoosp.github.io/lintl/e/LNL0301\"}"),
        "{stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn json_format_keeps_clean_files_silent() {
    let dir = scratch("json-clean");
    let source = write_source(&dir, "Clean.lintl.php", CLEAN_SOURCE);
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
    let source = write_source(&dir, "Warn.lintl.php", WARN_SOURCE);
    let (code, _, stderr) = run_cli(&[
        "check",
        source.to_str().expect("utf8 path"),
        "--format=json",
    ]);
    assert_eq!(code, 0, "warnings stay advisory in json format");
    assert!(stderr.contains("\"code\":\"LNL0302\""), "{stderr}");
    assert!(stderr.contains("\"severity\":\"warning\""), "{stderr}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn build_json_format_writes_php_to_stdout_and_json_to_stderr() {
    let dir = scratch("json-build");
    let source = write_source(&dir, "Warn.lintl.php", WARN_SOURCE);
    let (code, stdout, stderr) = run_cli(&[
        "build",
        source.to_str().expect("utf8 path"),
        "--format=json",
    ]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stdout.contains("new \\Lintl\\Html("), "{stdout}");
    assert!(stderr.contains("\"code\":\"LNL0302\""), "{stderr}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn format_accepts_a_separate_word_and_rejects_unknown_values() {
    let dir = scratch("json-usage");
    let source = write_source(&dir, "Page.lintl.php", CLEAN_SOURCE);
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
    let source = write_source(&dir, "Page.lintl.php", STATS_SOURCE);
    let (code, stdout, stderr) = run_cli(&["stats", source.to_str().expect("utf8 path")]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stderr, "", "stats is not a linter: no diagnostics here");
    // One section per file: the path header, then the declared table and
    // the invocation list (`!` marks a required parameter; `...` marks the
    // variadic tail).
    assert!(stdout.contains("Page.lintl.php\n"), "{stdout}");
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
    let source = write_source(&dir, "Page.lintl.php", STATS_SOURCE);
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
    let bad = write_source(&dir, "Bad.lintl.php", "<?php\nreturn <b>unclosed;\n");
    let (code, stdout, stderr) = run_cli(&["stats", bad.to_str().expect("utf8 path")]);
    assert_eq!(code, 1, "a parse error fails the stats run");
    assert_eq!(stdout, "", "a failed file prints no report section");
    assert!(stderr.contains("error[LNL0101]"), "{stderr}");
    // The JSON format renders the same diagnostics as JSON lines on stderr.
    let (code, stdout, stderr) =
        run_cli(&["stats", "--format=json", bad.to_str().expect("utf8 path")]);
    assert_eq!(code, 1);
    assert_eq!(stdout, "");
    assert!(stderr.contains("\"code\":\"LNL0101\""), "{stderr}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn stats_is_not_a_linter() {
    // `ERROR_SOURCE` fails `lintl check` (the dynamic-handler `LNL0301`
    // error); `stats` runs the scanner only and reports the file's shape.
    let dir = scratch("stats-not-a-linter");
    let source = write_source(&dir, "Handlers.lintl.php", ERROR_SOURCE);
    let (code, stdout, stderr) = run_cli(&["stats", source.to_str().expect("utf8 path")]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stderr, "");
    assert!(stdout.contains("Handlers.lintl.php\n"), "{stdout}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn stats_walks_directories_in_sorted_order() {
    let dir = scratch("stats-dirs");
    let sub = dir.join("sub");
    fs::create_dir_all(&sub).expect("creates subdir");
    write_source(&dir, "Zeta.lintl.php", STATS_SOURCE);
    write_source(&sub, "Alpha.lintl.php", STATS_SOURCE);
    let (code, stdout, stderr) = run_cli(&["stats", dir.to_str().expect("utf8 path")]);
    assert_eq!(code, 0, "stderr: {stderr}");
    let zeta = stdout.find("Zeta.lintl.php").expect("zeta section");
    let alpha = stdout.find("Alpha.lintl.php").expect("alpha section");
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
// `lintl rules` — the self-describing rule-registry listing
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
        lines[0].starts_with("builtin-shadow LNL0201 "),
        "registry order, `<id> <codes> <summary>`: {stdout}"
    );
    assert!(stdout.contains("void-children LNL0401 "), "{stdout}");
}

#[test]
fn rules_json_emits_one_stable_object_per_rule() {
    let (code, stdout, stderr) = run_cli(&["rules", "--format=json"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stderr, "");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 8, "one JSON object per rule: {stdout}");
    assert!(
        lines[0].starts_with("{\"id\":\"builtin-shadow\",\"codes\":[\"LNL0201\"],\"summary\":"),
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
    let source = write_source(&dir, "Page.lintl.php", CLEAN_SOURCE);
    let (code, stdout, stderr) = run_cli(&[
        "build",
        source.to_str().expect("utf8 path"),
        &format!("--out={}", out.display()),
    ]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "", "no stdout when writing to --out");
    let written = out.join("Page.php");
    let php = fs::read_to_string(&written).expect("the inline form wrote the file");
    assert!(php.contains("new \\Lintl\\Html("), "{php}");
    let _ = fs::remove_dir_all(&dir);
}

const FUNCTION_SOURCE: &str = concat!(
    "<?php\n\nnamespace App\\Ui;\n\nuse Lintl\\Html;\n\n",
    "function card(string $title): Html\n{\n    return <div class=\"card\">{$title}</div>;\n}\n",
    "function helper(int $n): int\n{\n    return $n + 1;\n}\n"
);

#[test]
fn build_manifest_writes_the_component_map_and_prod_files() {
    // T-31: `build --out DIR --manifest PATH` emits `components` (name →
    // source) and `prod.files` (the compiled artifacts), preserving the
    // user's `functions` globs.
    let dir = scratch("manifest-build");
    fs::create_dir_all(dir.join("src")).expect("creates src/");
    let source = write_source(&dir, "src/Card.lintl.php", FUNCTION_SOURCE);
    let manifest = dir.join("lintl.json");
    fs::write(&manifest, r#"{"functions": ["src/*.lintl.php"]}"#).expect("seeds the manifest");
    let (code, stdout, stderr) = run_cli(&[
        "build",
        source.to_str().expect("utf8 path"),
        "--out",
        dir.join("build").to_str().expect("utf8 path"),
        &format!("--manifest={}", manifest.display()),
    ]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "", "no stdout when writing to --out");
    let written = fs::read_to_string(&manifest).expect("reads the manifest");
    let decoded: serde_json::Value = serde_json::from_str(&written).expect("valid JSON");
    assert_eq!(
        decoded["components"]["card"], "src/Card.lintl.php",
        "{written}"
    );
    assert_eq!(
        decoded["prod"]["artifacts"]["src/Card.lintl.php"], "build/Card.php",
        "the compiled artifact pairs with its source in prod.artifacts"
    );
    assert_eq!(decoded["functions"][0], "src/*.lintl.php", "{written}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn build_manifest_on_a_stdout_build_omits_prod() {
    let dir = scratch("manifest-stdout-build");
    let source = write_source(&dir, "Card.lintl.php", FUNCTION_SOURCE);
    let manifest = dir.join("lintl.json");
    let (code, stdout, stderr) = run_cli(&[
        "build",
        source.to_str().expect("utf8 path"),
        &format!("--manifest={}", manifest.display()),
    ]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stdout.contains("new \\Lintl\\Html("),
        "stdout still compiles"
    );
    let decoded: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest).expect("reads manifest"))
            .expect("valid JSON");
    assert_eq!(decoded["components"]["card"], "Card.lintl.php", "{decoded}");
    assert!(
        decoded.get("prod").is_none(),
        "a stdout build writes no artifacts, so no prod section"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn build_manifest_without_a_value_defaults_to_lintl_json() {
    // Bare `--manifest` never consumes the next token: a following path is
    // a build input, not a manifest value. The default anchors to the
    // workspace being compiled — the common ancestor of the build inputs
    // and `--out` — so this one runs the real binary from the scratch dir
    // (a single file input: the workspace is its parent directory).
    let dir = scratch("manifest-default");
    let source = write_source(&dir, "Card.lintl.php", FUNCTION_SOURCE);
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_lintl"))
        .args(["build", "--manifest", source.to_str().expect("utf8 path")])
        .current_dir(&dir)
        .output()
        .expect("runs the lintl binary");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        dir.join("lintl.json").is_file(),
        "the default manifest path is <workspace>/lintl.json"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn bare_manifest_lands_in_the_workspace_never_the_process_cwd() {
    // The M0 cwd-hygiene regression: the bare `--manifest` console lanes
    // used to depend on the process cwd (the PHP suites chdir'd into the
    // workspace for the write, and under some invocation orderings a stray
    // empty `lintl.json` landed at the outer cwd). The default now
    // resolves against the workspace — the common ancestor of the build
    // inputs and `--out` — so running the real binary from a DIFFERENT
    // cwd leaves the manifest inside the workspace and nothing outside it.
    let dir = scratch("manifest-cwd-hygiene");
    let workspace = dir.join("app");
    fs::create_dir_all(workspace.join("src")).expect("creates the workspace src/");
    let source = write_source(&workspace.join("src"), "Badge.lintl.php", FUNCTION_SOURCE);
    let elsewhere = dir.join("elsewhere");
    fs::create_dir_all(&elsewhere).expect("creates the foreign cwd");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_lintl"))
        .args([
            "build",
            source.to_str().expect("utf8 source path"),
            "--out",
            workspace.join("build").to_str().expect("utf8 out path"),
            "--manifest",
        ])
        .current_dir(&elsewhere)
        .output()
        .expect("runs the lintl binary");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        workspace.join("lintl.json").is_file(),
        "the manifest lands at the workspace root (the src/… + build/ ancestor)"
    );
    assert!(
        !elsewhere.join("lintl.json").exists(),
        "no manifest may land at the process cwd"
    );
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(workspace.join("lintl.json")).expect("reads"))
            .expect("valid JSON");
    assert_eq!(
        manifest["components"]["card"], "src/Badge.lintl.php",
        "the manifest keys stay relative to the workspace: {manifest}"
    );
    assert_eq!(
        manifest["prod"]["artifacts"]["src/Badge.lintl.php"], "build/Badge.php",
        "{manifest}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn in_process_bare_manifest_never_litters_the_engine_cwd() {
    // The stray-file scenario the Phase 8 backlog recorded: the in-process
    // engine shares the caller's cwd, so a bare `--manifest` resolved
    // against it could drop `lintl.json` into whatever directory a test
    // runner or REPL happened to sit in. The workspace rule keeps the
    // write inside the compiled inputs' common ancestor even when the
    // engine's cwd is somewhere unrelated (this test's cwd is the crate
    // root — it must stay manifest-free).
    let dir = scratch("manifest-in-process-cwd");
    let workspace = dir.join("ws");
    fs::create_dir_all(workspace.join("src")).expect("creates the workspace src/");
    let source = write_source(&workspace.join("src"), "Badge.lintl.php", FUNCTION_SOURCE);
    let before = PathBuf::from(".").join("lintl.json");
    assert!(!before.exists(), "the test cwd starts manifest-free");
    let (code, _stdout, stderr) = run_cli(&[
        "build",
        source.to_str().expect("utf8 source path"),
        "--out",
        workspace.join("out").to_str().expect("utf8 out path"),
        "--manifest",
    ]);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        workspace.join("lintl.json").is_file(),
        "the manifest lands at the workspace root"
    );
    assert!(
        !before.exists(),
        "no manifest may land at the engine's working directory"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn workspace_root_anchors_to_the_shared_ancestor_of_inputs_and_out() {
    // Multiple inputs + `--out`: the manifest goes to the deepest
    // directory containing them all (the app root the console lanes run
    // against), not to either input dir or the cwd.
    let dir = scratch("manifest-multi-root");
    let app = dir.join("app");
    fs::create_dir_all(app.join("views")).expect("creates views/");
    fs::create_dir_all(app.join("widgets")).expect("creates widgets/");
    let first = write_source(&app.join("views"), "Card.lintl.php", FUNCTION_SOURCE);
    let second = write_source(
        &app.join("widgets"),
        "Badge.lintl.php",
        "<?php\n\nuse Lintl\\Html;\n\nfunction badge(string $label): Html\n{\n    return <b>{$label}</b>;\n}\n",
    );
    let (code, _stdout, stderr) = run_cli(&[
        "build",
        first.to_str().expect("utf8 path"),
        second.to_str().expect("utf8 path"),
        "--out",
        app.join("cache").to_str().expect("utf8 out path"),
        "--manifest",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let manifest = fs::read_to_string(app.join("lintl.json")).expect("workspace-root manifest");
    assert!(
        manifest.contains("\"card\": \"views/Card.lintl.php\"")
            && manifest.contains("\"badge\": \"widgets/Badge.lintl.php\""),
        "keys are relative to the shared ancestor: {manifest}"
    );
    assert!(
        !dir.join("lintl.json").exists() && !app.join("views").join("lintl.json").exists(),
        "nowhere but the workspace root"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn manifest_flag_is_build_only() {
    assert!(
        usage_error(&["migrate", "--manifest", "x"]).contains("--manifest is a `build` flag"),
        "migrate emits RFC-parity artifacts no autoloader can require"
    );
    assert!(
        usage_error(&["check", "--manifest", "x"]).contains("--manifest is a `build` flag"),
        "check writes nothing, so a manifest has nothing to record"
    );
    assert!(
        usage_error(&["stats", "--manifest", "x"]).contains("--manifest is a `build` flag"),
        "stats is a shape report"
    );
}

#[test]
fn build_manifest_skips_failed_sources() {
    // A source that fails to compile contributes no manifest entries (and
    // no prod artifact), but still fails the build.
    let dir = scratch("manifest-failed");
    let good = write_source(&dir, "Good.lintl.php", FUNCTION_SOURCE);
    let bad = write_source(&dir, "Bad.lintl.php", ERROR_SOURCE);
    let manifest = dir.join("lintl.json");
    let (code, _stdout, stderr) = run_cli(&[
        "build",
        good.to_str().expect("utf8 path"),
        bad.to_str().expect("utf8 path"),
        "--out",
        dir.join("build").to_str().expect("utf8 path"),
        &format!("--manifest={}", manifest.display()),
    ]);
    assert_eq!(code, 1, "the failing source fails the build: {stderr}");
    let decoded: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest).expect("reads manifest"))
            .expect("valid JSON");
    assert_eq!(decoded["components"]["card"], "Good.lintl.php", "{decoded}");
    assert!(
        decoded["components"].get("helper").is_none(),
        "non-Html functions are not components: {decoded}"
    );
    assert_eq!(
        decoded["prod"]["artifacts"]
            .as_object()
            .map(serde_json::Map::len),
        Some(1),
        "only the compiled source pairs into prod.artifacts"
    );
    let _ = fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// `lintl analyze` (§7.3-Y2, docs/type-elision.md): the types report drives
// escape elision, the counters ride on stderr, and every deviation from the
// report contract degrades conservatively or fails loudly.
// ---------------------------------------------------------------------------

/// A dashboard with one int-proven child and one unproven child.
const ANALYZE_SOURCE: &str =
    "<?php\nfunction Page(int $id, $label) {\n  return <><p>{$id}</p><span>{$label}</span></>;\n}\n";

const ANALYZE_REPORT: &str = r#"{"version":1,"source":"Page.lintl.php","children":[{"line":3,"expr":"$id","type":"int"},{"line":3,"expr":"$label","type":"string"}]}"#;

#[test]
fn analyze_elides_proven_children_and_reports_the_rate() {
    let dir = scratch("analyze-text");
    let source = write_source(&dir, "Page.lintl.php", ANALYZE_SOURCE);
    let report = write_source(&dir, "report.json", ANALYZE_REPORT);
    let (code, stdout, stderr) = run_cli(&[
        "analyze",
        source.to_str().expect("utf8 path"),
        "--types",
        report.to_str().expect("utf8 path"),
    ]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stdout.contains("' . $id . '"),
        "the proven int child elides: {stdout}"
    );
    assert!(
        stdout.contains("\\Lintl\\c($label)"),
        "the plain string child keeps the helper: {stdout}"
    );
    assert!(
        stderr.contains("analyze "),
        "the elision report rides on stderr: {stderr}"
    );
    assert!(
        stderr.contains("2 decision-reachable, 1 elided"),
        "{stderr}"
    );
    assert!(stderr.contains("50.0% elision rate"), "{stderr}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn analyze_json_report_carries_the_documented_shape() {
    let dir = scratch("analyze-json");
    let source = write_source(&dir, "Page.lintl.php", ANALYZE_SOURCE);
    let report = write_source(&dir, "report.json", ANALYZE_REPORT);
    let (code, stdout, stderr) = run_cli(&[
        "analyze",
        source.to_str().expect("utf8 path"),
        "--types",
        report.to_str().expect("utf8 path"),
        "--format=json",
    ]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stdout.contains("' . $id . '"), "{stdout}");
    let line = stderr
        .lines()
        .find(|line| line.starts_with('{'))
        .expect("one JSON report line");
    let decoded: serde_json::Value = serde_json::from_str(line).expect("valid JSON");
    assert_eq!(decoded["file"], source.display().to_string(), "{decoded}");
    assert_eq!(decoded["children"], 2, "{decoded}");
    assert_eq!(decoded["considered"], 2, "{decoded}");
    assert_eq!(decoded["elided"], 1, "{decoded}");
    assert_eq!(decoded["rate"], 0.5, "{decoded}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn analyze_without_a_report_compiles_without_hints() {
    let dir = scratch("analyze-noreport");
    let source = write_source(&dir, "Page.lintl.php", ANALYZE_SOURCE);
    let (code, stdout, stderr) = run_cli(&["analyze", source.to_str().expect("utf8 path")]);
    assert_eq!(code, 0, "the missing sidecar is advisory: {stderr}");
    assert!(
        stdout.contains("\\Lintl\\c($id)"),
        "no hints, no elision: {stdout}"
    );
    assert!(stderr.contains("0 elided by type"), "{stderr}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn analyze_defaults_to_the_source_sidecar() {
    let dir = scratch("analyze-sidecar");
    let source = write_source(&dir, "Page.lintl.php", ANALYZE_SOURCE);
    write_source(&dir, "Page.lintl.php.lintl-types.json", ANALYZE_REPORT);
    let (code, stdout, _stderr) = run_cli(&["analyze", source.to_str().expect("utf8 path")]);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("' . $id . '"),
        "the sidecar applies: {stdout}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn analyze_explicit_types_must_exist() {
    let dir = scratch("analyze-missing-types");
    let source = write_source(&dir, "Page.lintl.php", ANALYZE_SOURCE);
    let problem = usage_error(&[
        "analyze",
        source.to_str().expect("utf8 path"),
        "--types",
        dir.join("absent.json").to_str().expect("utf8 path"),
    ]);
    assert!(problem.contains("--types"), "{problem}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn analyze_rejects_unknown_report_kinds_loudly() {
    let dir = scratch("analyze-unknown-kind");
    let source = write_source(&dir, "Page.lintl.php", ANALYZE_SOURCE);
    let report = write_source(
        &dir,
        "report.json",
        r#"{"version":1,"source":"x","children":[{"line":3,"expr":"$id","type":"digit-string"}]}"#,
    );
    let (code, stdout, stderr) = run_cli(&[
        "analyze",
        source.to_str().expect("utf8 path"),
        "--types",
        report.to_str().expect("utf8 path"),
    ]);
    assert_eq!(code, 1, "a contract mismatch fails the file: {stderr}");
    assert!(stdout.is_empty(), "{stdout}");
    assert!(
        stderr.contains("unknown child type `digit-string`"),
        "{stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn analyze_treats_known_non_elidable_kinds_as_conservative() {
    let dir = scratch("analyze-bool");
    let source = write_source(&dir, "Page.lintl.php", ANALYZE_SOURCE);
    // Two conflicting ELIDABLE kinds poison the key (the analysis is
    // ambiguous → the helper stays); a `bool` entry is filtered at parse
    // time (known but non-elidable, docs/type-elision.md).
    let report = write_source(
        &dir,
        "report.json",
        r#"{"version":1,"source":"x","children":[{"line":3,"expr":"$id","type":"int"},{"line":3,"expr":"$id","type":"float"},{"line":3,"expr":"$label","type":"int"},{"line":3,"expr":"$label","type":"float"}]}"#,
    );
    let (code, stdout, stderr) = run_cli(&[
        "analyze",
        source.to_str().expect("utf8 path"),
        "--types",
        report.to_str().expect("utf8 path"),
    ]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stdout.contains("\\Lintl\\c($id)") && stdout.contains("\\Lintl\\c($label)"),
        "bool keeps the helper and a conflict poisons the key: {stdout}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn analyze_stream_target_elides_and_usage_errors_list_the_command() {
    let dir = scratch("analyze-stream");
    let source = write_source(&dir, "Page.lintl.php", ANALYZE_SOURCE);
    let report = write_source(&dir, "report.json", ANALYZE_REPORT);
    let (code, stdout, stderr) = run_cli(&[
        "analyze",
        source.to_str().expect("utf8 path"),
        "--types",
        report.to_str().expect("utf8 path"),
        "--target=stream",
    ]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stdout.contains("yield $id;"), "{stdout}");
    assert!(!stdout.contains("\\Lintl\\c($id)"), "{stdout}");
    let _ = fs::remove_dir_all(&dir);

    // The missing-command message names the new command.
    let problem = usage_error(&[]);
    assert!(problem.contains("analyze"), "{problem}");
}

#[test]
fn check_types_renders_stale_hints_as_advisories() {
    let dir = scratch("check-stale");
    let source = write_source(&dir, "Page.lintl.php", ANALYZE_SOURCE);
    // A fresh entry (line 3 `$id`) plus a stale one (line 99, long gone).
    let report = write_source(
        &dir,
        "report.json",
        r#"{"version":1,"source":"x","children":[{"line":3,"expr":"$id","type":"int"},{"line":99,"expr":"$ghost","type":"int"}]}"#,
    );
    let (code, _stdout, stderr) = run_cli(&[
        "check",
        source.to_str().expect("utf8 path"),
        "--types",
        report.to_str().expect("utf8 path"),
    ]);
    assert_eq!(code, 0, "advisories never change the exit code: {stderr}");
    assert!(
        stderr.contains("stale analyze hint at line 99: $ghost (type int)"),
        "{stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn check_types_json_renders_stale_entries_as_objects() {
    let dir = scratch("check-stale-json");
    let source = write_source(&dir, "Page.lintl.php", ANALYZE_SOURCE);
    let report = write_source(
        &dir,
        "report.json",
        r#"{"version":1,"source":"x","children":[{"line":99,"expr":"$ghost","type":"int"}]}"#,
    );
    let (code, _stdout, stderr) = run_cli(&[
        "check",
        source.to_str().expect("utf8 path"),
        "--types",
        report.to_str().expect("utf8 path"),
        "--format=json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let line = stderr
        .lines()
        .find(|l| l.starts_with('{'))
        .expect("one JSON object");
    let decoded: serde_json::Value = serde_json::from_str(line).expect("valid JSON");
    assert_eq!(decoded["file"], source.display().to_string(), "{decoded}");
    assert_eq!(decoded["line"], 99, "{decoded}");
    assert_eq!(decoded["expr"], "$ghost", "{decoded}");
    assert_eq!(decoded["stale"], true, "{decoded}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn check_rejects_a_missing_explicit_types_file() {
    let dir = scratch("check-missing-types");
    let source = write_source(&dir, "Page.lintl.php", ANALYZE_SOURCE);
    let problem = usage_error(&[
        "check",
        source.to_str().expect("utf8 path"),
        "--types",
        dir.join("absent.json").to_str().expect("utf8 path"),
    ]);
    assert!(problem.contains("--types"), "{problem}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn analyze_reports_stale_hint_counts() {
    let dir = scratch("analyze-stale");
    let source = write_source(&dir, "Page.lintl.php", ANALYZE_SOURCE);
    let report = write_source(
        &dir,
        "report.json",
        r#"{"version":1,"source":"x","children":[{"line":3,"expr":"$id","type":"int"},{"line":99,"expr":"$ghost","type":"int"}]}"#,
    );
    let (code, _stdout, stderr) = run_cli(&[
        "analyze",
        source.to_str().expect("utf8 path"),
        "--types",
        report.to_str().expect("utf8 path"),
        "--format=json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let line = stderr
        .lines()
        .find(|l| l.starts_with('{'))
        .expect("one JSON object");
    let decoded: serde_json::Value = serde_json::from_str(line).expect("valid JSON");
    assert_eq!(decoded["stale"], 1, "{decoded}");
    assert_eq!(decoded["elided"], 1, "{decoded}");
    let _ = fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// Phase 9 M2: the content-addressed build cache (§9.3's warm-build target)
// — the stats sidecar (.lintl-cache/stats.json) is the compile-count
// observable: run 2 must compile nothing, an edited file must recompile
// alone, `--no-cache` must bypass, and the target must ride the key.
// ---------------------------------------------------------------------------

/// A two-source workspace (the manifest fixture's sources) + the stats
/// reader.
fn cache_workspace(name: &str) -> (PathBuf, Vec<PathBuf>, PathBuf) {
    let workspace = scratch(name);
    let src = workspace.join("src");
    fs::create_dir_all(&src).expect("creates src/");
    let first = write_source(&src, "Card.lintl.php", FUNCTION_SOURCE);
    let second = write_source(
        &src,
        "Badge.lintl.php",
        "<?php\n\nuse Lintl\\Html;\n\nfunction badge(string $label): Html\n{\n    return <b>{$label}</b>;\n}\n",
    );
    let out = workspace.join("out");
    (workspace, vec![first, second], out)
}

/// The last build's counters from `.lintl-cache/stats.json`.
fn cache_stats(workspace: &std::path::Path) -> (usize, usize, bool) {
    let body = fs::read_to_string(workspace.join(".lintl-cache/stats.json")).expect("reads stats");
    let value: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
    (
        usize::try_from(value["compiled"].as_u64().unwrap_or(0)).unwrap_or(0),
        usize::try_from(value["cache_hits"].as_u64().unwrap_or(0)).unwrap_or(0),
        value["bypassed"].as_bool().unwrap_or(false),
    )
}

#[test]
fn build_cache_hits_on_unchanged_sources() {
    // Cold run compiles both; the warm run over the identical workspace is
    // a full cache hit — zero compiles (the §9.3 warm-build target), and
    // the artifacts still land.
    let (workspace, sources, out) = cache_workspace("cache-hit");
    let source_args: Vec<&str> = sources
        .iter()
        .map(|path| path.to_str().expect("utf8 path"))
        .collect();
    let out_arg = out.to_str().expect("utf8 out");
    let mut argv: Vec<&str> = vec!["build"];
    argv.extend(source_args.iter().copied());
    argv.extend(["--out", out_arg]);
    let (code, _stdout, stderr) = run_cli(&argv);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(
        cache_stats(&workspace),
        (2, 0, false),
        "the cold build compiles"
    );

    let (code, _stdout, stderr) = run_cli(&argv);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(
        cache_stats(&workspace),
        (0, 2, false),
        "the warm build is all hits"
    );
    assert!(
        out.join("Card.php").is_file() && out.join("Badge.php").is_file(),
        "the artifacts land on a warm build too"
    );
    let _ = fs::remove_dir_all(&workspace);
}

#[test]
fn build_cache_recompiles_only_the_changed_source() {
    // One edited file: exactly one recompile, the other stays a hit — and
    // the rebuilt artifact carries the edit.
    let (workspace, sources, out) = cache_workspace("cache-partial");
    let source_args: Vec<&str> = sources
        .iter()
        .map(|path| path.to_str().expect("utf8 path"))
        .collect();
    let out_arg = out.to_str().expect("utf8 out");
    let mut argv: Vec<&str> = vec!["build"];
    argv.extend(source_args.iter().copied());
    argv.extend(["--out", out_arg]);
    let (code, _stdout, stderr) = run_cli(&argv);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(cache_stats(&workspace), (2, 0, false));

    fs::write(
        &sources[1],
        "<?php\n\nuse Lintl\\Html;\n\nfunction badge(string $label): Html\n{\n    return <b class=\"v2\">{$label}</b>;\n}\n",
    )
    .expect("edits one source");
    let (code, _stdout, stderr) = run_cli(&argv);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(
        cache_stats(&workspace),
        (1, 1, false),
        "only the edited source recompiles"
    );
    let artifact = fs::read_to_string(out.join("Badge.php")).expect("reads the artifact");
    assert!(
        artifact.contains("v2"),
        "the rebuilt artifact reflects the edit: {artifact}"
    );
    let _ = fs::remove_dir_all(&workspace);
}

#[test]
fn no_cache_bypasses_lookup_and_store() {
    // A populated cache plus `--no-cache`: everything recompiles and the
    // stats sidecar says so (CI's honest cold shape).
    let (workspace, sources, out) = cache_workspace("cache-bypass");
    let source_args: Vec<&str> = sources
        .iter()
        .map(|path| path.to_str().expect("utf8 path"))
        .collect();
    let out_arg = out.to_str().expect("utf8 out");
    let mut argv: Vec<&str> = vec!["build"];
    argv.extend(source_args.iter().copied());
    argv.extend(["--out", out_arg]);
    let (code, _stdout, stderr) = run_cli(&argv);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(cache_stats(&workspace), (2, 0, false));

    let mut bypass: Vec<&str> = vec!["build"];
    bypass.extend(source_args.iter().copied());
    bypass.extend(["--out", out_arg, "--no-cache"]);
    let (code, _stdout, stderr) = run_cli(&bypass);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(
        cache_stats(&workspace),
        (2, 0, true),
        "the bypass compiles everything and records itself"
    );
    let _ = fs::remove_dir_all(&workspace);
}

#[test]
fn the_cache_is_target_aware() {
    // The same source under two targets: the stream build must not hit
    // the string entries (the artifacts differ for the same source).
    let (workspace, sources, out) = cache_workspace("cache-targets");
    let source_args: Vec<&str> = sources
        .iter()
        .map(|path| path.to_str().expect("utf8 path"))
        .collect();
    let out_arg = out.to_str().expect("utf8 out");
    let mut argv: Vec<&str> = vec!["build"];
    argv.extend(source_args.iter().copied());
    argv.extend(["--out", out_arg]);
    let (code, _stdout, stderr) = run_cli(&argv);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(cache_stats(&workspace), (2, 0, false));

    let mut stream: Vec<&str> = vec!["build", "--target=stream"];
    stream.extend(source_args.iter().copied());
    stream.extend(["--out", out_arg]);
    let (code, _stdout, stderr) = run_cli(&stream);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(
        cache_stats(&workspace),
        (2, 0, false),
        "a different target misses: the key carries the target"
    );
    let stream_artifact = fs::read_to_string(out.join("Badge.php")).expect("reads");
    assert!(
        stream_artifact.contains("\\Generator"),
        "the stream artifact really is the stream lowering: {stream_artifact}"
    );
    let _ = fs::remove_dir_all(&workspace);
}

#[test]
fn check_never_touches_the_cache() {
    // `check` must always re-validate — no cache directory appears.
    let (workspace, sources, _out) = cache_workspace("cache-check");
    let source_args: Vec<&str> = sources
        .iter()
        .map(|path| path.to_str().expect("utf8 path"))
        .collect();
    let mut argv: Vec<&str> = vec!["check"];
    argv.extend(source_args.iter().copied());
    let (code, _stdout, stderr) = run_cli(&argv);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        !workspace.join(".lintl-cache").exists(),
        "check writes no cache"
    );
    // And `--no-cache` is refused on the lanes that never cache.
    assert!(
        usage_error(&["check", "--no-cache", source_args[0]])
            .contains("--no-cache is a `build` flag"),
        "check rejects the bypass flag"
    );
    assert!(
        usage_error(&["migrate", "--no-cache", source_args[0]])
            .contains("--no-cache is a `build` flag"),
        "migrate rejects the bypass flag"
    );
    let _ = fs::remove_dir_all(&workspace);
}
