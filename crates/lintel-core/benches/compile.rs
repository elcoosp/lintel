//! Compile-throughput benches (architecture doc 9.3 hypotheses).
//!
//! Run: `cargo bench -p lintel-core`. Smoke-run under the test harness with
//! `cargo test --benches`.

use std::fmt::Write as _;
use std::hint::black_box;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};
use lintel_core::{builtin_rules, compile, compile_with, Options, Target};

/// ~200 lines of plain PHP with strings, comments, heredocs and no JSX.
fn passthrough_source() -> String {
    let mut src = String::from("<?php\ndeclare(strict_types=1);\n\n");
    for i in 0..40 {
        let _ = write!(
            src,
            "// row {i}: renders <b>{i}</b> rows via {{$rows}} &lt;helper&gt;\n\
             $row{i} = \"<tr class=\\\"r{i}\\\">{{$data['c{i}']}} & {{$other[i]}}</tr>\";\n\
             $n{i} = <<<EOT\n  <li>{{$row{i}}}</li>\n  EOT;\n\
             if ($n{i} < 10 && $n{i} > 0) {{ $sum += $n{i} <=> $base; }}\n\n"
        );
    }
    src
}

/// A component tree: depth 4 x fan-out 3 = 40 elements, mixed static/dynamic.
fn component_source() -> String {
    let mut src = String::from(
        "<?php\nfunction Page(string $user, array $items): Html {\n    return <PageShell user={$user}>\n",
    );
    for i in 0..3 {
        let _ = writeln!(src, "        <Section title=\"S{i}\" open>");
        for j in 0..3 {
            let _ = writeln!(
                src,
                "            <Row key={{j}} active={{j % 2 == 0}}><b>cell {i}-{j}</b> \
                 <a href={{$links[{j}]}}>go</a></Row>"
            );
        }
        src.push_str("        </Section>\n");
    }
    src.push_str("    </PageShell>;\n}\n");
    src
}

/// A static-heavy page: 95% literal markup with a handful of dynamic slots.
fn static_heavy_source() -> String {
    let mut src = String::from(
        "<?php\nfunction Landing(string $title): Html {\n    return <main class=\"landing\">\n",
    );
    for i in 0..60 {
        let _ = writeln!(
            src,
            "        <div class=\"card card--{i}\"><h3>Headline {i}</h3>\
             <p>Body copy stays literal &amp; escaped.</p></div>"
        );
    }
    src.push_str("        <footer>{$title}</footer>\n    </main>;\n}\n");
    src
}

/// A component tree whose handlers are all dynamic: measures the scan +
/// validate rejection path (LNT0301) against the clean compile cost.
fn rejected_handlers_source() -> String {
    let mut src = String::from(
        "<?php\nfunction Panel(string $user, array $events): Html {\n    return <section class=\"panel\">\n",
    );
    for i in 0..20 {
        let _ = writeln!(
            src,
            "        <button onclick={{$events[{i}]}} onmouseover={{$hover[{i}]}}>action {i}</button>"
        );
    }
    src.push_str("    </section>;\n}\n");
    src
}

/// Static-heavy handlers: every `onclick` is a static string, so each file
/// compiles successfully but carries an `LNT0302` warning — measures the
/// warn-and-continue validation path against the clean compile cost.
fn warned_handlers_source() -> String {
    let mut src = String::from(
        "<?php\nfunction Panel(string $user): Html {\n    return <section class=\"panel\">\n",
    );
    for i in 0..20 {
        let _ = writeln!(
            src,
            "        <button onclick=\"track('row-{i}')\">action {i}</button>"
        );
    }
    src.push_str("    </section>;\n}\n");
    src
}

/// The same warned-handlers file, but every `onclick` line carries a
/// `lintel-ignore-next` directive on the line above — measures the
/// suppression-window capture and per-diagnostic window lookup
/// (`docs/ignore-directives.md`) against the unsuppressed cost.
fn suppressed_handlers_source() -> String {
    let mut src = String::from(
        "<?php\nfunction Panel(string $user): Html {\n    return <section class=\"panel\">\n",
    );
    for i in 0..20 {
        let _ = writeln!(src, "        // lintel-ignore-next: legacy handler {i}");
        let _ = writeln!(
            src,
            "        <button onclick=\"track('row-{i}')\">action {i}</button>"
        );
    }
    src.push_str("    </section>;\n}\n");
    src
}

/// The same warned-handlers file, but the first line carries the file-level
/// `// lintel-ignore-file` directive (`docs/ignore-directives.md`) — the
/// validation pass short-circuits before the walk, so this measures the
/// suppressed-file floor against the unsuppressed `warned_handlers` cost.
fn file_suppressed_handlers_source() -> String {
    let mut src = String::from(
        "<?php\n// lintel-ignore-file: legacy CTA page, audited in CMS-77\n\
         function Panel(string $user): Html {\n    return <section class=\"panel\">\n",
    );
    for i in 0..20 {
        let _ = writeln!(
            src,
            "        <button onclick=\"track('row-{i}')\">action {i}</button>"
        );
    }
    src.push_str("    </section>;\n}\n");
    src
}

/// Component invocations checked against an in-file signature (doc §17.1):
/// every prop matches a declared parameter, so the file compiles cleanly —
/// measures the signature-capture and prop-lookup path against the
/// unchecked cost of undeclared components.
fn signature_checked_source() -> String {
    let mut src = String::from(
        "<?php\nfunction Row(string $label, bool $active = false): Html {\n    return <b>{$label}</b>;\n}\n",
    );
    for i in 0..20 {
        let _ = writeln!(src, "echo <Row label=\"cell {i}\" active />;");
    }
    src
}

/// An image gallery whose `<img>` tags carry dynamic `srcset` + `src`
/// attributes: measures the per-URL `srcset` routing path (`\Lintel\srcset()`
/// emission, `docs/srcset-alignment.md`) against the plain attribute helper
/// cost of `static_heavy`'s img-free markup.
fn srcset_attrs_source() -> String {
    let mut src = String::from(
        "<?php\nfunction Gallery(array $variants, array $sources): Html {\n    return <main class=\"gallery\">\n",
    );
    for i in 0..20 {
        let _ = writeln!(
            src,
            "        <img src={{$sources[{i}]}} srcset={{$variants[{i}]}} alt=\"photo {i}\" />"
        );
    }
    src.push_str("    </main>;\n}\n");
    src
}

/// A stats page whose dynamic children are all provably scalar — `(int)`
/// casts and `count()` calls (doc §7.3 escape elision, `docs/escape-elision.md`):
/// measures the elision path (raw-expression folding, no `\Lintel\c()`)
/// against the helper-wrapped `static_heavy` dynamic slots.
fn elided_children_source() -> String {
    let mut src = String::from(
        "<?php\nfunction Stats(array $rows, array $counts): Html {\n    return <main class=\"stats\">\n",
    );
    for i in 0..20 {
        let _ = writeln!(
            src,
            "        <p>{{(int)$counts[{i}]}} of {{count($rows)}} rows rendered</p>"
        );
    }
    src.push_str("    </main>;\n}\n");
    src
}

/// A profile page whose dynamic children are all `string`-typed parameters
/// of the enclosing function (doc §7.3 typed-parameter elision,
/// `docs/escape-elision.md`): measures the direct-`\htmlspecialchars()` fold
/// path (no `\Lintel\c()` wrapper) against the helper-wrapped cost of the
/// same children before the scope tracking existed.
fn typed_children_source() -> String {
    let mut src = String::from(
        "<?php\nfunction Profile(string $name, string $tagline, string $role): Html {\n    return <main class=\"profile\">\n",
    );
    for i in 0..20 {
        let _ = writeln!(
            src,
            "        <p class=\"line-{i}\"><b>{{$name}}</b> wrote {{$tagline}} as {{$role}}</p>"
        );
    }
    src.push_str("    </main>;\n}\n");
    src
}

/// The same signature-checked file, but every invocation omits the required
/// `$label` parameter — measures the required-parameter rejection path
/// (LNT0205) against the clean `signature_checked` cost.
fn required_missing_source() -> String {
    let mut src = String::from(
        "<?php\nfunction Row(string $label, bool $active = false): Html {\n    return <b>{$label}</b>;\n}\n",
    );
    for _ in 0..20 {
        let _ = writeln!(src, "echo <Row active />;");
    }
    src
}

/// A policy rule for the `custom_rule` scenario: every `<img>` must carry
/// an `alt` attribute (reuses the closest stable class, `LNT0103`, per the
/// `ValidationRule` trait docs). Holds no state, so it is `Sync` and can
/// live in the static registry slice.
struct RequireAltBench;

impl lintel_core::ValidationRule for RequireAltBench {
    fn check(
        &self,
        node: &lintel_core::ast::Node,
        _context: &lintel_core::scan::ScanContext,
        out: &mut Vec<lintel_core::Diagnostic>,
    ) {
        let lintel_core::ast::Node::Element(element) = node else {
            return;
        };
        if element.name == "img"
            && !element.attrs.iter().any(
                |attr| matches!(attr, lintel_core::ast::Attr::Str { name, .. } if name == "alt"),
            )
        {
            out.push(lintel_core::Diagnostic::new(
                lintel_core::Code::BadAttr,
                "`<img>` is missing its `alt` attribute",
                element.offset,
            ));
        }
    }
}

fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("compile");
    group.sample_size(30);
    group.warm_up_time(Duration::from_millis(200));
    group.measurement_time(Duration::from_secs(2));

    group.bench_function("passthrough", |b| {
        let src = passthrough_source();
        b.iter(|| compile(black_box(&src), black_box(&Options::default())));
    });
    group.bench_function("component_tree", |b| {
        let src = component_source();
        b.iter(|| compile(black_box(&src), black_box(&Options::default())));
    });
    group.bench_function("rfc_component_tree", |b| {
        let src = component_source();
        let opts = Options {
            target: Target::Rfc,
        };
        b.iter(|| compile(black_box(&src), black_box(&opts)));
    });
    group.bench_function("stream_component_tree", |b| {
        let src = component_source();
        let opts = Options {
            target: Target::Stream,
        };
        b.iter(|| compile(black_box(&src), black_box(&opts)));
    });
    group.bench_function("static_heavy", |b| {
        let src = static_heavy_source();
        b.iter(|| compile(black_box(&src), black_box(&Options::default())));
    });
    group.bench_function("srcset_attrs", |b| {
        let src = srcset_attrs_source();
        b.iter(|| compile(black_box(&src), black_box(&Options::default())));
    });
    group.bench_function("elided_children", |b| {
        let src = elided_children_source();
        b.iter(|| compile(black_box(&src), black_box(&Options::default())));
    });
    group.bench_function("typed_children", |b| {
        let src = typed_children_source();
        // Path drift-check (the §21.2 lesson): the fold must actually fire,
        // or the bench silently measures the wrapped path again.
        let out = compile(&src, &Options::default()).expect("typed children compile");
        assert!(
            out.php.contains("\\htmlspecialchars($name,"),
            "typed children elide: {}",
            out.php
        );
        assert!(!out.php.contains("\\Lintel\\c($name)"), "{}", out.php);
        b.iter(|| compile(black_box(&src), black_box(&Options::default())));
    });
    group.bench_function("rejected_handlers", |b| {
        let src = rejected_handlers_source();
        b.iter(|| compile(black_box(&src), black_box(&Options::default())));
    });
    group.bench_function("warned_handlers", |b| {
        let src = warned_handlers_source();
        b.iter(|| compile(black_box(&src), black_box(&Options::default())));
    });
    group.bench_function("file_suppressed_handlers", |b| {
        let src = file_suppressed_handlers_source();
        b.iter(|| compile(black_box(&src), black_box(&Options::default())));
    });
    group.bench_function("suppressed_handlers", |b| {
        let src = suppressed_handlers_source();
        b.iter(|| compile(black_box(&src), black_box(&Options::default())));
    });
    group.bench_function("signature_checked", |b| {
        let src = signature_checked_source();
        b.iter(|| compile(black_box(&src), black_box(&Options::default())));
    });
    group.bench_function("required_missing", |b| {
        let src = required_missing_source();
        b.iter(|| compile(black_box(&src), black_box(&Options::default())));
    });
    // The rule seam: same component tree through `compile_with` with the
    // built-in registry plus one extra custom rule — measures the injected
    // registry cost against the hardcoded `component_tree` path.
    group.bench_function("custom_rule", |b| {
        let src = component_source();
        let rules: Vec<&dyn lintel_core::ValidationRule> = builtin_rules()
            .iter()
            .copied()
            .chain([&RequireAltBench as &dyn lintel_core::ValidationRule])
            .collect();
        b.iter(|| {
            compile_with(
                black_box(&src),
                black_box(&Options::default()),
                black_box(&rules),
            )
        });
    });

    group.finish();
}

/// `SourceMap` position-lookup bench (the offset→line:col seam behind every
/// rendered diagnostic): one map over the component tree source, resolving
/// the offset of every tag-open in the file — the per-diagnostic work a CLI
/// or LSP consumer does.
fn sourcemap_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("sourcemap");
    group.sample_size(30);
    group.warm_up_time(Duration::from_millis(200));
    group.measurement_time(Duration::from_secs(2));

    group.bench_function("line_col_lookup", |b| {
        let src = component_source();
        let offsets: Vec<usize> = src
            .match_indices('<')
            .filter(
                |(at, _)| matches!(src.as_bytes().get(at + 1), Some(c) if c.is_ascii_alphabetic()),
            )
            .map(|(at, _)| at)
            .collect();
        b.iter(|| {
            let map = lintel_diag::SourceMap::new(black_box(&src));
            for offset in &offsets {
                black_box(map.line_col(*offset));
            }
        });
    });

    group.finish();
}

/// Machine-readable diagnostic emission (the `--format=json` CLI path, built
/// on the `lintel_diag::Diagnostic::to_json` seam — see
/// `docs/diagnostics-json.md`): compile the warned-handlers file, then emit
/// one JSON object per warning — the per-diagnostic cost a CI log or editor
/// pipe pays, `SourceMap` position lookup and escaping included.
fn json_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("json");
    group.sample_size(30);
    group.warm_up_time(Duration::from_millis(200));
    group.measurement_time(Duration::from_secs(2));

    group.bench_function("warning_emission", |b| {
        let src = warned_handlers_source();
        let warnings: Vec<lintel_core::Diagnostic> =
            compile(black_box(&src), black_box(&Options::default()))
                .expect("warned handlers compile")
                .warnings;
        // The source builders drift-checked (rustc accepts a lone `}` in a
        // format string, which once turned these static strings into dynamic
        // handlers and silently measured the rejection path instead).
        assert_eq!(warnings.len(), 20, "warned handlers carry 20 LNT0302");
        b.iter(|| {
            for warning in &warnings {
                black_box(warning.to_json(black_box(&src)));
            }
        });
    });

    group.finish();
}

/// The `NodeVisitor` read traversal (`docs/node-visitor.md` seam): walk the
/// component tree counting elements — the pure-AST read cost an LSP or
/// metrics consumer pays, with no lowering and no validation work.
struct ElementCounter {
    elements: usize,
}

impl lintel_core::NodeVisitor for ElementCounter {
    fn visit(&mut self, node: &lintel_core::ast::Node) {
        if matches!(node, lintel_core::ast::Node::Element(_)) {
            self.elements += 1;
        }
    }
}

fn visitor_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("visitor");
    group.sample_size(30);
    group.warm_up_time(Duration::from_millis(200));
    group.measurement_time(Duration::from_secs(2));

    group.bench_function("element_count_walk", |b| {
        let src = component_source();
        let parsed = lintel_core::scan::scan(black_box(&src));
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert_eq!(parsed.roots.len(), 1, "the component tree is one root");
        b.iter(|| {
            let mut counter = ElementCounter { elements: 0 };
            lintel_core::visit_roots(black_box(&parsed.roots), &mut counter);
            black_box(counter.elements);
        });
    });

    group.finish();
}

/// The component-inventory report (`docs/component-inventory.md` seam): the
/// declared-signature table plus the invocation walk over the component
/// tree — the shape-report cost an LSP outline or metrics collector pays,
/// with no diagnostics and no lowering output.
fn inventory_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("inventory");
    group.sample_size(30);
    group.warm_up_time(Duration::from_millis(200));
    group.measurement_time(Duration::from_secs(2));

    group.bench_function("component_inventory", |b| {
        let src = component_source();
        let parsed = lintel_core::scan::scan(black_box(&src));
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        // Path drift-check (the §21.2 lesson): the report must actually find
        // the component tree's invocations, or the bench silently measures
        // an empty walk.
        let inventory = lintel_core::collect_inventory(&parsed.roots, &parsed.context);
        assert_eq!(
            inventory.invocations.len(),
            13,
            "PageShell + 3 Section + 9 Row: {:?}",
            inventory.invocations
        );
        assert_eq!(inventory.declared.len(), 1, "only `Page` is declared here");
        b.iter(|| {
            lintel_core::collect_inventory(black_box(&parsed.roots), black_box(&parsed.context))
        });
    });

    group.finish();
}

/// The rule-registry listing (`docs/rule-registry.md` seam): the
/// machine-readable identities behind `lintel rules` — the built-in
/// listing's JSON emission, and `registry_metas` over a synthetic
/// 128-rule classified registry (the docs-site/editor-consumer listing
/// cost, linear in registry size).
struct ClassifiedBenchRule;

impl lintel_core::ValidationRule for ClassifiedBenchRule {
    fn check(
        &self,
        _: &lintel_core::ast::Node,
        _: &lintel_core::scan::ScanContext,
        _: &mut Vec<lintel_core::Diagnostic>,
    ) {
    }
    fn meta(&self) -> Option<lintel_core::RuleMeta> {
        Some(lintel_core::RuleMeta {
            id: "bench-policy",
            codes: &[lintel_core::Code::VoidChildren],
            summary: "a synthetic classified policy for the listing bench",
        })
    }
}

fn rules_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("rules");
    group.sample_size(30);
    group.warm_up_time(Duration::from_millis(200));
    group.measurement_time(Duration::from_secs(2));

    group.bench_function("builtin_registry_json", |b| {
        // Path drift-check (the §21.2 lesson): the listing must cover all
        // eight built-in rules, or the bench silently measures a stub.
        assert_eq!(lintel_core::builtin_rule_metas().count(), 8);
        b.iter(|| {
            let mut out = String::new();
            for meta in lintel_core::builtin_rule_metas() {
                let _ = writeln!(out, "{}", meta.to_json());
            }
            black_box(out)
        });
    });

    group.bench_function("classified_registry_metas_128", |b| {
        let rules: Vec<&dyn lintel_core::ValidationRule> =
            std::iter::repeat(&ClassifiedBenchRule as &dyn lintel_core::ValidationRule)
                .take(128)
                .collect();
        b.iter(|| {
            let metas = lintel_core::registry_metas(black_box(&rules));
            assert_eq!(metas.len(), 128, "one entry per rule");
            black_box(metas)
        });
    });

    group.finish();
}

/// The foreign declarations file for the cross-file bench (`docs/cross-file-signatures.md`
/// seam): declares the base file's `Section` with a `children` parameter
/// (accepted) and its `Row` without one (each invocation with children
/// reports LNT0204 once merged).
fn cross_file_foreign_source() -> String {
    String::from(
        "<?php\nfunction PageShell(string $user, array $items = [], $children = null) {}\n\
         function Section(string $title, bool $open = false, $children = null) {}\n\
         function Row(int $key, bool $active) {}\n",
    )
}

fn cross_file_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("cross_file");
    group.sample_size(30);
    group.warm_up_time(Duration::from_millis(200));
    group.measurement_time(Duration::from_secs(2));

    group.bench_function("merged_context_validate", |b| {
        let foreign = lintel_core::scan::scan(&cross_file_foreign_source());
        let base_src = component_source();
        let base = lintel_core::scan::scan(black_box(&base_src));
        assert!(
            foreign.diagnostics.is_empty() && base.diagnostics.is_empty(),
            "both files parse cleanly"
        );
        // Path drift-check (the §21.2 lesson): without the merge the base
        // file's `PageShell`/`Section`/`Row` invocations are cross-file and
        // unchecked; with it, the nine `Row` children violations report.
        let plain =
            lintel_core::validate::validate_with(&base.roots, &base.context, builtin_rules());
        assert!(
            plain.is_empty(),
            "the plain context checks nothing: {plain:?}"
        );
        let merged = base.context.merged_with(&foreign.context);
        let found = lintel_core::validate::validate_with(&base.roots, &merged, builtin_rules());
        assert_eq!(found.len(), 9, "nine `Row` children violations: {found:?}");
        b.iter(|| {
            lintel_core::validate::validate_with(
                black_box(&base.roots),
                black_box(&merged),
                builtin_rules(),
            )
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench,
    sourcemap_bench,
    json_bench,
    visitor_bench,
    inventory_bench,
    rules_bench,
    cross_file_bench
);
criterion_main!(benches);
