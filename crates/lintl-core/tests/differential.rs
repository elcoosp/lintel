//! The T-09 differential acceptance (architecture doc §13.2, task card T-09
//! §15): **compile → php -l → execute → compare stdout with a hand-written
//! reference PHP renderer**, for ≥ 200 generated templates — property-based:
//! proptest builds random JSX trees and the reference model in
//! `tests/differential/reference_renderer.php` (hand-written from §4/§7.1)
//! renders the same tree with the same argument values; the compiled
//! template's stdout must match byte-for-byte.
//!
//! Per the T-09 reconciliation contract (§21.2), the three known divergence
//! classes — class lists, spreads, self-closing shapes — each have minimal
//! isolated cases in [`cases::iso_cases`], and every reconciled
//! case stays here as a permanent regression.
//!
//! The case definitions (strategies + isolated cases) live in the shared
//! [`cases`] module so the fuzz-corpus seeder (`fuzz_corpus.rs`, Phase 12
//! M2) consumes exactly these sources.
//!
//! The PHP binary comes from `PHP_BIN` (default `php` on `PATH`); without
//! one the gate prints a skip note (CI's `php` job always sets it).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use proptest::prelude::*;
use serde_json::Value;

use lintl_core::{compile, Options, Target};

#[path = "differential/cases.rs"]
mod cases;

use cases::{case_strategy, iso_cases, Case, IsoCase};

// ---------------------------------------------------------------------------
// PHP execution harness.
// ---------------------------------------------------------------------------

fn php_bin() -> Option<PathBuf> {
    let name = std::env::var("PHP_BIN").unwrap_or_else(|_| "php".to_string());
    if name.contains('/') {
        let path = PathBuf::from(name);
        return path.is_file().then_some(path);
    }
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|dir| dir.join(&name))
        .find(|candidate| candidate.is_file())
}

/// The per-case driver: materialises the argument specs, loads the emitted
/// template and echoes the render (or the contract-throw marker).
const DRIVER_PHP: &str = r"<?php
declare(strict_types=1);
require $argv[1];
class DS implements Stringable {
    public function __construct(private string $s) {}
    public function __toString(): string { return $this->s; }
}
$payload = json_decode(file_get_contents($argv[2]), true);
$args = array_map(static function (array $spec) {
    return match ($spec['k']) {
        'string' => (string) $spec['v'],
        'int', 'float', 'bool' => $spec['v'],
        'null' => null,
        'array' => $spec['v'],
        'stringable' => new DS((string) $spec['v']),
        'html' => Html::unsafeRaw($spec['v']),
        default => throw new LogicException('bad arg spec'),
    };
}, $payload['args']);
require $argv[3];
try {
    if ($argv[4] === 'stream') {
        foreach (T(...$args) as $chunk) { echo $chunk; }
    } else {
        echo T(...$args);
    }
} catch (InvalidArgumentException $e) {
    echo 'THROW:InvalidArgumentException';
}
";

struct Harness {
    bin: PathBuf,
    runtime: PathBuf,
    reference: PathBuf,
    driver: PathBuf,
    dir: PathBuf,
    counter: AtomicUsize,
}

/// Parallel test binaries share the process id — give each harness an
/// extra unique suffix so one Drop never deletes another's directory.
static HARNESS_SEQ: AtomicUsize = AtomicUsize::new(0);

impl Harness {
    fn new(bin: PathBuf) -> Self {
        let seq = HARNESS_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("lintl-differential-{}-{seq}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("differential temp dir");
        let driver = dir.join("driver.php");
        std::fs::write(&driver, DRIVER_PHP).expect("write driver");
        Harness {
            runtime: Path::new(&env!("CARGO_MANIFEST_DIR"))
                .join("../../php/runtime/src/functions.php"),
            reference: Path::new(&env!("CARGO_MANIFEST_DIR"))
                .join("tests/differential/reference_renderer.php"),
            driver,
            dir,
            counter: AtomicUsize::new(0),
            bin,
        }
    }

    fn run_php(&self, args: &[&Path]) -> Result<String, String> {
        let output = Command::new(&self.bin)
            .args(args.iter().map(|p| p.as_os_str()))
            .output()
            .map_err(|error| format!("spawning php: {error}"))?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        if !output.status.success() {
            return Err(format!(
                "php {} failed\n--- stdout ---\n{stdout}\n--- stderr ---\n{}",
                args[0].display(),
                String::from_utf8_lossy(&output.stderr),
            ));
        }
        Ok(stdout)
    }

    fn lint(&self, name: &str, content: &str) -> Result<(), String> {
        let file = self.dir.join(format!("{name}.php"));
        std::fs::write(&file, content).map_err(|error| format!("write {name}: {error}"))?;
        self.run_php(&[file.as_path()])
            .map(|_| ())
            .map_err(|report| {
                format!("php -l {name} failed:\n{report}\n--- emitted ---\n{content}")
            })
    }

    fn next_paths(&self) -> (PathBuf, PathBuf) {
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        (
            self.dir.join(format!("payload-{n}.json")),
            self.dir.join(format!("emitted-{n}.php")),
        )
    }

    fn execute_against_reference(
        &self,
        name: &str,
        source: &str,
        payload: &Value,
        target: Target,
    ) -> Result<(), String> {
        let (payload_path, emitted_path) = self.next_paths();
        std::fs::write(&payload_path, serde_json::to_vec(payload).unwrap())
            .map_err(|error| format!("write payload: {error}"))?;
        let out = compile(source, &Options { target }).map_err(|problems| {
            format!(
                "{name}: unexpected compile diagnostics: {problems:?}\n--- source ---\n{source}"
            )
        })?;
        self.lint(name, &out.php)?;
        std::fs::write(&emitted_path, &out.php)
            .map_err(|error| format!("write emitted: {error}"))?;
        let expected = self
            .run_php(&[&self.reference, &payload_path])
            .map_err(|report| {
                format!("{name}: reference renderer failed:\n{report}\n--- payload ---\n{payload}")
            })?;
        let kind = match target {
            Target::Stream => "stream",
            _ => "string",
        };
        let actual = self
            .run_php(&[
                &self.driver,
                &self.runtime,
                &payload_path,
                &emitted_path,
                Path::new(kind),
            ])
            .map_err(|report| {
                format!(
                    "{name}: execution failed:\n{report}\n--- emitted ---\n{}",
                    out.php
                )
            })?;
        if actual == expected {
            Ok(())
        } else {
            Err(format!(
                "{name}: compiled output diverged from the reference\n--- source ---\n{source}\n--- emitted ---\n{}\n--- expected ---\n{expected}\n--- actual ---\n{actual}",
                out.php
            ))
        }
    }

    /// One differential case: compile -> php -l (all three targets) ->
    /// execute (string + stream) -> compare stdout with the reference.
    fn check_case(&self, case: &Case) -> Result<(), String> {
        for (label, target) in [
            ("string", Target::String),
            ("stream", Target::Stream),
            ("rfc", Target::Rfc),
        ] {
            let out = compile(&case.source, &Options { target }).map_err(|problems| {
                format!(
                    "unexpected compile diagnostics on {label}: {problems:?}\n--- source ---\n{}",
                    case.source
                )
            })?;
            self.lint(&format!("emitted-{label}"), &out.php)?;
        }
        self.execute_against_reference(
            "differential-string",
            &case.source,
            &case.payload,
            Target::String,
        )?;
        self.execute_against_reference(
            "differential-stream",
            &case.source,
            &case.payload,
            Target::Stream,
        )
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn harness_or_skip() -> Option<Harness> {
    let bin = php_bin()?;
    Some(Harness::new(bin))
}

const DIFFERENTIAL_CASES: u32 = 220;

/// §13.2: ≥ 200 generated templates equal the reference renderer, on both
/// executable targets, with all three targets' outputs linting.
#[test]
fn differential_generated_templates_match_reference() {
    let Some(harness) = harness_or_skip() else {
        eprintln!("skip differential: no PHP binary (set PHP_BIN)");
        return;
    };
    let config = ProptestConfig::with_cases(DIFFERENTIAL_CASES);
    proptest!(config, |(case in case_strategy())| {
        if let Err(report) = harness.check_case(&case) {
            prop_assert!(false, "{report}");
        }
    });
}

/// The per-divergence reconciliation: every isolated case must agree with
/// the reference renderer (or the compiler must reject with its code).
#[test]
fn differential_isolated_cases() {
    let Some(harness) = harness_or_skip() else {
        eprintln!("skip differential_isolated_cases: no PHP binary (set PHP_BIN)");
        return;
    };
    for case in iso_cases() {
        match case {
            IsoCase::Reject { name, source, code } => {
                let problems = compile(
                    &source,
                    &Options {
                        target: Target::String,
                    },
                )
                .err()
                .unwrap_or_else(|| panic!("{name}: expected the compiler to reject with {code}"));
                assert!(
                    problems.iter().any(|d| d.code.to_string() == code),
                    "{name}: expected {code}, got {problems:?}"
                );
            }
            IsoCase::Compare {
                name,
                source,
                payload,
            } => {
                harness
                    .execute_against_reference(&name, &source, &payload, Target::String)
                    .unwrap_or_else(|report| panic!("{report}"));
            }
        }
    }
}
