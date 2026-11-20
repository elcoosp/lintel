//! The T-13 size budget (acceptance, verbatim: "loads <1 MB gz"): the
//! `wasm32-unknown-unknown` bundle built `opt-level=z` + fat LTO +
//! `strip = true` — and the whole playground payload with it — must gzip to
//! under 1 MB. The core crate is dependency-free, so the only additions on
//! the wasm path are this crate's thin `wasm-bindgen` seam.
//!
//! The test builds its own artifact in an isolated target dir (a nested
//! `cargo` invocation must not fight the outer one for the workspace's
//! target lock), binds it with `wasm-bindgen --target web`, and gzips the
//! result. It needs the `wasm32-unknown-unknown` rust target and a
//! `wasm-bindgen-cli` matching the locked `wasm-bindgen` crate version, and
//! runs only under `LINTEL_WASM_SIZE_CHECK=1` — the plain rust/php/dx/bench
//! CI jobs (and wasm-less machines) keep passing `cargo test` with a skip
//! note, while the dedicated `wasm` CI job and `make size-budget` set the
//! variable and always run the assertion.

use std::path::{Path, PathBuf};
use std::process::Command;

/// One megabyte, the acceptance budget.
const BUDGET: u64 = 1_000_000;

/// The gate variable: the test builds a real wasm artifact, so it opts in.
const GATE_VAR: &str = "LINTEL_WASM_SIZE_CHECK";

/// Resolve the wasm-bindgen CLI: `WASM_BINDGEN` overrides the default on
/// `PATH`.
fn bindgen_bin() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("WASM_BINDGEN") {
        let path = PathBuf::from(path);
        return path.is_file().then_some(path);
    }
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|dir| dir.join("wasm-bindgen"))
        .find(|candidate| candidate.is_file())
}

/// The `wasm-bindgen` version locked in the workspace `Cargo.lock` — the
/// CLI must match it exactly or the binding step fails at runtime with a
/// version-mismatch panic.
fn locked_bindgen_version() -> String {
    let lock = Path::new(&env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock");
    let text = std::fs::read_to_string(&lock).expect("read Cargo.lock");
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        if line.trim() == "name = \"wasm-bindgen\"" {
            let version = lines.next().expect("version follows the name");
            let value = version
                .trim()
                .strip_prefix("version = \"")
                .expect("version line shape")
                .strip_suffix('"')
                .expect("version line shape");
            return value.to_string();
        }
    }
    panic!("Cargo.lock does not pin wasm-bindgen");
}

/// Build the cdylib for wasm32 under the size-optimized profile, in an
/// isolated target dir; returns the raw `.wasm` path.
fn build_wasm(out_dir: &Path) -> PathBuf {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let manifest = Path::new(&env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let status = Command::new(&cargo)
        .args([
            "build",
            "--manifest-path",
            manifest.to_str().expect("utf8 manifest path"),
            "--profile",
            "wasm-release",
            "--target",
            "wasm32-unknown-unknown",
        ])
        .env("CARGO_TARGET_DIR", out_dir)
        .status()
        .expect("spawn cargo for the wasm build");
    assert!(status.success(), "cargo build (wasm-release) failed");
    let raw = out_dir
        .join("wasm32-unknown-unknown")
        .join("wasm-release")
        .join("lintel_wasm.wasm");
    assert!(
        raw.is_file(),
        "the wasm cdylib is missing at {}",
        raw.display()
    );
    raw
}

/// Bind the raw wasm into the web artifact the playground loads; returns
/// the `_bg.wasm` path.
fn bind_wasm(bindgen: &Path, raw: &Path, out_dir: &Path) -> PathBuf {
    let status = Command::new(bindgen)
        .args(["--target", "web", "--no-typescript", "--out-dir"])
        .arg(out_dir)
        .arg(raw)
        .status()
        .expect("spawn wasm-bindgen");
    assert!(status.success(), "wasm-bindgen failed");
    let bound = out_dir.join("lintel_wasm_bg.wasm");
    assert!(
        bound.is_file(),
        "the bound wasm is missing at {}",
        bound.display()
    );
    bound
}

/// Gzipped byte count of one file, via the `gzip` CLI (`-9` matches how a
/// web server with gzip/brotli would ship it as close as this test can get
/// without a dependency).
fn gzipped_len(file: &Path) -> u64 {
    let output = Command::new("gzip")
        .arg("-9")
        .arg("-c")
        .arg(file)
        .output()
        .expect("spawn gzip");
    assert!(
        output.status.success(),
        "gzip failed for {}",
        file.display()
    );
    u64::try_from(output.stdout.len()).expect("gzip output fits in u64")
}

#[test]
fn playground_bundle_gzips_under_one_megabyte() {
    if std::env::var(GATE_VAR).is_err() {
        eprintln!(
            "skip lintel-wasm/size_budget: set {GATE_VAR}=1 (make size-budget or the CI wasm job)"
        );
        return;
    }
    let Some(bindgen) = bindgen_bin() else {
        eprintln!("skip lintel-wasm/size_budget: no wasm-bindgen CLI (set WASM_BINDGEN)");
        return;
    };
    let locked = locked_bindgen_version();
    let version = String::from_utf8_lossy(
        &Command::new(&bindgen)
            .arg("--version")
            .output()
            .expect("spawn wasm-bindgen --version")
            .stdout,
    )
    .trim()
    .to_string();
    assert!(
        version.ends_with(&locked),
        "wasm-bindgen CLI `{version}` must match the locked crate version {locked}"
    );

    let out_dir = std::env::temp_dir().join(format!("lintel-wasm-size-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out_dir);
    std::fs::create_dir_all(&out_dir).expect("temp build dir");
    let raw = build_wasm(&out_dir);
    let bound = bind_wasm(&bindgen, &raw, &out_dir);

    let wasm_gz = gzipped_len(&bound);
    assert!(
        wasm_gz < BUDGET,
        "the bound wasm gzips to {wasm_gz} bytes — over the {BUDGET}-byte budget"
    );

    // The whole page payload (what "loads <1 MB gz" actually means): the
    // wasm plus every static asset the page requests.
    let site = Path::new(&env!("CARGO_MANIFEST_DIR")).join("../../playground");
    let mut total = wasm_gz;
    for asset in [
        "index.html",
        "styles.css",
        "app.js",
        "sw.js",
        "assets/lintel_wasm.js",
    ] {
        let file = site.join(asset);
        assert!(file.is_file(), "playground asset missing: {asset}");
        total += gzipped_len(&file);
    }
    assert!(
        total < BUDGET,
        "the whole playground payload gzips to {total} bytes — over the {BUDGET}-byte budget"
    );
}
