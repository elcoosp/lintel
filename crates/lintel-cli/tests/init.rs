//! The `lintel init` contract (architecture doc §15 task-card T-10
//! acceptance: "`lintel init` yields a runnable demo in one command"):
//! init scaffolds a demo `hello.lintel.php` into the target directory and
//! compiles it once to `hello.php`, so the one command leaves a runnable
//! page. It refuses to clobber existing files, creates missing
//! directories, and takes no flags.

use std::{fs, path::PathBuf};

/// A fresh scratch directory for one test; recreated on each run.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lintel-init-{}-{name}", std::process::id()));
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

#[test]
fn init_scaffolds_a_runnable_demo_in_one_command() {
    let dir = scratch("runnable");
    let (code, stdout, stderr) = run_cli(&["init", dir.to_str().expect("utf8 path")]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stderr.is_empty(), "{stderr}");

    let source = dir.join("hello.lintel.php");
    let php = dir.join("hello.php");
    assert!(source.exists(), "the demo source is written: {stdout}");
    assert!(php.exists(), "the demo is compiled in the same command");

    let src = fs::read_to_string(&source).expect("source is utf8");
    let compiled = fs::read_to_string(&php).expect("compiled is utf8");
    // The demo itself exercises the compiler: a component declared in the
    // file, a `string`-typed parameter, dynamic children, an attribute.
    assert!(src.contains("function Hello(string $name): Html"), "{src}");
    assert!(src.contains("<Hello name=\"Lintel\" />"), "{src}");
    // The compiled page is plain, vendor-free PHP with the typed-parameter
    // direct escape (docs/escape-elision.md) — no `<?=` shortcut, no runtime.
    assert!(compiled.contains("new \\Lintel\\Html("), "{compiled}");
    assert!(
        compiled.contains("\\htmlspecialchars($name,"),
        "the demo's string-typed child elides: {compiled}"
    );
    assert_eq!(
        compiled.lines().count(),
        src.lines().count(),
        "line preservation holds for the demo"
    );
    assert!(stdout.contains("hello.lintel.php"), "{stdout}");
    assert!(stdout.contains("hello.php"), "{stdout}");
}

#[test]
fn init_output_compiles_cleanly_through_the_engine() {
    let dir = scratch("roundtrip");
    let (code, _, _) = run_cli(&["init", dir.to_str().expect("utf8 path")]);
    assert_eq!(code, 0);
    let source = dir.join("hello.lintel.php");
    let (build_code, build_stdout, build_stderr) =
        run_cli(&["build", source.to_str().expect("utf8 path")]);
    assert_eq!(build_code, 0, "the demo builds: {build_stderr}");
    assert!(
        build_stdout.contains("new \\Lintel\\Html("),
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
    let before = fs::read_to_string(dir.join("hello.lintel.php")).expect("demo exists");
    let problem = usage_error(&["init", dir.to_str().expect("utf8 path")]);
    assert!(problem.contains("refusing to overwrite"), "{problem}");
    let after = fs::read_to_string(dir.join("hello.lintel.php")).expect("demo survives");
    assert_eq!(before, after, "the existing demo is untouched");
}

#[test]
fn init_creates_missing_directories() {
    let dir = scratch("nested");
    let nested = dir.join("site").join("pages");
    let (code, _, stderr) = run_cli(&["init", nested.to_str().expect("utf8 path")]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(nested.join("hello.lintel.php").exists());
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
