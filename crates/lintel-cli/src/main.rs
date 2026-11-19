//! The `lintel` binary: a thin wrapper over the [`lintel_cli`] engine.
//! Argument collection is the only process-specific work; everything else
//! (parsing, compiling, diagnostic rendering, exit codes) runs through
//! [`lintel_cli::run`], which the integration tests drive directly.

use std::io::Write as _;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut stdout = std::io::stdout().lock();
    let mut stderr = std::io::stderr().lock();
    match lintel_cli::run(&args, &mut stdout, &mut stderr) {
        Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(u8::MAX)),
        Err(usage) => {
            let _ = writeln!(stderr, "lintel: {usage}");
            let _ = writeln!(stderr);
            let _ = stderr.write_all(lintel_cli::USAGE.as_bytes());
            ExitCode::from(2)
        }
    }
}
