//! The `lintel` command-line engine (architecture doc §6.7, task-card T-10
//! subset): `lintel build` compiles `.lintel.php` sources to plain,
//! vendor-free PHP; `lintel check` compiles diagnostics-only for CI gates.
//!
//! The engine lives in the library target so integration tests (and
//! embedders) drive the exact code path `src/main.rs` runs: argument
//! parsing returns [`CliError`] usage errors, diagnostics render through
//! [`lintel_diag::Diagnostic::render`], and every sink is an injected
//! [`std::io::Write`] instead of hard-coded stdout/stderr.
//!
//! # Errors
//!
//! [`run`] reports two failure shapes, both non-panicking: argument errors
//! come back as `Err`([`CliError`]) for the caller to print with the usage
//! text, and per-file failures (compile errors, unreadable sources) are
//! rendered to the error sink while the remaining files still compile —
//! the exit code reports whether *any* file failed.

use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use lintel_core::{compile, Options, Target};

/// The usage text `lintel --help` prints and usage errors point at.
pub const USAGE: &str = "\
lintel — the JSX-for-PHP compiler CLI (architecture doc §6.7 subset)

USAGE:
    lintel build <paths>... [--target=string|stream|rfc] [--out DIR | --in-place]
    lintel check <paths>... [--target=string|stream|rfc]
    lintel --help | --version

ARGS:
    <paths>...    .lintel.php files or directories (walked recursively, sorted)

COMMANDS:
    build    compile sources to plain PHP: stdout by default, --out or --in-place
    check    compile diagnostics-only; exit 1 when any error-severity diagnostic fires

FLAGS:
    --target=<t>    lowering target: string (default), stream or rfc
    --out <dir>     write each source as <dir>/<name>.php instead of stdout
    --in-place      write each source as <name>.php next to it
    --help, -h      print this help
    --version       print the version line

EXIT CODES:
    0    every file compiled cleanly (warnings are advisory)
    1    at least one file failed (rendered diagnostics on stderr)
    2    usage error (bad flag, missing path, conflicting output modes)";

/// A usage error: the message is printed by `main` ahead of [`USAGE`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliError(pub String);

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CliError {}

impl From<std::io::Error> for CliError {
    fn from(e: std::io::Error) -> Self {
        Self(format!("I/O error: {e}"))
    }
}

/// Where `build` writes the compiled PHP of one source.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Destination {
    /// Concatenated on the injected stdout sink.
    Stdout,
    /// `<dir>/<name>.php` per source (`Page.lintel.php` → `<dir>/Page.php`).
    Dir(PathBuf),
    /// `<name>.php` next to each source.
    InPlace,
}

/// One parsed invocation: a command, its paths and its flags.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Invocation {
    Build {
        paths: Vec<PathBuf>,
        target: Target,
        destination: Destination,
    },
    Check {
        paths: Vec<PathBuf>,
        target: Target,
    },
    Help,
    Version,
}

/// Run one CLI invocation: parse `args` (without the program name), print
/// compiled PHP / rendered diagnostics to the injected sinks, and return
/// the process exit code ([`USAGE`] lists the codes).
///
/// # Errors
///
/// Returns [`CliError`] when the arguments are malformed — unknown flags,
/// a missing subcommand or path, an unknown lowering target, conflicting
/// output modes, or a path that does not exist. The caller prints the
/// message plus [`USAGE`]; nothing is written to the sinks in that case.
pub fn run(
    args: &[String],
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<i32, CliError> {
    match parse(args)? {
        Invocation::Help => {
            stdout.write_all(USAGE.as_bytes())?;
            Ok(0)
        }
        Invocation::Version => {
            let _ = writeln!(stdout, "lintel {}", env!("CARGO_PKG_VERSION"));
            Ok(0)
        }
        Invocation::Build {
            paths,
            target,
            destination,
        } => {
            let sources = collect_sources(&paths)?;
            Ok(build(&sources, target, &destination, stdout, stderr))
        }
        Invocation::Check { paths, target } => {
            let sources = collect_sources(&paths)?;
            Ok(check(&sources, target, stderr))
        }
    }
}

/// Parse `args` (the program name is already stripped) into an
/// [`Invocation`]. Flags are order-free and must precede nothing — paths
/// and flags interleave freely — but `--in-place`/`--out` are `build`-only.
fn parse(args: &[String]) -> Result<Invocation, CliError> {
    let Some(command) = args.first() else {
        return Err(CliError("missing command: build or check".into()));
    };
    match command.as_str() {
        "--help" | "-h" => return Ok(Invocation::Help),
        "--version" => return Ok(Invocation::Version),
        "build" | "check" => {}
        other => {
            return Err(CliError(format!(
                "unknown command `{other}`: expected build or check"
            )))
        }
    }
    let is_build = command == "build";

    let mut paths: Vec<PathBuf> = vec![];
    let mut target = Target::default();
    let mut destination = Destination::Stdout;
    let mut out_seen = false;
    let mut in_place_seen = false;
    let mut index = 1;
    while index < args.len() {
        let arg = args[index].as_str();
        index += 1;
        match arg {
            "--help" | "-h" => return Ok(Invocation::Help),
            "--version" => return Ok(Invocation::Version),
            "--in-place" if is_build => {
                in_place_seen = true;
                destination = Destination::InPlace;
            }
            "--in-place" => return Err(CliError("--in-place is a `build` flag".into())),
            s if s.starts_with("--target=") => {
                target = parse_target(&s["--target=".len()..])?;
            }
            "--target" => {
                let Some(value) = args.get(index) else {
                    return Err(CliError(
                        "--target needs a value: string, stream or rfc".into(),
                    ));
                };
                index += 1;
                target = parse_target(value)?;
            }
            s if s.starts_with("--out=") => {
                out_seen = true;
                destination = Destination::Dir(PathBuf::from(&s["--out=".len()..]));
            }
            "--out" if is_build => {
                let Some(value) = args.get(index) else {
                    return Err(CliError("--out needs a directory argument".into()));
                };
                index += 1;
                out_seen = true;
                destination = Destination::Dir(PathBuf::from(value));
            }
            "--out" => return Err(CliError("--out is a `build` flag".into())),
            s if s.starts_with('-') => {
                return Err(CliError(format!("unknown flag `{s}`: see `lintel --help`")))
            }
            path => paths.push(PathBuf::from(path)),
        }
    }
    if paths.is_empty() {
        return Err(CliError(format!(
            "`lintel {command}` needs at least one path"
        )));
    }
    if in_place_seen && out_seen {
        return Err(CliError(
            "--out and --in-place are mutually exclusive".into(),
        ));
    }
    for path in &paths {
        if !path.exists() {
            return Err(CliError(format!("no such path: {}", path.display())));
        }
    }
    if is_build {
        Ok(Invocation::Build {
            paths,
            target,
            destination,
        })
    } else {
        Ok(Invocation::Check { paths, target })
    }
}

/// The lowering target named by a `--target` value (doc 5.3).
fn parse_target(value: &str) -> Result<Target, CliError> {
    match value {
        "string" => Ok(Target::String),
        "stream" => Ok(Target::Stream),
        "rfc" => Ok(Target::Rfc),
        other => Err(CliError(format!(
            "unknown target `{other}`: expected string, stream or rfc"
        ))),
    }
}

/// Expand path arguments into the list of `.lintel.php` sources to
/// compile, **sorted by full path** for deterministic output: a file
/// argument is taken as-is, a directory argument is walked recursively
/// collecting `*.lintel.php` files. The existence check happened at parse
/// time, so only I/O can fail here.
///
/// # Errors
///
/// Propagates filesystem errors from the directory walk (unreadable
/// directories).
fn collect_sources(paths: &[PathBuf]) -> Result<Vec<PathBuf>, CliError> {
    let mut sources: Vec<PathBuf> = vec![];
    for path in paths {
        if path.is_file() {
            sources.push(path.clone());
            continue;
        }
        walk_dir(path, &mut sources)?;
    }
    sources.sort();
    Ok(sources)
}

/// Recursive directory walk body for [`collect_sources`]; entries are
/// visited sorted, and the final list is globally sorted by the caller.
///
/// # Errors
///
/// Propagates filesystem errors from `read_dir`.
fn walk_dir(dir: &Path, sources: &mut Vec<PathBuf>) -> Result<(), CliError> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| CliError(format!("cannot read directory {}: {e}", dir.display())))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    for entry in entries {
        if entry.is_dir() {
            walk_dir(&entry, sources)?;
        } else if entry
            .file_name()
            .is_some_and(|name| name.to_string_lossy().ends_with(".lintel.php"))
        {
            sources.push(entry);
        }
    }
    Ok(())
}

/// Compile every source with `build` semantics: PHP goes to the
/// destination (stdout, `--out` directory, or `--in-place` sibling), a
/// failed file writes nothing and is rendered to `stderr`, and warnings
/// render without failing the build. Returns 0 when every file compiled,
/// 1 when at least one did not. (The return is a plain code: per-file
/// failures are reported in-band, never propagated as `Err`.)
fn build(
    sources: &[PathBuf],
    target: Target,
    destination: &Destination,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let mut failed = false;
    for source in sources {
        let src = match fs::read_to_string(source) {
            Ok(src) => src,
            Err(e) => {
                let _ = writeln!(stderr, "lintel: cannot read {}: {e}", source.display());
                failed = true;
                continue;
            }
        };
        match compile(&src, &Options { target }) {
            Ok(out) => {
                for warning in &out.warnings {
                    let _ = write!(stderr, "{}", warning.render(&src));
                }
                if let Err(e) = write_output(source, &out.php, destination, stdout) {
                    let _ = writeln!(stderr, "lintel: cannot write {}: {e}", source.display());
                    failed = true;
                }
            }
            Err(problems) => {
                for problem in &problems {
                    let _ = write!(stderr, "{}", problem.render(&src));
                }
                failed = true;
            }
        }
    }
    i32::from(failed)
}

/// Write one compiled file to its [`Destination`].
///
/// # Errors
///
/// Propagates filesystem errors (`create_dir_all`, `write`) and stdout
/// write errors.
fn write_output(
    source: &Path,
    php: &str,
    destination: &Destination,
    stdout: &mut dyn Write,
) -> Result<(), CliError> {
    match destination {
        Destination::Stdout => {
            stdout.write_all(php.as_bytes())?;
            Ok(())
        }
        Destination::Dir(dir) => {
            fs::create_dir_all(dir)
                .map_err(|e| CliError(format!("cannot create directory {}: {e}", dir.display())))?;
            let path = dir.join(output_name(source));
            fs::write(&path, php)
                .map_err(|e| CliError(format!("cannot write {}: {e}", path.display())))
        }
        Destination::InPlace => {
            let parent = source.parent().unwrap_or(Path::new("."));
            let path = parent.join(output_name(source));
            fs::write(&path, php)
                .map_err(|e| CliError(format!("cannot write {}: {e}", path.display())))
        }
    }
}

/// Compile every source with `check` semantics: diagnostics render to
/// `stderr` (both severities), nothing is written, and the exit code fails
/// only on error-severity diagnostics — a warning-severity file passes the
/// CI gate (doc 4.4).
fn check(sources: &[PathBuf], target: Target, stderr: &mut dyn Write) -> i32 {
    let mut failed = false;
    for source in sources {
        let src = match fs::read_to_string(source) {
            Ok(src) => src,
            Err(e) => {
                let _ = writeln!(stderr, "lintel: cannot read {}: {e}", source.display());
                failed = true;
                continue;
            }
        };
        match compile(&src, &Options { target }) {
            Ok(out) => {
                for warning in &out.warnings {
                    let _ = write!(stderr, "{}", warning.render(&src));
                }
            }
            Err(problems) => {
                for problem in &problems {
                    let _ = write!(stderr, "{}", problem.render(&src));
                }
                failed = true;
            }
        }
    }
    i32::from(failed)
}

/// The output file name for one source: `Page.lintel.php` → `Page.php`; a
/// source without the `.lintel.php` suffix keeps its name and gains
/// `.compiled.php`, so a build can never overwrite its own input.
fn output_name(source: &Path) -> String {
    let name = source.file_name().map_or_else(
        || source.to_string_lossy().into_owned(),
        |n| n.to_string_lossy().into_owned(),
    );
    match name.strip_suffix(".lintel.php") {
        Some(stem) => format!("{stem}.php"),
        None => format!("{name}.compiled.php"),
    }
}
