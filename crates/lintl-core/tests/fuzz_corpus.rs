//! The Phase 12 M2 acceptance: the `cargo fuzz` corpus seeds from the
//! boundaries the other gates already prove — the differential
//! acceptance's 220 generated templates (deterministically replayed
//! through the same proptest strategies `differential.rs` runs, so the
//! corpus and the acceptance can never drift apart) and every isolated
//! reconciliation case — plus the tree-sitter lane's fallback edge
//! shapes (the malformed declaration header, the declaration after a
//! markup region) and the §7.2 threat-corpus payload vectors.
//!
//! `cargo fuzz run compile_fuzz` picks `fuzz/corpus/compile_fuzz/` up as
//! its initial corpus (the directory is a generated cache: gitignored,
//! rebuilt here, excluded from source archives), so the fuzzer starts
//! from known-good and known-bad boundaries instead of an empty
//! directory. Seed-time invariants, proven per seed: compiling on all
//! three targets never panics and never loops (refusal-by-diagnostics is
//! a fine seed — the hostile shapes are the point), the directory holds
//! exactly the seeded set, and no seed is empty.

use std::fs;
use std::path::{Path, PathBuf};

use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::{RngAlgorithm, TestRng, TestRunner};

use lintl_core::{compile, Options, Target};

#[path = "differential/cases.rs"]
mod cases;

use cases::{case_strategy, iso_cases, IsoCase};

/// The generated-template seeds: the differential acceptance's
/// `DIFFERENTIAL_CASES` count (§13.2's "≥ 200 generated templates").
const GENERATED_SEEDS: usize = 220;

/// Where cargo-fuzz looks for the initial corpus: `<repo>/fuzz/corpus/<target>`.
fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .join("fuzz")
        .join("corpus")
        .join("compile_fuzz")
}

/// The generated differential sources, replayed deterministically: the
/// same strategy [`cases::case_strategy`] gives the acceptance, drawn
/// through a seeded runner (`ChaCha`, proptest's hard-coded deterministic
/// seed) so every machine and CI run materializes the same 220 sources.
fn generated_sources(count: usize) -> Vec<String> {
    let mut runner = TestRunner::new_with_rng(
        ProptestConfig::with_cases(0),
        TestRng::deterministic_rng(RngAlgorithm::ChaCha),
    );
    let strategy = case_strategy();
    (0..count)
        .map(|index| {
            let case = strategy
                .new_tree(&mut runner)
                .unwrap_or_else(|reason| panic!("generated seed {index}: {reason}"))
                .current();
            // The payload the acceptance drives the reference renderer with:
            // serializable and non-empty, or the corpus would seed sources
            // whose harness materialization has drifted.
            assert!(
                !serde_json::to_string(&case.payload)
                    .expect("generated payload serializes")
                    .is_empty(),
                "generated seed {index}: the payload is empty"
            );
            case.source.clone()
        })
        .collect()
}

/// The isolated reconciliation cases' sources (§21.2's divergence
/// classes, kept forever as regressions): the same sources the
/// acceptance compares against the reference renderer.
fn isolated_sources() -> Vec<(String, String)> {
    iso_cases()
        .into_iter()
        .map(|case| match case {
            IsoCase::Compare {
                name,
                source,
                payload,
            } => {
                assert!(
                    !serde_json::to_string(&payload)
                        .expect("isolated payload serializes")
                        .is_empty(),
                    "{name}: the payload is empty"
                );
                (name, source)
            }
            IsoCase::Reject { name, source, code } => {
                assert!(
                    code.starts_with("LNL"),
                    "{name}: the rejection must pin an LNL code, got {code}"
                );
                (name, source)
            }
        })
        .collect()
}

/// The tree-sitter lane's fallback edge shapes (the Phase 11 M1 probes,
/// `crates/lintl-lsp/src/tree_sitter.rs`): the declaration header the
/// PHP grammar cannot recover (`(***`), the declaration error recovery
/// swallows after a markup region, and the unclosed element under a
/// declaration. The fuzz target drives these through the LSP session's
/// merged scanner+tree-sitter view; the corpus starts from the shapes
/// that lane exists for.
fn tree_sitter_edge_sources() -> Vec<(&'static str, String)> {
    vec![
        (
            "ts-fallback-broken-header",
            concat!(
                "<?php\nfunction Badge(***): Html\n{\n",
                "    return <b>{$label}</b>;\n}\n"
            )
            .to_string(),
        ),
        (
            "ts-recovery-declaration-after-markup",
            concat!(
                "<?php\nfunction Page(string $title): Html\n{\n",
                "    return <html><body><Badge label={$title} />\n",
                "    </body></html>;\n}\n\n",
                "function Grid(int $rows): Html\n{\n",
                "    return <table>{$rows}</table>;\n}\n"
            )
            .to_string(),
        ),
        (
            "ts-fallback-unclosed-element",
            "<?php\nfunction Page(): Html\n{\n    return <div><span>oops;\n}\n".to_string(),
        ),
        (
            "ts-fallback-markup-only",
            "<?php\n<html><body><p>static page</p></body></html>\n".to_string(),
        ),
        (
            "ts-fallback-broken-params-with-markup",
            concat!(
                "<?php\nfunction Broken(***): Html\n{\n",
                "    return <div><b>{$x}</b> && <<<EOS\nheredoc soup\nEOS;\n}\n"
            )
            .to_string(),
        ),
    ]
}

/// The §7.2 threat-corpus payload vectors, as compiler inputs: each
/// payload rides a dynamic URL attribute (the `u()` lane), a dynamic
/// child (the `c()` lane) and, where the vector is attribute-shaped, a
/// quoted-attribute fold. These are the known-bad boundaries the fuzzer
/// must keep refusing — never emit raw — when it mutates around them.
fn threat_sources() -> Vec<(String, String)> {
    const SCHEMES: &[(&str, &str)] = &[
        ("scheme-tab", "java\tscript:alert(1)"),
        ("scheme-newline", "java\nscript:alert(1)"),
        ("scheme-nul", "java\0script:alert(1)"),
        ("scheme-mixed-case", "JaVaScRiPt:alert(1)"),
        ("scheme-entity", "&#x6A;avascript:alert(1)"),
        (
            "scheme-data",
            "data:text/html;base64,PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg==",
        ),
        ("scheme-vbscript", "vbscript:msgbox(1)"),
        ("scheme-space", " javascript:x"),
        ("scheme-crlf", "java\rscript:alert(1)"),
    ];
    let mut sources = Vec::new();
    // Every scheme vector: dynamic URL attribute + dynamic child.
    for (name, payload) in SCHEMES {
        sources.push((
            format!("threat-url-{name}"),
            format!(
                "<?php\nfunction T($p) {{ return <a href={{'{}'}}>t</a>; }}\n",
                payload.replace('\'', "\\'")
            ),
        ));
        sources.push((
            format!("threat-child-{name}"),
            format!(
                "<?php\nfunction T($p) {{ return <p>{{'{}'}}</p>; }}\n",
                payload.replace('\'', "\\'")
            ),
        ));
    }
    // The positive boundary (§7.2 testAllowedSchemesAndRelativeUrlsPass):
    // allowed schemes and relative URLs pass through as written.
    sources.push((
        "threat-url-allowlist-pass".to_string(),
        "<?php\nfunction T($p) { return <a href={'https://lintl.sh/p?x=1&y=2'}>t</a>; }\n"
            .to_string(),
    ));
    // Attribute breakout: the quote must escape inside the attribute.
    sources.push((
        "threat-attr-breakout".to_string(),
        "<?php\nfunction T($p) { return <b title={'\" onmouseover=\"alert(1)'}>t</b>; }\n"
            .to_string(),
    ));
    // `</script>` as a child value and inside a raw-text JSON island.
    sources.push((
        "threat-script-close-child".to_string(),
        "<?php\nfunction T($p) { return <p>{'</script><script>alert(1)</script>'}</p>; }\n"
            .to_string(),
    ));
    sources.push((
        "threat-raw-text-json-island".to_string(),
        "<?php\nfunction T($p) { return <script>{\"a\": 1}</script>; }\n".to_string(),
    ));
    // NUL bytes around markup, and the UTF-8 substitution character.
    sources.push((
        "threat-nul-child".to_string(),
        "<?php\nfunction T($p) { return <p>{\"a\\0b<script>alert(1)</script>\"}</p>; }\n"
            .to_string(),
    ));
    sources.push((
        "threat-replacement-char".to_string(),
        "<?php\nfunction T($p) { return <p>{\"\u{FFFD}<script>\"}</p>; }\n".to_string(),
    ));
    // Spread routing: the `on*` key is refused outright (LNL0301), the
    // srcset key routes per-URL.
    sources.push((
        "threat-spread-onclick-refused".to_string(),
        "<?php\nfunction T($p) { return <div {...['onclick' => 'alert(1)']}>t</div>; }\n"
            .to_string(),
    ));
    sources.push((
        "threat-spread-srcset-hostile".to_string(),
        "<?php\nfunction T($p) { return <div {...['srcset' => 'a.png 1x, javascript:alert(1) 2x']}>t</div>; }\n"
            .to_string(),
    ));
    sources.push((
        "threat-srcset-attr-hostile".to_string(),
        "<?php\nfunction T($p) { return <img srcset={'safe.png 1x, vbscript:x 2x'} />; }\n"
            .to_string(),
    ));
    // Stringable-shaped markup must never pass through raw.
    sources.push((
        "threat-stringable-img".to_string(),
        "<?php\nfunction T($p) { return <p>{$p}</p>; }\n".to_string(),
    ));
    // §10 streaming: a hostile payload inside a boundary child (the real
    // Suspense shape the §7.2 streaming threat cases use).
    sources.push((
        "threat-suspense-late-payload".to_string(),
        concat!(
            "<?php\nfunction T($p): \\Generator {\n",
            "    yield new Suspense('<img src=x onerror=alert(1)>', static fn (): string => 'ok');\n}\n"
        )
        .to_string(),
    ));
    // Style charset: hostile value inside the style map.
    sources.push((
        "threat-style-charset".to_string(),
        "<?php\nfunction T($p) { return <p style={['background' => 'url(\"java\\tscript:x\")']}>t</p>; }\n"
            .to_string(),
    ));
    // Namespace/name-shape probes the validator rejects (LNL0202 lane).
    sources.push((
        "threat-hyphenated-prop-refused".to_string(),
        "<?php\nfunction T($p) { return <Card data-x=\"1\" />; }\n".to_string(),
    ));
    sources
}

/// Seed every source into the corpus after proving it compiles (or is
/// refused by diagnostics) on all three targets without panicking.
fn seed(sources: &[(String, String)], dir: &Path) -> usize {
    for (name, source) in sources {
        for target in [Target::String, Target::Stream, Target::Rfc] {
            // Refusal is a fine seed (the hostile shapes); a panic or a
            // hang here is a compiler bug the fuzz run would find anyway —
            // but the corpus must not START from a crashing seed.
            let _ = compile(source, &Options { target });
        }
        assert!(!source.is_empty(), "{name}: the seed is empty");
        let path = dir.join(format!("{name}.lintl.php"));
        fs::write(&path, source).unwrap_or_else(|e| panic!("{name}: write the seed: {e}"));
    }
    sources.len()
}

#[test]
fn fuzz_corpus_seeds_the_differential_ts_and_threat_boundaries() {
    let dir = corpus_dir();
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("creates the corpus dir");

    let generated = generated_sources(GENERATED_SEEDS);
    let isolated = isolated_sources();
    let ts_edges = tree_sitter_edge_sources();
    let threats = threat_sources();

    let mut seeds: Vec<(String, String)> = Vec::new();
    for (index, source) in generated.iter().enumerate() {
        seeds.push((format!("differential-{index:03}"), source.clone()));
    }
    for (name, source) in &isolated {
        seeds.push((format!("iso-{name}"), source.clone()));
    }
    for (name, source) in &ts_edges {
        seeds.push(((*name).to_string(), source.clone()));
    }
    for (name, source) in &threats {
        seeds.push((name.clone(), source.clone()));
    }

    let seeded = seed(&seeds, &dir);

    // The counts: the differential acceptance's exact set, the isolated
    // reconciliation set (23 — §21.2's "24" was a counting slip; the
    // docs-sweep card fixes the prose), the fallback-edge shapes, and the
    // §7.2 payload coverage (the corpus's 31 tests, mirrored source-shaped).
    assert_eq!(generated.len(), GENERATED_SEEDS);
    assert_eq!(isolated.len(), 23, "the acceptance's isolated case count");
    assert!(ts_edges.len() >= 5, "the fallback-edge shapes");
    assert!(threats.len() >= 31, "the §7.2 payload vectors, mirrored");
    assert_eq!(
        seeded,
        GENERATED_SEEDS + isolated.len() + ts_edges.len() + threats.len()
    );

    // Exactly the seeded set is on disk — no strays from earlier runs.
    let mut files: Vec<String> = fs::read_dir(&dir)
        .expect("corpus dir readable")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    files.sort();
    let mut expected: Vec<String> = seeds
        .iter()
        .map(|(name, _)| format!("{name}.lintl.php"))
        .collect();
    expected.sort();
    assert_eq!(files, expected, "the corpus holds exactly the seeded set");

    // Every seed landed with its bytes (the fuzzer reads them verbatim).
    for (name, source) in &seeds {
        let path = dir.join(format!("{name}.lintl.php"));
        let bytes = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{name}: readable: {e}"));
        assert_eq!(bytes, *source, "{name}: the seed's bytes are verbatim");
    }
}
