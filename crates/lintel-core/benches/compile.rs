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
            "        <button onclick={{track('row-{i}')}}>action {i}</button>"
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
            "        <button onclick={{track('row-{i}')}}>action {i}</button>"
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
            "        <button onclick={{track('row-{i}')}}>action {i}</button>"
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

criterion_group!(benches, bench, sourcemap_bench);
criterion_main!(benches);
