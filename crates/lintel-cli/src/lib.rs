//! The `lintel` command-line engine (architecture doc §6.7, task-card T-10
//! subset): `lintel build` compiles `.lintel.php` sources to plain,
//! vendor-free PHP; `lintel check` compiles diagnostics-only for CI gates.
//!
//! The engine lives in the library target so integration tests (and
//! embedders) drive the exact code path `src/main.rs` runs: argument
//! parsing returns [`CliError`] usage errors, diagnostics render through
//! [`lintel_diag::Diagnostic::render`] — or, with `--format=json`, as one
//! [`lintel_diag::Diagnostic::to_json`] object per line for CI/LSP-style
//! consumers — and every sink is an injected [`std::io::Write`] instead of
//! hard-coded stdout/stderr.
//!
//! # Errors
//!
//! [`run`] reports two failure shapes, both non-panicking: argument errors
//! come back as `Err`([`CliError`]) for the caller to print with the usage
//! text, and per-file failures (compile errors, unreadable sources) are
//! rendered to the error sink while the remaining files still compile —
//! the exit code reports whether *any* file failed.

use std::fmt;
use std::fmt::Write as _;
use std::fs;
use std::io::Write;
use std::ops::ControlFlow;
use std::path::{absolute, Path, PathBuf};

use lintel_core::{
    builtin_rule_metas, collect_inventory, compile, compile_analyzed, ChildHint, ChildKind,
    Diagnostic, Options, Target, TypeHints,
};

mod cache;
mod manifest;
pub mod watch;

/// The usage text `lintel --help` prints and usage errors point at.
pub const USAGE: &str = "\
lintel — the JSX-for-PHP compiler CLI (architecture doc §6.7 subset)

USAGE:
    lintel build <paths>... [--target=string|stream|rfc] [--format=text|json] [--out DIR | --in-place] [--manifest [PATH]]
    lintel check <paths>... [--target=string|stream|rfc] [--format=text|json] [--types PATH]
    lintel analyze <paths>... [--types PATH] [--target=string|stream|rfc] [--format=text|json]
    lintel stats <paths>... [--format=text|json]
    lintel rules [--format=text|json]
    lintel init [DIR] [--demo N]
    lintel migrate <paths>... [--to=native] [--format=text|json] [--dry-run] [--backup]
    lintel watch [paths]... [--socket PATH] [--foreground]
    lintel --help | --version

ARGS:
    <paths>...    .lintel.php files or directories (walked recursively, sorted)
    [DIR]         where `init` scaffolds the demo (default: the working directory)

COMMANDS:
    build    compile sources to plain PHP: stdout by default, --out or --in-place;
             with --manifest also write the T-31 `lintel.json` function-component
             manifest (see MANIFEST below)
    check    compile diagnostics-only; exit 1 when any error-severity diagnostic fires
    analyze  the §7.3-Y2 pass: compile with the PHPStan bridge's proven child
             types driving escape elision (stdout like build) and report the
             elision counters on stderr (see ANALYZE REPORT below)
    stats    report each file's component shape (declared + invoked) without
             emitting PHP; exit 1 only when a file fails to parse
    rules    list the built-in validation rules — one `<id> <LNTxxxx codes>
             <summary>` line per rule in registry order, or one JSON object
             per rule with --format=json (docs/rule-registry.md); takes no
             paths, exit 0
    init     scaffold a runnable demo: hello.lintel.php source + compiled hello.php;
             with --demo N (1-5) scaffold one of the five T-15 demos instead
    migrate  rewrite `.lintel.php` sources IN PLACE to the native RFC-parity
             lowering (docs/rfc-alignment.md): each source compiles with
             Target::Rfc and its JSX becomes `new \\Markup\\Element(...)` calls,
             line-preserved, so the migrated file is the same PHP file the
             future native runtime consumes. `--dry-run` prints a unified
             diff per file and writes nothing, each hunk header carrying the
             function-level context (`@@ -10,5 +10,5 @@ function Badge(string
             $name): Html`) so a review sees which component is being
             rewritten; `--backup` keeps the original
             bytes beside the rewrite as `<file>.bak`. A file that fails to
             compile, or whose component invocations none of the migrated
             sources declares, is refused untouched (exit 1); `--to=native`
             is accepted and is the only value today

FLAGS:
    --target=<t>    lowering target for build/check: string (default), stream
                    or rfc (accepted on stats for pipeline parity; the shape
                    report is target-independent)
    --format=<f>    diagnostic/report format: text (default) or json (one JSON
                    object per diagnostic or report on the sink; exit codes
                    unchanged)
    --out <dir>     write each source as <dir>/<name>.php instead of stdout
    --in-place      write each source as <name>.php next to it
    --no-cache      bypass the `.lintel-cache/` build cache: every source
                    recompiles and nothing is stored (CI or debugging);
                    `check`, `analyze`, `migrate` and `stats` never touch
                    the cache
    --types PATH    the analyze report file (analyze compiles against it;
                    check renders its stale entries as advisories). Without
                    it analyze reads each source's own
                    <source>.lintel-types.json sidecar, an absent one
                    compiling without hints
    --manifest [PATH]
                    after building, scan the compiled sources for top-level
                    `function Name(...): Html` declarations and write the
                    T-31 function-component manifest; `--manifest=PATH`
                    writes PATH, bare `--manifest` writes `lintel.json` in
                    the workspace root — the common ancestor of the build
                    inputs and `--out` — never the process cwd, so the
                    manifest lands with the sources it describes from any
                    invocation directory. A bare `--manifest` never consumes
                    the next token — paths and flags interleave freely, so
                    use the `=` form to name a path. `build` only — `migrate`
                    emits RFC-parity artifacts no autoloader can require
    --help, -h      print this help
    --version       print the version line

MANIFEST (T-31):
    `lintel build --manifest` writes/updates the project's `lintel.json`,
    preserving every existing key (the user's `functions` globs) and
    setting two: `components` maps each declared component name to its
    source path relative to the manifest, and `prod.artifacts` maps each
    compiled source to its artifact (only when the build wrote artifacts
    via --out/--in-place). Function-only components are not PSR-4
    autoloadable: the Composer plugin's prod shim requires the artifacts
    and the dev loader (§6.10) requires the sources eagerly — skipping
    whichever source the shim already loaded, so the two lanes never
    redeclare a function.

CACHE (§9.3 warm-build target):
    `lintel build` caches compiled PHP under `.lintel-cache/` in the
    workspace root (the common ancestor of the build inputs and `--out`):
    one `<content-hash>.php` entry per source content + target pair, with
    a metadata sidecar (source path, target, options hash) and the cold
    build's rendered warnings, which a warm build replays verbatim. The
    key hashes the source CONTENT with the target (mtime lies after a git
    checkout; string and stream artifacts differ), so an edited source or
    a different `--target` misses and recompiles. `--no-cache` skips
    lookup and store; `check`/`analyze` always re-validate. `.lintel-cache
    /stats.json` records the last build's counters (`compiled`,
    `cache_hits`, `bypassed`).

STATS REPORT:
    One section per file: the path header, then one `declared Name(params)`
    line per component the file declares (`!` marks a required parameter,
    `...` the variadic tail) and one
    `invoke <Name> at line:col props: ...` line per component invocation
    site, followed by `; children`, `; spread` and `; undeclared` as they
    apply. --format=json emits one JSON object per file instead, a `file`
    field ahead of the core inventory's documented shape
    (docs/component-inventory.md).

ANALYZE REPORT (§7.3-Y2, docs/type-elision.md):
    `lintel analyze` compiles each source with the PHPStan bridge's proven
    child-expression types driving escape elision and reports the elision
    counters on stderr: `children` (every dynamic child lowered on the
    markup targets), `considered` (how many reached the analyze decision
    step past the v0.1 folds), `elided` (how many the proven
    int/float/numeric-string/literal types removed the `c()` wrapper for)
    and `stale` (report entries matching no child of the current source —
    `lintel check --types PATH` renders each one as an advisory line /
    JSON object).
    Text prints one summary line per file with the elision rate; JSON
    emits one object per file with a `rate` field (elided/considered,
    null when nothing was reachable). The types file is advisory: a
    missing report compiles without hints, and a malformed one fails the
    file (exit 1) — never silently.

EXIT CODES:
    0    every file compiled/scanned cleanly (warnings are advisory)
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

/// How diagnostics (errors and warnings) are rendered to the error sink
/// (doc §6.7): [`Format::Text`] draws [`Diagnostic::render`] snippets;
/// [`Format::Json`] emits one [`Diagnostic::to_json`] object per line for
/// machine-readable consumers. The exit-code contract is identical — only
/// the rendering changes.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum Format {
    /// Human-rendered snippets (default, back-compat).
    #[default]
    Text,
    /// One JSON object per diagnostic, JSON Lines style.
    Json,
}

/// The diagnostic format named by a `--format` value.
fn parse_format(value: &str) -> Result<Format, CliError> {
    match value {
        "text" => Ok(Format::Text),
        "json" => Ok(Format::Json),
        other => Err(CliError(format!(
            "unknown format `{other}`: expected text or json"
        ))),
    }
}

/// One parsed invocation: a command, its paths and its flags.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Invocation {
    Build {
        paths: Vec<PathBuf>,
        target: Target,
        format: Format,
        destination: Destination,
        /// The T-31 manifest to write after the build (`--manifest [PATH]`);
        /// `None` keeps `build` manifest-free (back-compat). The bare form
        /// resolves against the workspace being compiled at run time — see
        /// [`ManifestArg`].
        manifest: Option<ManifestArg>,
        /// The M2 `--no-cache` bypass: every source recompiles, no cache
        /// entry is read or stored (CI or debugging).
        no_cache: bool,
    },
    /// `lintel migrate <paths> [--dry-run] [--backup]`: the Year-3
    /// RFC-convergence migration — rewrite each source IN PLACE with the
    /// native RFC-parity lowering (`new \Markup\Element` calls), refusing
    /// files that fail to compile or whose components none of the migrated
    /// sources declares. `--dry-run` prints the unified diff (each hunk
    /// header carrying the enclosing function signature) and writes
    /// nothing; `--backup` keeps the original bytes as `<file>.bak`.
    Migrate {
        paths: Vec<PathBuf>,
        format: Format,
        dry_run: bool,
        backup: bool,
    },
    Check {
        paths: Vec<PathBuf>,
        target: Target,
        format: Format,
        /// The explicit analyze report (`--types`): its stale entries
        /// surface as advisories (§7.3-Y2, `docs/type-elision.md`).
        types: Option<PathBuf>,
    },
    /// `lintel analyze [paths] [--types PATH]`: the §7.3-Y2 pass
    /// (`docs/type-elision.md`) — compile each source with the `PHPStan`
    /// bridge's proven child types driving escape elision, write the PHP
    /// to stdout like `build`, and report the elision counters on stderr.
    Analyze {
        paths: Vec<PathBuf>,
        /// The explicit report file every source compiles against; `None`
        /// reads each source's own `<source>.lintel-types.json` sidecar
        /// (the bridge runner's output), absent reports compiling without
        /// hints.
        types: Option<PathBuf>,
        target: Target,
        format: Format,
    },
    /// `lintel stats [paths]`: the per-file component shape report
    /// (`docs/component-inventory.md`), rendered to stdout in the
    /// requested [`Format`]; parse diagnostics fail a file like `check`,
    /// validation lints play no part.
    Stats {
        paths: Vec<PathBuf>,
        format: Format,
    },
    /// `lintel init [DIR] [--demo N]`: scaffold the runnable demo into
    /// `path` — the hello demo by default, or T-15 demo `N` (1-5).
    Init {
        path: PathBuf,
        demo: Option<usize>,
    },
    /// `lintel rules`: the built-in validation registry's machine-readable
    /// listing (`docs/rule-registry.md`), rendered to stdout in the
    /// requested [`Format`]; compile-time knowledge, so no paths are taken
    /// and the exit code is always 0.
    Rules {
        format: Format,
    },
    /// `lintel watch [paths] [--socket PATH] [--foreground]`: the T-12
    /// daemon — a resident compiler behind a Unix socket (TCP fallback on
    /// Windows) with an mtime+size cache, invalidated by the `notify`
    /// file watcher. Without `--foreground` the command spawns itself
    /// detached and exits once the socket answers.
    Watch {
        paths: Vec<PathBuf>,
        socket: PathBuf,
        foreground: bool,
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
            // Write errors propagate like `Help`'s: a broken/injected sink
            // must fail the run instead of reporting a bogus exit 0.
            writeln!(stdout, "lintel {}", env!("CARGO_PKG_VERSION"))?;
            Ok(0)
        }
        Invocation::Build {
            paths,
            target,
            format,
            destination,
            manifest,
            no_cache,
        } => {
            // The bare `--manifest` default anchors to the workspace being
            // compiled (the M0 cwd-hygiene card): the common ancestor of
            // the build inputs and `--out` — never the process cwd, so a
            // lane invoking the engine from anywhere else cannot scatter a
            // stray `lintel.json` outside the workspace. The M2 build cache
            // lives at the same root.
            let manifest = manifest.map(|arg| match arg {
                ManifestArg::Default => workspace_root(&paths, &destination).join("lintel.json"),
                ManifestArg::Explicit(path) => path,
            });
            run_build(
                &paths,
                &BuildPlan {
                    target,
                    format,
                    destination,
                    manifest,
                    no_cache,
                },
                stdout,
                stderr,
            )
        }
        Invocation::Migrate {
            paths,
            format,
            dry_run,
            backup,
        } => run_migrate(&paths, format, dry_run, backup, stdout, stderr),
        Invocation::Check {
            paths,
            target,
            format,
            types,
        } => {
            check_types_report(types.as_deref())?;
            let sources = collect_sources(&paths)?;
            Ok(check(&sources, target, format, types.as_deref(), stderr))
        }
        Invocation::Analyze {
            paths,
            types,
            target,
            format,
        } => run_analyze(&paths, types.as_deref(), target, format, stdout, stderr),
        Invocation::Stats { paths, format } => {
            let sources = collect_sources(&paths)?;
            Ok(stats(&sources, format, stdout, stderr))
        }
        Invocation::Init { path, demo } => {
            let report = init_demo(&path, demo)?;
            writeln!(
                stdout,
                "wrote {} ({} lines of .lintel.php)\ncompiled {} — edit the source and run \
                 `lintel build {}` to rebuild",
                report.source.display(),
                report.lines,
                report.compiled.display(),
                report.source.display()
            )?;
            Ok(0)
        }
        Invocation::Rules { format } => rules(format, stdout),
        Invocation::Watch {
            paths,
            socket,
            foreground,
        } => {
            let sources = collect_sources(&paths)?;
            if foreground {
                return Ok(watch::run_foreground(&socket, &sources, stdout, stderr));
            }
            let mut foreground_args: Vec<String> = vec![
                "watch".into(),
                "--socket".into(),
                socket.display().to_string(),
                "--foreground".into(),
            ];
            foreground_args.extend(sources.iter().map(|path| path.display().to_string()));
            Ok(watch::spawn_background(
                &socket,
                &foreground_args,
                stdout,
                stderr,
            ))
        }
    }
}

/// The `build` invocation's knobs, bundled so the engine entry points stay
/// inside the pedantic argument budget (the M0 manifest resolution and the
/// M2 cache bypass joined the original four). The manifest path arrives
/// RESOLVED — the bare `--manifest` default was already anchored to the
/// workspace by the caller.
struct BuildPlan {
    target: Target,
    format: Format,
    destination: Destination,
    manifest: Option<PathBuf>,
    no_cache: bool,
}

/// The running tally one `build` carries across its sources (the T-31
/// scan, the artifact pairs, the counters the stats sidecar records).
#[derive(Default)]
struct BuildState {
    failed: bool,
    components: Vec<(String, PathBuf)>,
    artifacts: Vec<(PathBuf, PathBuf)>,
    compiled: usize,
    cache_hits: usize,
}

/// `build`: collect the sources, compile to the destination, report.
///
/// # Errors
///
/// Propagates [`collect_sources`]'s path errors.
fn run_build(
    paths: &[PathBuf],
    plan: &BuildPlan,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<i32, CliError> {
    // The M2 build cache lives at the workspace root — the same root the
    // bare `--manifest` default anchors to — so warm and cold builds land
    // the cache with the sources regardless of the process cwd. It rides
    // the artifact-producing destinations (`--out`, `--in-place`) only: a
    // stdout build (the §6.10 dev loader's compile lane, the tester) has
    // no artifacts to warm and must leave no `.lintel-cache/` beside the
    // sources it compiles. `--no-cache` keeps the stats sidecar honest
    // (`bypassed: true`) while skipping lookup and store.
    let cache = match plan.destination {
        Destination::Stdout => None,
        _ => Some(cache::BuildCache::at(&workspace_root(
            paths,
            &plan.destination,
        ))),
    };
    Ok(build(
        &collect_sources(paths)?,
        plan,
        cache.as_ref(),
        stdout,
        stderr,
    ))
}

/// `migrate`: collect the sources, rewrite in place, report.
///
/// # Errors
///
/// Propagates [`collect_sources`]'s path errors.
fn run_migrate(
    paths: &[PathBuf],
    format: Format,
    dry_run: bool,
    backup: bool,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<i32, CliError> {
    let sources = collect_sources(paths)?;
    Ok(migrate(&sources, format, dry_run, backup, stdout, stderr))
}

/// `check`'s `--types` pre-check: an explicit report must exist (analyze
/// reads sidecars; check demands the file).
///
/// # Errors
///
/// A `--types` path that does not exist returns [`CliError`].
fn check_types_report(types: Option<&Path>) -> Result<(), CliError> {
    if let Some(report) = types {
        if !report.is_file() {
            return Err(CliError(format!(
                "cannot read {}: no such file (--types names one analyze report file)",
                report.display()
            )));
        }
    }
    Ok(())
}

/// The `--manifest` flag value (the M0 cwd-hygiene card): the bare form
/// anchors the T-31 default to the workspace being compiled — the manifest
/// lands with the sources it describes, never wherever the process happens
/// to run — while an explicit `--manifest=PATH` writes exactly the path
/// given, like every other explicit path argument.
#[derive(Clone, PartialEq, Eq, Debug)]
enum ManifestArg {
    /// Bare `--manifest`: `lintel.json` in the workspace root (the common
    /// ancestor of the build inputs and `--out`).
    Default,
    /// `--manifest=PATH`: the path exactly as given (cwd-relative like the
    /// build inputs).
    Explicit(PathBuf),
}

/// Flags and paths collected after the command word (see [`parse_args`]).
struct ParsedArgs {
    paths: Vec<PathBuf>,
    target: Target,
    format: Format,
    destination: Destination,
    manifest: Option<ManifestArg>,
    /// The M2 `--no-cache` bypass: build compiles every source and reads
    /// no cache entry (CI or debugging).
    no_cache: bool,
    /// The `check`-only analyze report (`--types`): when present, check
    /// renders the report's stale entries as advisories (§7.3-Y2,
    /// `docs/type-elision.md`).
    types: Option<PathBuf>,
    out_seen: bool,
    in_place_seen: bool,
    /// The `migrate`-only flags.
    migrate: MigrateFlags,
}

/// The `migrate`-only flags: `--dry-run` prints the unified diff and
/// writes nothing; `--backup` keeps the original bytes as `<file>.bak`
/// beside the rewrite.
struct MigrateFlags {
    dry_run: bool,
    backup: bool,
}

/// The `check`-only analyze report flag (§7.3-Y2): `build` rejects it.
///
/// # Errors
///
/// Under `build` the flag is a contract mismatch.
fn types_flag(value: String, is_build: bool) -> Result<Option<PathBuf>, CliError> {
    if is_build {
        return Err(CliError(
            "--types is a `check` flag: build takes no analyze report".into(),
        ));
    }
    Ok(Some(PathBuf::from(value)))
}

/// The `migrate` `--to` value: only `native` exists today (T-19), so any
/// other value is a contract mismatch naming the only supported one.
///
/// # Errors
///
/// Any value but `native` returns [`CliError`].
fn migrate_to(value: &str) -> Result<(), CliError> {
    if value != "native" {
        return Err(CliError(format!(
            "`migrate` only supports `--to=native` today (got `--to={value}`): \
             the native RFC lowering is the migration target \
             (docs/rfc-alignment.md)"
        )));
    }
    Ok(())
}

/// Consume the flags and paths following the command word. Flags and paths
/// interleave freely, but `--in-place`/`--out` are `build`-only. Breaks
/// with a short-circuiting `--help`/`--version` [`Invocation`].
///
/// # Errors
///
/// Returns [`CliError`] for unknown flags, missing flag values, unknown
/// targets/formats, or a `build`-only flag under `check`.
fn parse_args(
    args: &[String],
    is_build: bool,
    is_migrate: bool,
) -> Result<ControlFlow<Invocation, ParsedArgs>, CliError> {
    let mut parsed = ParsedArgs {
        paths: vec![],
        // `migrate` compiles with the native RFC lowering unless --to=native
        // (accepted) says the same thing; --target is rejected below.
        target: if is_migrate {
            Target::Rfc
        } else {
            Target::default()
        },
        format: Format::default(),
        destination: Destination::Stdout,
        manifest: None,
        no_cache: false,
        types: None,
        out_seen: false,
        in_place_seen: false,
        migrate: MigrateFlags {
            dry_run: false,
            backup: false,
        },
    };
    let mut index = 1;
    while index < args.len() {
        let arg = args[index].as_str();
        index += 1;
        match arg {
            "--help" | "-h" => return Ok(ControlFlow::Break(Invocation::Help)),
            "--version" => return Ok(ControlFlow::Break(Invocation::Version)),
            s if s.starts_with("--target=") && is_migrate => {
                return Err(CliError(MIGRATE_TARGET_ONLY.into()));
            }
            "--target" if is_migrate => return Err(CliError(MIGRATE_TARGET_ONLY.into())),
            s if s.starts_with("--target=") => {
                parsed.target = parse_target(&s["--target=".len()..])?;
            }
            "--target" => {
                let value = value_flag(
                    args,
                    &mut index,
                    "--target needs a value: string, stream or rfc",
                )?;
                parsed.target = parse_target(&value)?;
            }
            "--to" if is_migrate => {
                let value = value_flag(args, &mut index, "--to needs a value: native")?;
                migrate_to(&value)?;
                parsed.target = Target::Rfc;
            }
            s if s.starts_with("--to=") && is_migrate => {
                migrate_to(&s["--to=".len()..])?;
                parsed.target = Target::Rfc;
            }
            s if s.starts_with("--to=") => {
                return Err(CliError("--to is a `migrate` flag (values: native)".into()));
            }
            "--to" => return Err(CliError("--to is a `migrate` flag (values: native)".into())),
            "--dry-run" if is_migrate => {
                parsed.migrate.dry_run = true;
            }
            "--dry-run" => return Err(CliError("--dry-run is a `migrate` flag".into())),
            "--backup" if is_migrate => {
                parsed.migrate.backup = true;
            }
            "--backup" => return Err(CliError("--backup is a `migrate` flag".into())),
            "--types" => {
                let value = value_flag(args, &mut index, "--types needs a value: the report file")?;
                parsed.types = types_flag(value, is_build)?;
            }
            s if s.starts_with("--types=") => {
                parsed.types = types_flag(s["--types=".len()..].to_string(), is_build)?;
            }
            s if s.starts_with("--format=") => {
                parsed.format = parse_format(&s["--format=".len()..])?;
            }
            "--format" => {
                let value = value_flag(args, &mut index, "--format needs a value: text or json")?;
                parsed.format = parse_format(&value)?;
            }
            s if s.starts_with('-') => {
                match build_flag(arg, args, &mut index, &mut parsed, is_build, is_migrate) {
                    Ok(Some(())) => {}
                    Ok(None) => {
                        return Err(CliError(format!(
                            "unknown flag `{arg}`: see `lintel --help`"
                        )))
                    }
                    Err(problem) => return Err(problem),
                }
            }
            path => parsed.paths.push(PathBuf::from(path)),
        }
    }
    Ok(ControlFlow::Continue(parsed))
}

/// The `migrate` target error: migrate compiles with the native RFC
/// lowering; a per-invocation target choice has nothing to annotate.
const MIGRATE_TARGET_ONLY: &str = "`migrate` compiles with the native RFC lowering; use \
     `lintel build --target=…` to pick a target";

/// The `build`-only output flags — `--in-place`, `--out`, `--manifest`.
/// Returns `Ok(None)` when `arg` is not one of them (the caller falls
/// through to the unknown-flag error), `Ok(Some(()))` when it consumed
/// the flag.
///
/// # Errors
///
/// A build-only flag under `check`/`stats`, or a malformed value, returns
/// [`CliError`].
fn build_flag(
    arg: &str,
    args: &[String],
    index: &mut usize,
    parsed: &mut ParsedArgs,
    is_build: bool,
    is_migrate: bool,
) -> Result<Option<()>, CliError> {
    // A build-only flag under `check`/`stats` keeps its specific error (it
    // names the flag and the command that owns it) instead of falling
    // through to the generic unknown-flag message.
    if let Some(name) = arg
        .strip_prefix("--out=")
        .map(|_| "--out")
        .or((arg == "--out").then_some("--out"))
        .or((arg == "--in-place").then_some("--in-place"))
    {
        // `migrate` rewrites the source files themselves — an artifact
        // destination contradicts the rewrite.
        if !is_build || is_migrate {
            return Err(CliError(format!(
                "{name} is a `build` flag (migrate rewrites the source files in place)"
            )));
        }
    } else if arg == "--manifest" || arg.starts_with("--manifest=") {
        // `migrate` shares `build`'s output flags but never takes a
        // manifest: its RFC-parity artifacts are lowered for the future
        // native runtime, not for a `files` autoloader.
        if !is_build || is_migrate {
            return Err(CliError(
                "--manifest is a `build` flag (migrate emits RFC-parity artifacts no \
                 autoloader can require)"
                    .into(),
            ));
        }
    } else if arg == "--no-cache" {
        // The M2 cache bypass is a `build` concern: check/analyze must
        // always re-validate (a cached acceptance is no acceptance),
        // migrate rewrites in place, stats reads nothing.
        if !is_build || is_migrate {
            return Err(CliError(
                "--no-cache is a `build` flag (check, analyze, migrate and stats never \
                 touch the build cache)"
                    .into(),
            ));
        }
    } else {
        return Ok(None);
    }
    if let Some(value) = arg.strip_prefix("--out=") {
        parsed.out_seen = true;
        parsed.destination = Destination::Dir(PathBuf::from(value));
        return Ok(Some(()));
    }
    if arg == "--out" {
        let value = value_flag(args, index, "--out needs a directory argument")?;
        parsed.out_seen = true;
        parsed.destination = Destination::Dir(PathBuf::from(value));
        return Ok(Some(()));
    }
    if arg == "--in-place" {
        parsed.in_place_seen = true;
        parsed.destination = Destination::InPlace;
        return Ok(Some(()));
    }
    if let Some(value) = arg.strip_prefix("--manifest=") {
        parsed.manifest = Some(ManifestArg::Explicit(PathBuf::from(value)));
        return Ok(Some(()));
    }
    if arg == "--manifest" {
        // Bare `--manifest` writes the default `lintel.json` and never
        // consumes the next token — paths and flags interleave freely, so
        // a following path is a build input, not a value. The default
        // resolves against the workspace being compiled, not the process
        // cwd (`ManifestArg::Default` → `workspace_root` at run time).
        parsed.manifest = Some(ManifestArg::Default);
        return Ok(Some(()));
    }
    if arg == "--no-cache" {
        parsed.no_cache = true;
        return Ok(Some(()));
    }
    Ok(None)
}

/// Consume the space form of a value-taking flag: `--flag VALUE`. The
/// inline `--flag=VALUE` form is handled by the caller's prefix match; the
/// next token is always the value (paths and flags interleave freely, so a
/// flag's value is unambiguous only in the `=` form — the space form is
/// reserved for flags whose value is mandatory).
///
/// # Errors
///
/// A missing value returns [`CliError`] with the caller's exact message.
fn value_flag(args: &[String], index: &mut usize, missing: &str) -> Result<String, CliError> {
    let Some(value) = args.get(*index) else {
        return Err(CliError(missing.into()));
    };
    *index += 1;
    Ok(value.clone())
}

/// Parse `args` (the program name is already stripped) into an
/// [`Invocation`]. Flags are order-free and must precede nothing — paths
/// and flags interleave freely — but `--in-place`/`--out` are `build`-only.
fn parse(args: &[String]) -> Result<Invocation, CliError> {
    let Some(command) = args.first() else {
        return Err(CliError(
            "missing command: build, check, analyze, stats, rules, init, watch or migrate".into(),
        ));
    };
    match command.as_str() {
        "--help" | "-h" => return Ok(Invocation::Help),
        "--version" => return Ok(Invocation::Version),
        "build" | "check" | "stats" | "migrate" => {}
        "analyze" => {
            let args = match parse_analyze(&args[1..])? {
                ControlFlow::Break(invocation) => return Ok(invocation),
                ControlFlow::Continue(parsed) => parsed,
            };
            return Ok(Invocation::Analyze {
                paths: args.paths,
                types: args.types,
                target: args.target,
                format: args.format,
            });
        }
        "rules" => {
            let format = match parse_rules(&args[1..])? {
                ControlFlow::Break(invocation) => return Ok(invocation),
                ControlFlow::Continue(format) => format,
            };
            return Ok(Invocation::Rules { format });
        }
        "init" => {
            let (path, demo) = parse_init(&args[1..])?;
            return Ok(Invocation::Init { path, demo });
        }
        "watch" => {
            let (paths, socket, foreground) = match parse_watch(&args[1..])? {
                ControlFlow::Break(invocation) => return Ok(invocation),
                ControlFlow::Continue(parsed) => parsed,
            };
            return Ok(Invocation::Watch {
                paths,
                socket,
                foreground,
            });
        }
        other => {
            return Err(CliError(format!(
                "unknown command `{other}`: expected build, check, stats, rules, init, watch \
                 or migrate"
            )))
        }
    }
    let is_build = command == "build" || command == "migrate";
    let is_migrate = command == "migrate";
    let parsed = match parse_args(args, is_build, is_migrate)? {
        ControlFlow::Break(invocation) => return Ok(invocation),
        ControlFlow::Continue(parsed) => parsed,
    };
    if parsed.paths.is_empty() {
        return Err(CliError(format!(
            "`lintel {command}` needs at least one path"
        )));
    }
    if parsed.in_place_seen && parsed.out_seen {
        return Err(CliError(
            "--out and --in-place are mutually exclusive".into(),
        ));
    }
    for path in &parsed.paths {
        if !path.exists() {
            return Err(CliError(format!("no such path: {}", path.display())));
        }
    }
    match command.as_str() {
        "build" => Ok(Invocation::Build {
            paths: parsed.paths,
            target: parsed.target,
            format: parsed.format,
            destination: parsed.destination,
            manifest: parsed.manifest,
            no_cache: parsed.no_cache,
        }),
        "migrate" => Ok(Invocation::Migrate {
            paths: parsed.paths,
            format: parsed.format,
            dry_run: parsed.migrate.dry_run,
            backup: parsed.migrate.backup,
        }),
        "check" => Ok(Invocation::Check {
            paths: parsed.paths,
            target: parsed.target,
            format: parsed.format,
            types: parsed.types,
        }),
        _ => Ok(Invocation::Stats {
            paths: parsed.paths,
            format: parsed.format,
        }),
    }
}

/// Parse the `rules` invocation: the registry listing takes no paths and
/// only the shared `--format` flag (plus the global `--help`/`--version`
/// short-circuits) — the registry is compile-time knowledge, so path,
/// target and output flags have nothing to annotate
/// (`docs/rule-registry.md`).
fn parse_rules(args: &[String]) -> Result<ControlFlow<Invocation, Format>, CliError> {
    let mut format = Format::default();
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        index += 1;
        match arg {
            "--help" | "-h" => return Ok(ControlFlow::Break(Invocation::Help)),
            "--version" => return Ok(ControlFlow::Break(Invocation::Version)),
            s if s.starts_with("--format=") => {
                format = parse_format(&s["--format=".len()..])?;
            }
            "--format" => {
                let Some(value) = args.get(index) else {
                    return Err(CliError("--format needs a value: text or json".into()));
                };
                index += 1;
                format = parse_format(value)?;
            }
            s if s.starts_with('-') => {
                return Err(CliError(format!("unknown flag `{s}`: see `lintel --help`")))
            }
            path => {
                return Err(CliError(format!(
                    "`lintel rules` takes no paths (the registry is compile-time knowledge): \
                     got `{path}`"
                )))
            }
        }
    }
    Ok(ControlFlow::Continue(format))
}

/// Parse the `init` invocation: `lintel init [DIR] [--demo N]` takes the
/// `--demo` selector (1-based, 1-5) and at most one directory argument
/// (default: the working directory). The subcommand scaffolds files and
/// compiles them once, so lowering-target and diagnostic-format flags have
/// nothing to annotate and output flags would collide with the files it
/// writes.
fn parse_init(args: &[String]) -> Result<(PathBuf, Option<usize>), CliError> {
    let mut path: Option<PathBuf> = None;
    let mut demo: Option<usize> = None;
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        index += 1;
        let value = if arg == "--demo" {
            match args.get(index) {
                Some(value) => {
                    index += 1;
                    value.as_str()
                }
                None => {
                    return Err(CliError(
                        "--demo needs a value: the demo number 1-5 (see `lintel --help`)".into(),
                    ))
                }
            }
        } else if let Some(value) = arg.strip_prefix("--demo=") {
            value
        } else if arg.starts_with('-') {
            return Err(CliError(format!(
                "`lintel init` takes only --demo (got `{arg}`): it scaffolds a runnable \
                 demo with the default target — see `lintel --help`"
            )));
        } else {
            if path.replace(PathBuf::from(arg)).is_some() {
                return Err(CliError(
                    "`lintel init` takes at most one directory argument".into(),
                ));
            }
            continue;
        };
        let number: usize = value.parse().map_err(|_| {
            CliError(format!(
                "`--demo` needs a number (got `{value}`): the demo number 1-5"
            ))
        })?;
        if number == 0 || number > DEMO_SOURCES.len() {
            return Err(CliError(format!(
                "unknown demo `{number}`: --demo takes 1-{} (component-library, \
                 xss-gallery, table-10k, htmx-todo, error-showcase)",
                DEMO_SOURCES.len()
            )));
        }
        if demo.replace(number).is_some() {
            return Err(CliError("--demo was given twice".into()));
        }
    }
    Ok((path.unwrap_or_else(|| PathBuf::from(".")), demo))
}

/// Parse the `watch` invocation: `lintel watch [paths]... [--socket PATH]
/// [--foreground]`. Paths default to the working directory (the daemon
/// watches it recursively); `--socket` names the socket path (default:
/// the per-working-directory socket under the system temp dir, see
/// [`watch::default_socket`]); `--foreground` runs the daemon in-process
/// instead of spawning a detached background process.
/// What `watch` parsed: the paths, the socket and the foreground bit.
type WatchArgs = (Vec<PathBuf>, PathBuf, bool);

fn parse_watch(args: &[String]) -> Result<ControlFlow<Invocation, WatchArgs>, CliError> {
    let mut paths: Vec<PathBuf> = vec![];
    let mut socket: Option<PathBuf> = None;
    let mut foreground = false;
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        index += 1;
        match arg {
            "--help" | "-h" => return Ok(ControlFlow::Break(Invocation::Help)),
            "--version" => return Ok(ControlFlow::Break(Invocation::Version)),
            s if s.starts_with("--socket=") => {
                socket = Some(PathBuf::from(&s["--socket=".len()..]));
            }
            "--socket" => {
                let value = value_flag(args, &mut index, "--socket needs a path argument")?;
                socket = Some(PathBuf::from(value));
            }
            "--foreground" => foreground = true,
            s if s.starts_with('-') => {
                return Err(CliError(format!(
                    "unknown flag `{s}`: `lintel watch` takes --socket and --foreground only"
                )))
            }
            path => paths.push(PathBuf::from(path)),
        }
    }
    for path in &paths {
        if !path.exists() {
            return Err(CliError(format!("no such path: {}", path.display())));
        }
    }
    let socket = socket.unwrap_or_else(watch::default_socket);
    if paths.is_empty() {
        paths.push(PathBuf::from("."));
    }
    Ok(ControlFlow::Continue((paths, socket, foreground)))
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

/// The workspace being compiled: the deepest common ancestor of the build
/// input paths and — when `--out` names an artifact directory — that
/// directory. The bare `--manifest` default anchors `lintel.json` here and
/// the `.lintel-cache` build cache lives here, so both land with the
/// sources they describe regardless of the process cwd (the M0
/// cwd-hygiene card). Relative inputs resolve against the process cwd
/// first (they name the workspace the way the user spelled it); the
/// resolved root is absolute, so the outputs never depend on where the
/// process runs. A file input contributes its parent directory (its
/// siblings are the workspace it compiles beside).
fn workspace_root(paths: &[PathBuf], destination: &Destination) -> PathBuf {
    let mut roots: Vec<PathBuf> = paths
        .iter()
        .map(|path| absolute(path).unwrap_or_else(|_| path.clone()))
        .collect();
    if let Destination::Dir(dir) = destination {
        roots.push(absolute(dir).unwrap_or_else(|_| dir.clone()));
    }
    roots
        .into_iter()
        .map(|root| {
            if root.is_file() {
                root.parent().map(Path::to_path_buf).unwrap_or(root)
            } else {
                root
            }
        })
        .reduce(|a, b| common_ancestor(&a, &b))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// The deepest directory both paths sit under, lexically (neither path
/// needs to exist; unrelated absolute roots share the filesystem root).
fn common_ancestor(a: &Path, b: &Path) -> PathBuf {
    let a: Vec<_> = a.components().collect();
    let b: Vec<_> = b.components().collect();
    let shared = a
        .iter()
        .zip(b.iter())
        .take_while(|(left, right)| left == right)
        .count();
    a[..shared].iter().collect()
}

/// Render the built-in rule-registry listing to `stdout`
/// (`docs/rule-registry.md`): text = one `<id> <codes> <summary>` line per
/// rule in registry order (comma-joined `LNTxxxx` codes), JSON = one
/// [`lintel_core::RuleMeta::to_json`] object per line. The listing never
/// scans sources, so the exit code is always 0; a broken/injected sink
/// fails the run like `--help` does.
fn rules(format: Format, stdout: &mut dyn Write) -> Result<i32, CliError> {
    let mut out = String::new();
    match format {
        Format::Text => {
            for meta in builtin_rule_metas() {
                let codes = meta
                    .codes
                    .iter()
                    .map(|code| code.lnt())
                    .collect::<Vec<_>>()
                    .join(",");
                let _ = writeln!(out, "{} {codes} {}", meta.id, meta.summary);
            }
        }
        Format::Json => {
            for meta in builtin_rule_metas() {
                let _ = writeln!(out, "{}", meta.to_json());
            }
        }
    }
    stdout.write_all(out.as_bytes())?;
    Ok(0)
}

/// Compile every source with `build` semantics: PHP goes to the
/// destination (stdout, `--out` directory, or `--in-place` sibling), a
/// failed file writes nothing and is rendered to `stderr` in the requested
/// [`Format`], and warnings render without failing the build. The M2
/// content-addressed cache (§9.3's warm-build target) reuses the compiled
/// PHP of an unchanged source on the same target — the key hashes the
/// source CONTENT with the target, and a hit replays the cold build's
/// warnings verbatim so warm stderr matches cold; `--no-cache` skips both
/// lookup and store. With a manifest in the plan, the successfully
/// compiled sources are scanned for top-level `function Name(...): Html`
/// declarations after the loop and the T-31 manifest is written (a
/// manifest failure fails the build the same way a failed artifact write
/// does). Returns 0 when every file compiled, 1 when at least one did not.
/// (The return is a plain code: per-file failures are reported in-band,
/// never propagated as `Err`.)
fn build(
    sources: &[PathBuf],
    plan: &BuildPlan,
    cache: Option<&cache::BuildCache>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let mut state = BuildState::default();
    for source in sources {
        let src = match fs::read_to_string(source) {
            Ok(src) => src,
            Err(e) => {
                let _ = writeln!(stderr, "lintel: cannot read {}: {e}", source.display());
                state.failed = true;
                continue;
            }
        };
        // The cache key is a content hash keyed with the target: an
        // unchanged source on the same target reuses the cold build's
        // artifact without compiling (mtime is never consulted — it lies
        // after a git checkout).
        let cached = cache.filter(|_| !plan.no_cache).and_then(|cache| {
            let key = cache::BuildCache::key(&src, plan.target);
            cache.lookup(&key, plan.target)
        });
        match cached {
            Some(hit) => {
                state.cache_hits += 1;
                replay_warnings(&hit.warnings_text, &hit.warnings_json, plan.format, stderr);
                emit_source(source, &src, &hit.php, plan, &mut state, stdout, stderr);
            }
            None => match compile(
                &src,
                &Options {
                    target: plan.target,
                },
            ) {
                Ok(out) => {
                    state.compiled += 1;
                    let warnings_text: Vec<String> = out
                        .warnings
                        .iter()
                        .map(|warning| warning.render(&src))
                        .collect();
                    let warnings_json: Vec<String> = out
                        .warnings
                        .iter()
                        .map(|warning| warning.to_json(&src))
                        .collect();
                    for warning in &out.warnings {
                        emit_diagnostic(warning, &src, plan.format, stderr);
                    }
                    emit_source(source, &src, &out.php, plan, &mut state, stdout, stderr);
                    if let (Some(cache), false) = (cache, plan.no_cache) {
                        let key = cache::BuildCache::key(&src, plan.target);
                        cache.store(
                            &key,
                            source,
                            plan.target,
                            &out.php,
                            &warnings_text,
                            &warnings_json,
                        );
                    }
                }
                Err(problems) => {
                    for problem in &problems {
                        emit_diagnostic(problem, &src, plan.format, stderr);
                    }
                    state.failed = true;
                }
            },
        }
    }
    if let Some(manifest_path) = &plan.manifest {
        if let Err(problem) =
            manifest::write_manifest(manifest_path, &state.components, &state.artifacts)
        {
            let _ = writeln!(
                stderr,
                "lintel: cannot write the manifest {}: {problem}",
                manifest_path.display()
            );
            state.failed = true;
        }
    }
    if let Some(cache) = cache {
        cache.record(&cache::CacheStats {
            compiled: state.compiled,
            cache_hits: state.cache_hits,
            bypassed: plan.no_cache,
        });
    }
    i32::from(state.failed)
}

/// Replay a cache hit's cold-build warnings so a warm build's stderr
/// matches cold byte for byte in either `--format`.
fn replay_warnings(
    warnings_text: &[String],
    warnings_json: &[String],
    format: Format,
    stderr: &mut dyn Write,
) {
    match format {
        Format::Text => {
            for warning in warnings_text {
                let _ = write!(stderr, "{warning}");
            }
        }
        Format::Json => {
            for warning in warnings_json {
                let _ = writeln!(stderr, "{warning}");
            }
        }
    }
}

/// The shared tail of one successful source (cache hit or fresh compile):
/// the T-31 manifest scan, the artifact pair, and the destination write
/// (its failure marks the build).
fn emit_source(
    source: &Path,
    src: &str,
    php: &str,
    plan: &BuildPlan,
    state: &mut BuildState,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) {
    if plan.manifest.is_some() {
        for found in manifest::scan_function_components(src) {
            state.components.push((found.name, source.to_path_buf()));
        }
        if let Some(artifact) = artifact_path(source, &plan.destination) {
            state.artifacts.push((source.to_path_buf(), artifact));
        }
    }
    if let Err(e) = write_output(source, php, &plan.destination, stdout) {
        let _ = writeln!(stderr, "lintel: cannot write {}: {e}", source.display());
        state.failed = true;
    }
}

/// `lintel migrate` (the Year-3 RFC-convergence card): rewrite each
/// source IN PLACE with the native RFC-parity lowering. Safety rails,
/// in order: a file that fails to compile is refused untouched (the
/// rendered diagnostics go to the error sink), and a file whose component
/// invocations none of the migrated sources declares is refused too — the
/// rewrite must not leave a call the migrated set cannot resolve.
/// `--dry-run` prints a unified diff per file (each hunk header carrying
/// the enclosing function signature, the way git's funcname context does)
/// and writes nothing; `--backup` keeps the original bytes beside the
/// rewrite as `<file>.bak`. The exit code reports whether any file was
/// refused.
fn migrate(
    sources: &[PathBuf],
    format: Format,
    dry_run: bool,
    backup: bool,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    // Pass 1: read every source and collect the declared component names
    // across the whole migrated set (case-insensitive — PHP function
    // lookups are case-insensitive, and the scanner's table agrees).
    let mut declared: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut read: Vec<(PathBuf, String)> = Vec::new();
    let mut failed = false;
    for source in sources {
        match fs::read_to_string(source) {
            Ok(src) => {
                let parsed = lintel_core::scan::scan(&src);
                for (name, _signature) in parsed.context.declared_signatures() {
                    declared.insert(name.to_lowercase());
                }
                read.push((source.clone(), src));
            }
            Err(e) => {
                let _ = writeln!(stderr, "lintel: cannot read {}: {e}", source.display());
                failed = true;
            }
        }
    }
    // Pass 2: compile, resolve, rewrite.
    for (source, src) in &read {
        match compile(
            src,
            &Options {
                target: Target::Rfc,
            },
        ) {
            Err(problems) => {
                for problem in &problems {
                    emit_diagnostic(problem, src, format, stderr);
                }
                failed = true;
            }
            Ok(out) => {
                if let Some((name, line)) = unresolved_invocation(src, &declared) {
                    match format {
                        Format::Text => {
                            let _ = writeln!(
                                stderr,
                                "lintel: cannot migrate {}: the component `{}` (line {}) is declared in no migrated source — migrate its declaring file too",
                                source.display(),
                                name,
                                line
                            );
                        }
                        Format::Json => {
                            let object = serde_json::json!({
                                "file": source.display().to_string(),
                                "line": line,
                                "component": name,
                                "message": "declared in no migrated source",
                                "migrate": "refused",
                            });
                            let _ = writeln!(stderr, "{object}");
                        }
                    }
                    failed = true;
                    continue;
                }
                if out.php == *src {
                    let _ = writeln!(stdout, "already migrated: {}", source.display());
                    continue;
                }
                let rewritten = changed_line_count(src, &out.php);
                if dry_run {
                    let _ = writeln!(
                        stdout,
                        "would migrate {} ({} lines rewritten)",
                        source.display(),
                        rewritten
                    );
                    let _ = unified_diff(source, src, &out.php, stdout);
                } else {
                    if backup {
                        let bak = PathBuf::from(format!("{}.bak", source.display()));
                        if let Err(e) = fs::write(&bak, src) {
                            let _ = writeln!(
                                stderr,
                                "lintel: cannot write the backup {}: {e}",
                                bak.display()
                            );
                            failed = true;
                            continue;
                        }
                    }
                    if let Err(e) = fs::write(source, &out.php) {
                        let _ =
                            writeln!(stderr, "lintel: cannot rewrite {}: {e}", source.display());
                        failed = true;
                        continue;
                    }
                    let _ = writeln!(
                        stdout,
                        "migrated {} ({} lines rewritten)",
                        source.display(),
                        rewritten
                    );
                }
            }
        }
    }
    i32::from(failed)
}

/// The first component invocation of `src` that neither the file itself
/// nor any migrated source declares: `(name, 1-based line)`, or `None`
/// when every invocation resolves. Cross-file components resolve through
/// the migrated set (see [`migrate`]'s declared-name pass), never through
/// the disk.
fn unresolved_invocation(
    src: &str,
    declared: &std::collections::HashSet<String>,
) -> Option<(String, usize)> {
    let parsed = lintel_core::scan::scan(src);
    let inventory = collect_inventory(&parsed.roots, &parsed.context);
    inventory
        .invocations
        .iter()
        .filter(|site| !site.declared_in_file && !declared.contains(&site.name.to_lowercase()))
        .map(|site| {
            let line = src[..site.offset.min(src.len())].matches('\n').count() + 1;
            (site.name.clone(), line)
        })
        .next()
}

/// How many lines the rewrite actually changes (the report's number).
fn changed_line_count(old: &str, new: &str) -> usize {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    old_lines
        .iter()
        .zip(new_lines.iter())
        .filter(|(a, b)| a != b)
        .count()
        + old_lines.len().abs_diff(new_lines.len())
}

/// One line-pair edit-script entry.
enum DiffOp<'a> {
    Same(&'a str),
    Del(&'a str),
    Add(&'a str),
}

/// The chars a [`hunk_header_context`] keeps before truncating with an
/// ellipsis (git truncates funcname context the same way).
const CONTEXT_HEADER_MAX: usize = 80;

/// The context lines a unified-diff hunk carries around its changes.
const DIFF_CONTEXT: usize = 2;

/// The LCS edit script between two line lists (the lines are small; the
/// `O(n·m)` table is fine for source files).
fn diff_ops<'a>(old: &[&'a str], new: &[&'a str]) -> Vec<DiffOp<'a>> {
    let n = old.len();
    let m = new.len();
    let mut table = vec![vec![0_u32; m + 1]; n + 1];
    for (i, old_line) in old.iter().enumerate().rev() {
        for (j, new_line) in new.iter().enumerate().rev() {
            table[i][j] = if old_line == new_line {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }
    let mut ops = Vec::new();
    let mut i = 0;
    let mut j = 0;
    while i < n && j < m {
        if old[i] == new[j] {
            ops.push(DiffOp::Same(old[i]));
            i += 1;
            j += 1;
        } else if table[i + 1][j] >= table[i][j + 1] {
            ops.push(DiffOp::Del(old[i]));
            i += 1;
        } else {
            ops.push(DiffOp::Add(new[j]));
            j += 1;
        }
    }
    ops.extend(old[i..].iter().map(|line| DiffOp::Del(line)));
    ops.extend(new[j..].iter().map(|line| DiffOp::Add(line)));
    ops
}

/// Whether this line declares a named PHP function — the funcname context
/// a hunk header carries. Visibility/static modifiers peel off first
/// (methods); a closure (`function (`) or the arrow `fn` spelling names
/// no component and is not a declaration header. Component identifiers
/// are ASCII-shaped (the scanner's rule), so the name check is ASCII.
fn is_function_declaration(line: &str) -> bool {
    let mut rest = line.trim_start();
    loop {
        let stripped = [
            "public",
            "protected",
            "private",
            "static",
            "final",
            "abstract",
        ]
        .iter()
        .find_map(|modifier| rest.strip_prefix(modifier))
        // `publication` is not the `public` modifier: a modifier word
        // must be followed by whitespace.
        .filter(|after| after.starts_with(char::is_whitespace));
        match stripped {
            Some(after) => rest = after.trim_start(),
            None => break,
        }
    }
    let Some(after) = rest.strip_prefix("function") else {
        return false;
    };
    let Some(after) = after.strip_prefix(char::is_whitespace) else {
        return false;
    };
    let after = after.trim_start();
    let after = after.strip_prefix('&').unwrap_or(after);
    let name_len = after
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .count();
    if name_len == 0 || after.starts_with(|c: char| c.is_ascii_digit()) {
        return false;
    }
    after[name_len..].trim_start().starts_with('(')
}

/// The function-context suffix one hunk header carries (`@@ … @@ function
/// Badge(string $name): Html`): the nearest function declaration at or
/// above the hunk's first old line — git's funcname convention, PHP-shaped
/// — trimmed, and truncated at [`CONTEXT_HEADER_MAX`] chars with an
/// ellipsis so a long signature stays a summary. An empty `String` means
/// the change sits before any declaration (the `<?php` prologue).
fn hunk_header_context(old_lines: &[&str], first_index: usize) -> String {
    let top = first_index.min(old_lines.len().saturating_sub(1));
    for index in (0..=top).rev() {
        if is_function_declaration(old_lines[index]) {
            let mut text = old_lines[index].trim().to_string();
            if text.chars().count() > CONTEXT_HEADER_MAX {
                text = text.chars().take(CONTEXT_HEADER_MAX).collect::<String>() + "…";
            }
            return format!(" {text}");
        }
    }
    String::new()
}

/// Print a unified diff (`--- a/<path>` … `@@ …` hunks, 2 context lines,
/// each hunk header carrying its function-level context) of `old` → `new`
/// to `stdout`. Returns whether anything was printed.
fn unified_diff(
    path: &Path,
    old: &str,
    new: &str,
    stdout: &mut dyn Write,
) -> std::io::Result<bool> {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let ops = diff_ops(&old_lines, &new_lines);
    if ops.iter().all(|op| matches!(op, DiffOp::Same(_))) {
        return Ok(false);
    }
    // Per-op line numbers: where the op's line sits in the old and new
    // file (1-based; a Del advances only the old counter, an Add only
    // the new one).
    let mut numbers: Vec<(u32, u32)> = Vec::with_capacity(ops.len());
    let mut old_no = 1_u32;
    let mut new_no = 1_u32;
    for op in &ops {
        numbers.push(match op {
            DiffOp::Same(_) => {
                let pair = (old_no, new_no);
                old_no += 1;
                new_no += 1;
                pair
            }
            DiffOp::Del(_) => {
                let pair = (old_no, new_no);
                old_no += 1;
                pair
            }
            DiffOp::Add(_) => {
                let pair = (old_no, new_no);
                new_no += 1;
                pair
            }
        });
    }
    let changed: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter(|(_, op)| !matches!(op, DiffOp::Same(_)))
        .map(|(index, _)| index)
        .collect();
    writeln!(stdout, "--- a/{}", path.display())?;
    writeln!(stdout, "+++ b/{}", path.display())?;
    let mut hunk_start = 0_usize;
    while hunk_start < changed.len() {
        // The hunk spans `changed[hunk_start..hunk_end]`, expanded by the
        // context window and merged with the next change when the windows
        // overlap.
        let mut hunk_end = hunk_start + 1;
        while hunk_end < changed.len()
            && changed[hunk_end] <= changed[hunk_end - 1] + 2 * DIFF_CONTEXT + 1
        {
            hunk_end += 1;
        }
        let first = changed[hunk_start].saturating_sub(DIFF_CONTEXT);
        let last = (changed[hunk_end - 1] + DIFF_CONTEXT).min(ops.len() - 1);
        let old_start = numbers[first].0;
        let new_start = numbers[first].1;
        let old_count = ops[first..=last]
            .iter()
            .filter(|op| !matches!(op, DiffOp::Add(_)))
            .count();
        let new_count = ops[first..=last]
            .iter()
            .filter(|op| !matches!(op, DiffOp::Del(_)))
            .count();
        writeln!(
            stdout,
            "@@ -{old_start},{old_count} +{new_start},{new_count} @@{}",
            hunk_header_context(
                &old_lines,
                usize::try_from(numbers[first].0.saturating_sub(1)).unwrap_or(0),
            )
        )?;
        for op in &ops[first..=last] {
            match op {
                DiffOp::Same(line) => writeln!(stdout, " {line}")?,
                DiffOp::Del(line) => writeln!(stdout, "-{line}")?,
                DiffOp::Add(line) => writeln!(stdout, "+{line}")?,
            }
        }
        hunk_start = hunk_end;
    }
    Ok(true)
}

/// Where this source's compiled artifact lands (the T-31 manifest's
/// `prod.artifacts` value): `<dir>/<name>.php` under `--out`, the sibling
/// `<name>.php` under `--in-place`, and nothing for a stdout build.
fn artifact_path(source: &Path, destination: &Destination) -> Option<PathBuf> {
    match destination {
        Destination::Stdout => None,
        Destination::Dir(dir) => Some(dir.join(output_name(source))),
        Destination::InPlace => {
            let parent = source.parent().unwrap_or(Path::new("."));
            Some(parent.join(output_name(source)))
        }
    }
}

/// Emit one diagnostic to the error sink in the requested [`Format`]:
/// [`Format::Text`] draws the human [`Diagnostic::render`] snippet,
/// [`Format::Json`] writes one [`Diagnostic::to_json`] object per line.
/// Like every error-sink write in the engine, a failing sink is best-effort
/// here (the per-file exit code, not `Err`, reports failures).
fn emit_diagnostic(problem: &Diagnostic, src: &str, format: Format, stderr: &mut dyn Write) {
    match format {
        Format::Text => {
            let _ = write!(stderr, "{}", problem.render(src));
        }
        Format::Json => {
            let _ = writeln!(stderr, "{}", problem.to_json(src));
        }
    }
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
/// `stderr` (both severities) in the requested [`Format`], nothing is
/// written, and the exit code fails only on error-severity diagnostics — a
/// warning-severity file passes the CI gate (doc 4.4).
/// The `analyze` dispatch: validate the explicit report path (a usage
/// error before any sink output, like a missing input path), collect the
/// sources, run the pass.
///
/// # Errors
///
/// A `--types` path that does not exist returns [`CliError`].
fn run_analyze(
    paths: &[PathBuf],
    types: Option<&Path>,
    target: Target,
    format: Format,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<i32, CliError> {
    if let Some(report) = types {
        if !report.is_file() {
            return Err(CliError(format!(
                "cannot read {}: no such file (--types names one analyze report file)",
                report.display()
            )));
        }
    }
    let sources = collect_sources(paths)?;
    Ok(analyze(&sources, types, target, format, stdout, stderr))
}

fn check(
    sources: &[PathBuf],
    target: Target,
    format: Format,
    types: Option<&Path>,
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
        if let Some(report) = types {
            match load_type_hints(source, Some(report)) {
                Ok(hints) => report_stale(source, &src, &hints, format, stderr),
                Err(problem) => {
                    let _ = writeln!(stderr, "lintel: {}: {problem}", source.display());
                    failed = true;
                }
            }
        }
        match compile(&src, &Options { target }) {
            Ok(out) => {
                for warning in &out.warnings {
                    emit_diagnostic(warning, &src, format, stderr);
                }
            }
            Err(problems) => {
                for problem in &problems {
                    emit_diagnostic(problem, &src, format, stderr);
                }
                failed = true;
            }
        }
    }
    i32::from(failed)
}

/// The `analyze` runner (§7.3-Y2, `docs/type-elision.md`): compile each
/// source through [`compile_analyzed`] with the `PHPStan` bridge's proven
/// child types, write the PHP to stdout exactly like `build`'s stdout lane,
/// and report the elision counters on stderr in the requested [`Format`].
/// Exit 1 when any file fails to parse/validate or its analyze report is
/// malformed; usage errors (an explicit `--types` path that does not
/// exist) surface as [`CliError`] from [`run`].
fn analyze(
    sources: &[PathBuf],
    types: Option<&Path>,
    target: Target,
    format: Format,
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
        let hints = match load_type_hints(source, types) {
            Ok(hints) => hints,
            Err(problem) => {
                let _ = writeln!(stderr, "lintel: {}: {problem}", source.display());
                failed = true;
                continue;
            }
        };
        match compile_analyzed(
            &src,
            &Options { target },
            &hints,
            lintel_core::builtin_rules(),
        ) {
            Ok(out) => {
                for warning in &out.output.warnings {
                    emit_diagnostic(warning, &src, format, stderr);
                }
                if let Err(e) = stdout.write_all(out.output.php.as_bytes()) {
                    let _ = writeln!(stderr, "lintel: cannot write {}: {e}", source.display());
                    failed = true;
                    continue;
                }
                let stale = lintel_core::stale_hints(&src, &hints).len();
                report_elision(source, &out, stale, format, stderr);
            }
            Err(problems) => {
                for problem in &problems {
                    emit_diagnostic(problem, &src, format, stderr);
                }
                failed = true;
            }
        }
    }
    i32::from(failed)
}

/// The stale-hint advisories for one source (§7.3-Y2 UX): report entries
/// that match no child expression of the current source — an analysis run
/// against an older artifact. Advisory by construction (elision already
/// ignores them): text prints one line per stale entry, JSON emits one
/// object per entry with `stale: true`.
fn report_stale(
    source: &Path,
    src: &str,
    hints: &TypeHints,
    format: Format,
    stderr: &mut dyn Write,
) {
    for stale in lintel_core::stale_hints(src, hints) {
        match format {
            Format::Text => {
                let _ = writeln!(
                    stderr,
                    "check {}: stale analyze hint at line {}: {} (type {})",
                    source.display(),
                    stale.line,
                    stale.expr,
                    stale.kind
                );
            }
            Format::Json => {
                let object = serde_json::json!({
                    "file": source.display().to_string(),
                    "line": stale.line,
                    "expr": stale.expr,
                    "type": stale.kind,
                    "stale": true,
                });
                let _ = writeln!(stderr, "{object}");
            }
        }
    }
}

/// The elision report for one compiled file: text prints one summary line,
/// JSON emits one object — `file`, `children`, `considered`, `elided` and
/// `rate` (elided/considered, `null` when nothing reached the step).
fn report_elision(
    source: &Path,
    out: &lintel_core::AnalyzedOutput,
    stale: usize,
    format: Format,
    stderr: &mut dyn Write,
) {
    let rate = out.elision_rate();
    match format {
        Format::Text => {
            let rate_note = rate.map_or_else(String::new, |value| {
                format!(" ({:.1}% elision rate)", value * 100.0)
            });
            let stale_note = if stale == 0 {
                String::new()
            } else {
                format!(", {stale} stale hint(s) ignored")
            };
            let _ = writeln!(
                stderr,
                "analyze {}: {} dynamic children, {} decision-reachable, {} elided by type{}{}",
                source.display(),
                out.dynamic_children,
                out.considered,
                out.elided,
                rate_note,
                stale_note,
            );
        }
        Format::Json => {
            let object = serde_json::json!({
                "file": source.display().to_string(),
                "children": out.dynamic_children,
                "considered": out.considered,
                "elided": out.elided,
                "stale": stale,
                "rate": rate,
            });
            let _ = writeln!(stderr, "{object}");
        }
    }
}

/// The analyze report for one source: an explicit `--types PATH` applies to
/// every source; otherwise the per-source sidecar
/// `<source>.lintel-types.json` (the bridge runner's default output) is
/// advisory — an absent sidecar compiles without hints. A malformed or
/// contract-mismatching report fails the file (exit 1), never silently.
///
/// # Errors
///
/// Returns the problem text for unreadable/malformed/unknown-shaped
/// reports; the caller renders it and fails the file.
fn load_type_hints(source: &Path, types: Option<&Path>) -> Result<TypeHints, String> {
    let report = if let Some(path) = types {
        path.to_path_buf()
    } else {
        let mut sidecar = source.as_os_str().to_os_string();
        sidecar.push(".lintel-types.json");
        PathBuf::from(sidecar)
    };
    let json = match fs::read_to_string(&report) {
        Ok(json) => json,
        Err(e) => {
            return if types.is_some() {
                Err(format!("cannot read {}: {e}", report.display()))
            } else {
                // The advisory sidecar is absent: no analyze information.
                Ok(TypeHints::default())
            };
        }
    };
    parse_type_hints(&json)
}

/// Parse the bridge runner's report JSON into [`TypeHints`].
///
/// # Errors
///
/// Every deviation from the documented shape fails with a precise message:
/// a non-object document, an unknown `version`, a non-list `children`, or
/// an entry whose `line`/`expr`/`type`/`value` fields are missing,
/// mistyped, or name an unknown kind (a producer/consumer contract
/// mismatch must be loud, not silently dropped).
fn parse_type_hints(json: &str) -> Result<TypeHints, String> {
    let document: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("the analyze report is not JSON: {e}"))?;
    let object = document
        .as_object()
        .ok_or("the analyze report is not a JSON object")?;
    let version = object
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or("the analyze report has no integer `version`")?;
    if version != 1 {
        return Err(format!("unsupported analyze report version {version}"));
    }
    let children = object
        .get("children")
        .and_then(serde_json::Value::as_array)
        .ok_or("the analyze report has no `children` list")?;
    let mut entries = Vec::with_capacity(children.len());
    for child in children {
        let entry = child
            .as_object()
            .ok_or("the analyze report `children` holds a non-object entry")?;
        let line = entry
            .get("line")
            .and_then(serde_json::Value::as_u64)
            .ok_or("an analyze report entry has no integer `line`")?;
        let expr = entry
            .get("expr")
            .and_then(serde_json::Value::as_str)
            .ok_or("an analyze report entry has no string `expr`")?
            .to_string();
        let kind_name = entry
            .get("type")
            .and_then(serde_json::Value::as_str)
            .ok_or("an analyze report entry has no string `type`")?;
        let value = entry.get("value").and_then(serde_json::Value::as_str);
        match ChildKind::parse(kind_name, value) {
            Ok(Some(kind)) => entries.push(ChildHint {
                line: u32::try_from(line)
                    .map_err(|_| "an analyze report `line` is out of range")?,
                expr,
                kind,
            }),
            // Known but non-elidable (bool, string, …): the report carries
            // every resolved type; these simply keep the helper.
            Ok(None) => {}
            Err(problem) => return Err(problem),
        }
    }
    Ok(TypeHints::from_entries(entries))
}

/// The parsed `analyze` invocation: paths, the optional report file, the
/// target and the report format.
struct AnalyzeArgs {
    paths: Vec<PathBuf>,
    types: Option<PathBuf>,
    target: Target,
    format: Format,
}

/// The `analyze` argument parser: paths, `--types PATH`, `--target` and
/// `--format` (no output-destination flags — the compiled PHP always goes
/// to stdout this pass; destination flags stay `build`'s).
///
/// # Errors
///
/// Unknown flags, missing flag values, unknown targets/formats.
fn parse_analyze(args: &[String]) -> Result<ControlFlow<Invocation, AnalyzeArgs>, CliError> {
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut types: Option<PathBuf> = None;
    let mut target = Target::default();
    let mut format = Format::default();
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        index += 1;
        match arg {
            "--help" | "-h" => return Ok(ControlFlow::Break(Invocation::Help)),
            "--version" => return Ok(ControlFlow::Break(Invocation::Version)),
            s if s.starts_with("--types=") => {
                types = Some(PathBuf::from(&s["--types=".len()..]));
            }
            "--types" => {
                let value = value_flag(args, &mut index, "--types needs a value: the report file")?;
                types = Some(PathBuf::from(value));
            }
            s if s.starts_with("--target=") => {
                target = parse_target(&s["--target=".len()..])?;
            }
            "--target" => {
                let value = value_flag(
                    args,
                    &mut index,
                    "--target needs a value: string, stream or rfc",
                )?;
                target = parse_target(&value)?;
            }
            s if s.starts_with("--format=") => {
                format = parse_format(&s["--format=".len()..])?;
            }
            "--format" => {
                let value = value_flag(args, &mut index, "--format needs a value: text or json")?;
                format = parse_format(&value)?;
            }
            s if s.starts_with('-') => {
                return Err(CliError(format!(
                    "unknown flag `{s}`: analyze takes --types, --target and --format \
                     (see `lintel --help`)"
                )));
            }
            path => paths.push(PathBuf::from(path)),
        }
    }
    Ok(ControlFlow::Continue(AnalyzeArgs {
        paths,
        types,
        target,
        format,
    }))
}

/// Compile the per-file component shape report with `stats` semantics: the
/// scanner runs (the pipeline `check` uses, minus validation and emission —
/// the shape report is target-independent), parse diagnostics are rendered
/// to `stderr` in the requested [`Format`] and fail the file, and every
/// clean file prints one report section to `stdout` — the text table or one
/// JSON object per file. Validation lints play no part: `stats` is a shape
/// report, not a linter. Returns 0 when every file scanned, 1 when at least
/// one failed to parse (or could not be read).
fn stats(
    sources: &[PathBuf],
    format: Format,
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
        let parsed = lintel_core::scan::scan(&src);
        if !parsed.diagnostics.is_empty() {
            for problem in &parsed.diagnostics {
                emit_diagnostic(problem, &src, format, stderr);
            }
            failed = true;
            continue;
        }
        let inventory = lintel_core::collect_inventory(&parsed.roots, &parsed.context);
        match format {
            Format::Text => {
                let _ = write!(stdout, "{}", stats_text(source, &inventory, &src));
            }
            Format::Json => {
                let _ = writeln!(stdout, "{}", stats_json(source, &inventory, &src));
            }
        }
    }
    i32::from(failed)
}

/// The text report for one file: the path header, then one `declared`
/// line per declared component — `Name(params)` with `!` marking a
/// required parameter and `...` the variadic tail — and one `invoke` line
/// per invocation site (position mapped through [`lintel_core::SourceMap`],
/// flags appended in the fixed order `children`, `spread`, `undeclared`).
fn stats_text(source: &Path, inventory: &lintel_core::FileInventory, src: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{}", source.display());
    let map = lintel_core::SourceMap::new(src);
    for component in &inventory.declared {
        let signature = &component.signature;
        let params = signature
            .params
            .iter()
            .enumerate()
            .map(|(index, param)| {
                let tail = if signature.variadic && index == signature.params.len() - 1 {
                    "..."
                } else if signature.required.contains(param) {
                    "!"
                } else {
                    ""
                };
                format!("{param}{tail}")
            })
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(out, "  declared {}({params})", component.name);
    }
    for site in &inventory.invocations {
        let (line, col) = map.line_col(site.offset);
        let mut flags = String::new();
        if site.children {
            flags.push_str("; children");
        }
        if site.spread {
            flags.push_str("; spread");
        }
        if !site.declared_in_file {
            flags.push_str("; undeclared");
        }
        let props = if site.props.is_empty() {
            "none".to_string()
        } else {
            site.props.join(", ")
        };
        let _ = writeln!(
            out,
            "  invoke <{}> at {line}:{col} props: {props}{flags}",
            site.name
        );
    }
    out
}

/// The JSON report line for one file: the core inventory's documented
/// `to_json` object with a `file` field prepended — the splice replaces the
/// leading `{"declared"` key, which can only be the object's first field,
/// so no value can ever be mistaken for it (`docs/component-inventory.md`).
fn stats_json(source: &Path, inventory: &lintel_core::FileInventory, src: &str) -> String {
    let mut file = String::new();
    file.push('"');
    lintel_core::json_escape(&source.to_string_lossy(), &mut file);
    file.push('"');
    inventory.to_json(src).replacen(
        "{\"declared\"",
        &format!("{{\"file\":{file},\"declared\""),
        1,
    )
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

/// The name of the demo source `lintel init` scaffolds.
pub const DEMO_SOURCE_NAME: &str = "hello.lintel.php";

/// The name of the file `lintel init` compiles the demo into.
pub const DEMO_COMPILED_NAME: &str = "hello.php";

/// The five task-card T-15 demos, `lintel init --demo N` (1-based)
/// scaffolds: each is ≤ 60 lines, compiles with zero diagnostics on the
/// default target, and — required with the §6.8 runtime — renders HTML with
/// asserted output (`tests/php_demo.rs` executes all five under PHP).
/// (1) component-library page, (2) XSS-attack gallery, (3) 10k-row table,
/// (4) HTMX todo, (5) compile-error showcase.
pub const DEMO_SOURCES: [&str; 5] = [
    // Demo 1: component library page.
    r"<?php
// Generated by `lintel init --demo 1` — a component-library page.

function Badge(string $label, string $variant = 'plain'): Html
{
    return <span class={['badge', 'badge--' . $variant]}>{$label}</span>;
}

function Card(string $title, ?Html $children = null): Html
{
    return <article class='card'>
        <h2 class='card__title'>{$title}</h2>
        {$children}
    </article>;
}

function Stack(Html $children): Html
{
    return <div class='stack'>{$children}</div>;
}

echo <Stack>
    <Card title='Badges'>
        <Badge label='new' variant='accent' />
        <Badge label='beta' variant='warn' />
        <Badge label='stable' />
    </Card>
    <Card title='Composable'>
        <p>Components are PHP functions; markup is an expression.</p>
    </Card>
</Stack>;
",
    // Demo 2: XSS-attack gallery (the §7.2 corpus, neutralized in text and URL positions).
    r#"<?php
// Generated by `lintel init --demo 2` — the XSS gallery: every payload neutralized.

const PAYLOADS = [
    'javascript:alert(1)', 'JaVaScRiPt:alert(1)', 'data:text/html;base64,PHNjcmlwdD4=', 'vbscript:msgbox',
    'java' . "\t" . 'script:alert(1)', '<script>alert(1)</script>', '" onmouseover="x()', '</script>',
];

function Victim(string $payload): Html
{
    return <div class='attack'>
        <p class='text'>text child: {$payload}</p>
        <a class='url' href={$payload}>href attr: {$payload}</a>
    </div>;
}

$rendered = [];
foreach (PAYLOADS as $payload) {
    $rendered[] = <Victim payload={$payload} />;
}

echo <main class='gallery'>
    <h1>XSS gallery: every payload neutralized</h1>
    {$rendered}
</main>;
"#,
    // Demo 3: 10k-row table.
    r"<?php
// Generated by `lintel init --demo 3` — a 10,000-row table.

function Rows(int $count): Html
{
    $o = [];
    for ($i = 1; $i <= $count; $i++) {
        $o[] = <tr><td>{$i}</td><td>row {$i}</td><td class='num'>{$i}.5</td></tr>;
    }
    return <table class='rows'><tbody>{$o}</tbody></table>;
}

echo <main>
    <h1>10,000 rows, one pass, no virtual DOM</h1>
    <Rows count={10000} />
</main>;
",
    // Demo 4: HTMX todo.
    r"<?php
// Generated by `lintel init --demo 4` — an HTMX todo list.

function TodoItem(string $text, bool $done): Html
{
    return <li class={['todo', 'todo--done' => $done]}>
        <span class='todo__text'>{$text}</span>
        <button hx-post='/todos/toggle' hx-target='closest li' hx-swap='outerHTML'>toggle</button>
    </li>;
}

echo <main class='todos'>
    <h1>Todo (HTMX)</h1>
    <form hx-post='/todos' hx-target='#list' hx-swap='beforeend'>
        <input name='text' placeholder='add a todo' />
        <button type='submit'>add</button>
    </form>
    <ul id='list'>
        <TodoItem text='write JSX in PHP' done={true} />
        <TodoItem text='refresh to recompile' done={false} />
    </ul>
    <script src='https://unpkg.com/htmx.org@2' defer></script>
</main>;
",
    // Demo 5: compile-error showcase (the LNT codes as content cards).
    r"<?php
// Generated by `lintel init --demo 5` — the compile-error showcase.

const ERRORS = [
    ['LNT0102', 'mismatched close tag', 'Did you mean </div>? The compiler suggests the nearest open tag.'],
    ['LNT0201', 'builtin shadow', '<Header> would call PHP\'s header() — declare or import the component first.'],
    ['LNT0202', 'hyphenated prop', 'data-x can\'t be a named argument — spread it: <Card {...[\'data-x\' => 1]} />'],
    ['LNT0301', 'dynamic handler', 'onclick={…} is a compile error; static strings lint, data never becomes JS.'],
    ['LNT0401', 'void with children', '<br>children</br> is invalid: void elements take no children.'],
];

function ErrorCard(string $code, string $name, string $help): Html
{
    return <article class={'error error--' . $code}>
        <h2>{$code}</h2>
        <p class='name'>{$name}</p>
        <p class='help'>help: {$help}</p>
    </article>;
}

$cards = [];
foreach (ERRORS as [$code, $name, $help]) {
    $cards[] = <ErrorCard code={$code} name={$name} help={$help} />;
}

echo <main class='errors'><h1>Errors that teach</h1>{$cards}</main>;
",
];

/// The file-stem slug per T-15 demo (`demo1-component-library`, …).
pub const DEMO_SLUGS: [&str; 5] = [
    "component-library",
    "xss-gallery",
    "table-10k",
    "htmx-todo",
    "error-showcase",
];

/// The runnable demo `lintel init` scaffolds (task-card T-10 acceptance:
/// "`lintel init` yields a runnable demo in one command"). It compiles
/// with zero diagnostics on the default target and exercises the core
/// contract: an in-file component with a required prop, a `string`-typed
/// parameter folding to the direct escape (`docs/escape-elision.md`),
/// static markup folding, and line preservation.
pub const DEMO_SOURCE: &str = "\
<?php
// Generated by `lintel init` — a runnable demo page. Edit this file,
// then run `lintel build hello.lintel.php` (or `lintel check
// hello.lintel.php` for the diagnostics-only CI gate).

function Hello(string $name): Html {
    return <div class=\"hello\">
        <h1>Hello, {$name}!</h1>
        <p class=\"tagline\">Markup is an expression: JSX compiles ahead of
            time to plain PHP, the author's line numbers are preserved, and
            dynamic values are escaped by default.</p>
        <ul class=\"points\">
            <li>static markup folds into string literals</li>
            <li><b class=\"x\">{$name}</b> rides through the escape helpers</li>
            <li><a href=\"https://lintel.sh\">links</a> are scheme-checked</li>
        </ul>
    </div>;
}

echo <Hello name=\"Lintel\" />;
";

/// What `lintel init` wrote — the scaffolding report printed to stdout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InitReport {
    /// The scaffolded demo source, ready to edit.
    pub source: PathBuf,
    /// The demo compiled once by the same command.
    pub compiled: PathBuf,
    /// Line count of the scaffolded source (line preservation makes this
    /// the compiled file's line count too).
    pub lines: usize,
}

/// The usage-style error for a demo source that does not compile: each
/// problem's human headline (`error[LNTxxxx]: …`), one per line, instead
/// of a `Debug` dump — the same shape a failed `lintel check` renders.
fn compile_problems_error(problems: &[Diagnostic]) -> CliError {
    let mut message = String::from("the demo source does not compile:");
    for problem in problems {
        let _ = write!(message, "\n  {}", problem.headline());
    }
    CliError(message)
}

/// Scaffold the runnable demo (task-card T-10 acceptance): write
/// [`DEMO_SOURCE`] into `target_dir` as [`DEMO_SOURCE_NAME`], creating
/// missing directories, and compile it once to [`DEMO_COMPILED_NAME`] so a
/// single command leaves a runnable page — the engine's own `build`
/// semantics, applied to the demo. An existing compiled file is rebuilt
/// from the pristine source (the source file is the user-editable
/// artifact); an existing *source* file is never touched.
///
/// # Errors
///
/// Returns [`CliError`] when [`DEMO_SOURCE_NAME`] already exists (no
/// clobber), when the directories or files cannot be written, or — never,
/// while [`DEMO_SOURCE`] stays valid — when the demo fails to compile.
pub fn init(target_dir: &Path) -> Result<InitReport, CliError> {
    init_demo(target_dir, None)
}

/// Scaffold one of the runnable demos (task-cards T-10/T-15): `demo` `None`
/// writes the hello demo ([`DEMO_SOURCE_NAME`]/[`DEMO_COMPILED_NAME`]);
/// `Some(n)` (1-based) writes T-15 demo `n` as
/// `demo{n}-{slug}.lintel.php` plus its compiled `demo{n}-{slug}.php`.
/// Either way the demo compiles once in the same command, so a single
/// command leaves a runnable page.
///
/// # Errors
///
/// Returns [`CliError`] when the target source already exists (no clobber),
/// when the directories or files cannot be written, or — never, while the
/// demo sources stay valid — when the demo fails to compile.
pub fn init_demo(target_dir: &Path, demo: Option<usize>) -> Result<InitReport, CliError> {
    let (source_name, compiled_name, source_body) = match demo {
        None => (DEMO_SOURCE_NAME, DEMO_COMPILED_NAME, DEMO_SOURCE),
        Some(n) if (1..=DEMO_SOURCES.len()).contains(&n) => (
            demo_source_name(n),
            demo_compiled_name(n),
            DEMO_SOURCES[n - 1],
        ),
        Some(n) => {
            return Err(CliError(format!(
                "unknown demo `{n}`: --demo takes 1-{} (component-library, xss-gallery, \
                 table-10k, htmx-todo, error-showcase)",
                DEMO_SOURCES.len()
            )))
        }
    };
    let source = target_dir.join(source_name);
    if source.exists() {
        return Err(CliError(format!(
            "refusing to overwrite existing {}: remove it (or the whole demo) \
             first, or init into another directory",
            source.display()
        )));
    }
    fs::create_dir_all(target_dir)?;
    fs::write(&source, source_body)?;
    let compiled_php = compile(source_body, &Options::default())
        .map_err(|problems| compile_problems_error(&problems))?
        .php;
    let compiled = target_dir.join(compiled_name);
    fs::write(&compiled, compiled_php)?;
    Ok(InitReport {
        lines: source_body.lines().count(),
        source,
        compiled,
    })
}

/// The source file name for T-15 demo `n` (1-based).
#[must_use]
pub fn demo_source_name(n: usize) -> &'static str {
    // `demo{n}-{slug}.lintel.php` — the names are fixed constants, so the
    // match keeps them `&'static str` without runtime formatting.
    match n {
        1 => "demo1-component-library.lintel.php",
        2 => "demo2-xss-gallery.lintel.php",
        3 => "demo3-table-10k.lintel.php",
        4 => "demo4-htmx-todo.lintel.php",
        5 => "demo5-error-showcase.lintel.php",
        _ => unreachable!("demo index validated by the caller"),
    }
}

/// The compiled file name for T-15 demo `n` (1-based).
#[must_use]
pub fn demo_compiled_name(n: usize) -> &'static str {
    match n {
        1 => "demo1-component-library.php",
        2 => "demo2-xss-gallery.php",
        3 => "demo3-table-10k.php",
        4 => "demo4-htmx-todo.php",
        5 => "demo5-error-showcase.php",
        _ => unreachable!("demo index validated by the caller"),
    }
}

#[cfg(test)]
mod tests {
    use super::compile_problems_error;
    use lintel_core::{Code, Diagnostic};

    #[test]
    fn demo_compile_failures_render_human_headlines() {
        let problems = vec![
            Diagnostic::new(Code::MissingRequiredProp, "missing `$label`", 3),
            Diagnostic::new(Code::BadAttr, "bad attribute", 7),
        ];
        let message = compile_problems_error(&problems).0;
        assert!(
            message.starts_with("the demo source does not compile:"),
            "{message}"
        );
        assert!(
            message.contains("error[LNT0205]: missing `$label`"),
            "{message}"
        );
        assert!(
            message.contains("error[LNT0103]: bad attribute"),
            "{message}"
        );
        assert!(
            !message.contains("Diagnostic {"),
            "no Debug dump: {message}"
        );
    }
}
