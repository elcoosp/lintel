//! The M2 cold vs warm build-throughput bench (architecture doc §9.3's
//! "cold & warm" metric — warm (cache hit) must be I/O-bound): the same
//! 24-source workspace builds three ways through the public engine entry
//! (`lintl_cli::run`, the exact code path the binary drives) — cold (the
//! `.lintl-cache` wiped, everything compiles and stores), warm (every
//! source is a cache hit; only output writes remain) and bypassed
//! (`--no-cache`, the honest cold shape).
//!
//! Run: `cargo bench -p lintl-cli --bench build_cache`. Smoke-run under
//! the test harness with `cargo test --benches`.

use std::fmt::Write as _;
use std::fs;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};

/// One fixture source: a ~40-line component with a unique name per index
/// (so the sources differ by content, not only by path).
fn source(index: usize) -> String {
    let mut src = format!(
        "<?php\n\nuse Lintl\\Html;\n\nfunction Page{index}(string $title, array $rows): Html\n{{\n    return <main class=\"page page{index}\">\n        <h1>{{$title}}</h1>\n"
    );
    for row in 0..12 {
        let _ = writeln!(
            src,
            "        <Row n={{$rows[{row}]}} active={{$rows[{row}] % 2 == 0}}><b>cell {index}-{row}</b> \
             <a href={{$links[{row}] ?? '/'}}>go</a></Row>"
        );
    }
    src.push_str("    </main>;\n}\n");
    src
}

/// Lay the fixture workspace out: `files` sources under `src/`.
fn fixture(workspace: &Path, files: usize) -> Vec<PathBuf> {
    let dir = workspace.join("src");
    fs::create_dir_all(&dir).expect("creates src/");
    (0..files)
        .map(|index| {
            let path = dir.join(format!("Page{index}.lintl.php"));
            fs::write(&path, source(index)).expect("writes the source");
            path
        })
        .collect()
}

fn bench_build(c: &mut Criterion) {
    let workspace = std::env::temp_dir().join(format!("lintl-bench-cache-{}", std::process::id()));
    let _ = fs::remove_dir_all(&workspace);
    let sources = fixture(&workspace, 24);
    let out = workspace.join("out");

    let argv = |cache: bool| -> Vec<String> {
        let mut args: Vec<String> = vec!["build".into()];
        args.extend(sources.iter().map(|path| path.display().to_string()));
        args.push("--out".into());
        args.push(out.display().to_string());
        if !cache {
            args.push("--no-cache".into());
        }
        args
    };
    let run = |args: &[String]| {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = lintl_cli::run(args, &mut stdout, &mut stderr).expect("arguments parse");
        assert_eq!(code, 0, "the bench build compiles cleanly");
        black_box(stdout);
    };

    let mut group = c.benchmark_group("build_cache");
    group.sample_size(30);
    group.warm_up_time(Duration::from_millis(200));
    group.measurement_time(Duration::from_secs(2));

    // Cold: wipe the cache, compile everything, store the entries.
    group.bench_function("cold_24_files", |b| {
        b.iter(|| {
            let _ = fs::remove_dir_all(workspace.join(".lintl-cache"));
            run(&argv(true));
        });
    });
    // Warm: the cache is fully populated — every source is a hit, only
    // the artifact writes remain (the §9.3 "I/O-bound" claim under test).
    group.bench_function("warm_24_files", |b| {
        b.iter(|| run(&argv(true)));
    });
    // Bypass: `--no-cache` — the compile cost the cache removes.
    group.bench_function("no_cache_24_files", |b| {
        b.iter(|| run(&argv(false)));
    });

    group.finish();
    let _ = fs::remove_dir_all(&workspace);
}

criterion_group!(benches, bench_build);
criterion_main!(benches);
