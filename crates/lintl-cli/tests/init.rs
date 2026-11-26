//! The `lintl init` contract (architecture doc §15 task-card T-10
//! acceptance: "`lintl init` yields a runnable demo in one command"):
//! init scaffolds a demo `hello.lintl.php` into the target directory and
//! compiles it once to `hello.php`, so the one command leaves a runnable
//! page. It refuses to clobber existing files, creates missing
//! directories, and takes no flags.

use std::{fs, path::PathBuf};

/// A fresh scratch directory for one test; recreated on each run.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lintl-init-{}-{name}", std::process::id()));
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

#[test]
fn init_scaffolds_a_runnable_demo_in_one_command() {
    let dir = scratch("runnable");
    let (code, stdout, stderr) = run_cli(&["init", dir.to_str().expect("utf8 path")]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stderr.is_empty(), "{stderr}");

    let source = dir.join("hello.lintl.php");
    let php = dir.join("hello.php");
    assert!(source.exists(), "the demo source is written: {stdout}");
    assert!(php.exists(), "the demo is compiled in the same command");

    let src = fs::read_to_string(&source).expect("source is utf8");
    let compiled = fs::read_to_string(&php).expect("compiled is utf8");
    // The demo itself exercises the compiler: a component declared in the
    // file, a `string`-typed parameter, dynamic children, an attribute.
    assert!(src.contains("function Hello(string $name): Html"), "{src}");
    assert!(src.contains("<Hello name=\"Lintl\" />"), "{src}");
    // The compiled page is plain, vendor-free PHP with the typed-parameter
    // direct escape (docs/escape-elision.md) — no `<?=` shortcut, no runtime.
    assert!(compiled.contains("new \\Lintl\\Html("), "{compiled}");
    assert!(
        compiled.contains("\\htmlspecialchars($name,"),
        "the demo's string-typed child elides: {compiled}"
    );
    assert_eq!(
        compiled.lines().count(),
        src.lines().count(),
        "line preservation holds for the demo"
    );
    assert!(stdout.contains("hello.lintl.php"), "{stdout}");
    assert!(stdout.contains("hello.php"), "{stdout}");
}

#[test]
fn init_output_compiles_cleanly_through_the_engine() {
    let dir = scratch("roundtrip");
    let (code, _, _) = run_cli(&["init", dir.to_str().expect("utf8 path")]);
    assert_eq!(code, 0);
    let source = dir.join("hello.lintl.php");
    let (build_code, build_stdout, build_stderr) =
        run_cli(&["build", source.to_str().expect("utf8 path")]);
    assert_eq!(build_code, 0, "the demo builds: {build_stderr}");
    assert!(
        build_stdout.contains("new \\Lintl\\Html("),
        "{build_stdout}"
    );
    let (check_code, _, _) = run_cli(&["check", source.to_str().expect("utf8 path")]);
    assert_eq!(check_code, 0, "the demo passes the CI gate");
}

#[test]
fn init_refuses_to_clobber_an_existing_demo() {
    let dir = scratch("clobber");
    let (code, _, _) = run_cli(&["init", dir.to_str().expect("utf8 path")]);
    assert_eq!(code, 0);
    let before = fs::read_to_string(dir.join("hello.lintl.php")).expect("demo exists");
    let problem = usage_error(&["init", dir.to_str().expect("utf8 path")]);
    assert!(problem.contains("refusing to overwrite"), "{problem}");
    let after = fs::read_to_string(dir.join("hello.lintl.php")).expect("demo survives");
    assert_eq!(before, after, "the existing demo is untouched");
}

#[test]
fn init_creates_missing_directories() {
    let dir = scratch("nested");
    let nested = dir.join("site").join("pages");
    let (code, _, stderr) = run_cli(&["init", nested.to_str().expect("utf8 path")]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(nested.join("hello.lintl.php").exists());
    assert!(nested.join("hello.php").exists());
}

#[test]
fn init_takes_no_flags_and_at_most_one_directory() {
    let dir = scratch("flags");
    let problem = usage_error(&["init", dir.to_str().expect("utf8 path"), "--target=stream"]);
    assert!(problem.contains("init"), "{problem}");
    let problem = usage_error(&["init", dir.to_str().expect("utf8 path"), "--format=json"]);
    assert!(problem.contains("init"), "{problem}");
    let problem = usage_error(&["init", "a", "b"]);
    assert!(problem.contains("at most one"), "{problem}");
}

#[test]
fn init_demo_scaffolds_all_five_t15_demos() {
    for (n, slug) in lintl_cli::DEMO_SLUGS.iter().enumerate() {
        let demo = n + 1;
        let dir = scratch(&format!("demo{demo}"));
        let (code, stdout, stderr) = run_cli(&[
            "init",
            dir.to_str().expect("utf8 path"),
            "--demo",
            &demo.to_string(),
        ]);
        assert_eq!(code, 0, "stderr: {stderr}");

        let source_name = lintl_cli::demo_source_name(demo);
        let compiled_name = lintl_cli::demo_compiled_name(demo);
        let source = dir.join(source_name);
        let compiled = dir.join(compiled_name);
        assert!(source.exists(), "demo {demo} source written: {stdout}");
        assert!(
            compiled.exists(),
            "demo {demo} compiled in the same command"
        );

        // Card T-15: each demo is <= 60 lines.
        let src = fs::read_to_string(&source).expect("source is utf8");
        let lines = src.lines().count();
        assert!(lines <= 60, "demo {demo} must be <= 60 lines, got {lines}");
        assert!(source_name.contains(slug), "{source_name}");
    }
}

#[test]
fn init_demo_accepts_the_equals_form_and_rejects_bad_numbers() {
    let dir = scratch("demo-equals");
    let (code, _, stderr) = run_cli(&["init", dir.to_str().expect("utf8 path"), "--demo=3"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(dir.join("demo3-table-10k.lintl.php").exists());

    let problem = usage_error(&["init", "--demo", "6"]);
    assert!(problem.contains("unknown demo `6`"), "{problem}");
    let problem = usage_error(&["init", "--demo", "0"]);
    assert!(problem.contains("unknown demo `0`"), "{problem}");
    let problem = usage_error(&["init", "--demo", "many"]);
    assert!(problem.contains("needs a number"), "{problem}");
    let problem = usage_error(&["init", "--demo"]);
    assert!(problem.contains("needs a value"), "{problem}");
}

#[test]
fn init_demo_never_clobbers_a_different_demo() {
    let dir = scratch("demo-clobber");
    let (code, _, _) = run_cli(&["init", dir.to_str().expect("utf8 path"), "--demo", "1"]);
    assert_eq!(code, 0);
    // A second demo in the same directory is fine (different file names).
    let (code, _, stderr) = run_cli(&["init", dir.to_str().expect("utf8 path"), "--demo", "5"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(dir.join("demo1-component-library.lintl.php").exists());
    assert!(dir.join("demo5-error-showcase.lintl.php").exists());
    // The SAME demo refuses to clobber.
    let problem = usage_error(&["init", dir.to_str().expect("utf8 path"), "--demo", "5"]);
    assert!(problem.contains("refusing to overwrite"), "{problem}");
}

#[test]
fn every_demo_source_compiles_zero_diagnostics_and_stays_short() {
    for (n, source) in lintl_cli::DEMO_SOURCES.iter().enumerate() {
        let demo = n + 1;
        let lines = source.lines().count();
        assert!(lines <= 60, "demo {demo} must be <= 60 lines, got {lines}");
        match lintl_core::compile(source, &lintl_core::Options::default()) {
            Ok(output) => {
                assert!(
                    output.warnings.is_empty(),
                    "demo {demo} carries warnings: {:?}",
                    output.warnings
                );
                assert_eq!(
                    output.php.lines().count(),
                    lines,
                    "demo {demo} line preservation"
                );
            }
            Err(problems) => panic!("demo {demo} does not compile: {problems:?}"),
        }
    }
}
