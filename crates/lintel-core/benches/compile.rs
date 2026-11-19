//! Compile-throughput benches (architecture doc 9.3 hypotheses).
//!
//! Run: `cargo bench -p lintel-core`. Smoke-run under the test harness with
//! `cargo test --benches`.

use std::fmt::Write as _;
use std::hint::black_box;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};
use lintel_core::{compile, Options, Target};

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
    group.bench_function("static_heavy", |b| {
        let src = static_heavy_source();
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

    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
