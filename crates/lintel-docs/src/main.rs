//! The T-17 docs-site builder (task card: docs site v0).
//!
//! One binary does the whole v0 job: render the committed markdown pages
//! under `docs-site/pages/`, generate the error index from the
//! `lintel_diag` code registry (serving `/e/LNTxxxx` exactly as
//! `Code::docs_url` promises), embed the committed benchmark results, copy
//! the playground in, and then **link-check every emitted page** — a broken
//! internal link fails the build (§13.7's docs link check as a gate, not a
//! hope).
//!
//! The output lands in `docs-site/build/` and is a complete deployable tree:
//! any static file server serves it (`php -S localhost:8080 -t
//! docs-site/build/`). Deploying to lintel.sh is T-18/launch; v0 is the
//! local build plus the green link check.
//!
//! External `http(s)` links are recorded but not fetched (no network in a
//! build gate); `https://lintel.sh/e/...` links are an exception — they are
//! the diagnostics' own `docs_url` contract, so they are rewritten to the
//! local `/e/...` page and must resolve.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use lintel_diag::Code;
use pulldown_cmark::{html, Options, Parser};

/// The repo root (the crate lives at `crates/lintel-docs`).
const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

/// The committed nav, in order — every entry must resolve (the check at the
/// bottom enforces it).
const NAV: &[(&str, &str)] = &[
    ("/", "Home"),
    ("/quick-start/", "Quick start"),
    ("/tooling/", "Tooling"),
    ("/spec/", "Language spec"),
    ("/security/", "Security model"),
    ("/benchmarks/", "Benchmarks"),
    ("/errors/", "Error index"),
    ("/playground/", "Playground"),
];

/// The error registry: every `Code` with the one-line description the index
/// shows. Pages that exist under `docs/errors/` are served; the rest are
/// listed as pending rather than linked (no dead links, no stubs).
const ERRORS: &[(Code, &str)] = &[
    (Code::UnterminatedTag, "a tag or element was never closed"),
    (
        Code::MismatchedClose,
        "a closing tag does not match the innermost open tag",
    ),
    (Code::BadAttr, "an attribute could not be parsed"),
    (Code::BadSpread, "a spread was not written as `{...expr}`"),
    (Code::Expected, "a specific character was expected"),
    (
        Code::BuiltinShadow,
        "a component name shadows a PHP builtin",
    ),
    (
        Code::HyphenArg,
        "an attribute name cannot become a PHP named argument",
    ),
    (
        Code::UnknownProp,
        "a component prop is not a declared parameter",
    ),
    (
        Code::UnexpectedChildren,
        "children passed to a component without a `children` parameter",
    ),
    (
        Code::MissingRequiredProp,
        "a required component parameter was omitted",
    ),
    (
        Code::DynamicHandler,
        "a dynamic `on*` event-handler attribute",
    ),
    (
        Code::StaticHandler,
        "a static `on*` event-handler attribute (warning)",
    ),
    (Code::VoidChildren, "children inside a void element"),
];

fn main() {
    let out = Path::new(ROOT).join("docs-site/build");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).expect("build root");

    let mut emitted: Vec<String> = Vec::new();
    let mut pages: BTreeMap<String, String> = BTreeMap::new();

    // The hand-written content pages, from `docs-site/pages/*.md`.
    for name in [
        "index",
        "quick-start",
        "tooling",
        "spec",
        "security",
        "benchmarks",
    ] {
        let src = Path::new(ROOT).join(format!("docs-site/pages/{name}.md"));
        let markdown = std::fs::read_to_string(&src)
            .unwrap_or_else(|error| panic!("missing docs page {}: {error}", src.display()));
        let html_path = if name == "index" {
            "/index.html".to_string()
        } else {
            format!("/{name}/index.html")
        };
        pages.insert(html_path, render_page(&markdown, name));
    }

    // The error index, registry-driven, plus every committed error page.
    let (index_md, error_pages) = error_index();
    pages.insert(
        "/errors/index.html".to_string(),
        render_page(&index_md, "errors"),
    );
    for (path, body) in error_pages {
        pages.insert(path, render_page(&body, "errors"));
    }

    // The benchmarks page embeds the committed results verbatim.
    let benchmarks_md = benchmarks_page();
    pages.insert(
        "/benchmarks/index.html".to_string(),
        render_page(&benchmarks_md, "benchmarks"),
    );

    // Emit pages, copy static assets, mirror the playground into the tree.
    for (path, body) in &pages {
        write_out(&out, path, body);
        emitted.push(path.clone());
    }
    copy_file(
        &out,
        "style.css",
        include_str!("../../../docs-site/assets/style.css"),
    );
    emitted.push("/style.css".to_string());
    for chart in [
        "chart-S1.svg",
        "chart-S2.svg",
        "chart-S3.svg",
        "chart-S4.svg",
        "chart-S5.svg",
    ] {
        let source = Path::new(ROOT).join("bench/results").join(chart);
        copy_repo_file(&out, &format!("/bench/{chart}"), &source);
        emitted.push(format!("/bench/{chart}"));
    }
    for results in std::fs::read_dir(Path::new(ROOT).join("bench/results")).expect("bench results")
    {
        let file = results.expect("bench results entry").path();
        if file.extension().is_some_and(|ext| {
            matches!(
                ext.to_string_lossy().to_ascii_lowercase().as_str(),
                "json" | "svg" | "md"
            )
        }) {
            let name = file
                .file_name()
                .expect("file name")
                .to_string_lossy()
                .to_string();
            copy_repo_file(&out, &format!("/bench/results/{name}"), &file);
            emitted.push(format!("/bench/results/{name}"));
        }
    }
    copy_dir(&Path::new(ROOT).join("playground"), &out.join("playground"));
    collect_tree(&out.join("playground"), "/playground", &mut emitted);

    // §13.7 docs link check: every emitted page's internal links resolve.
    let broken = check_links(&out, &emitted);
    if !broken.is_empty() {
        for link in &broken {
            eprintln!("docs-site: broken link {link}");
        }
        panic!(
            "docs-site link check failed with {} broken link(s)",
            broken.len()
        );
    }
    println!(
        "docs-site: built {} pages/files into docs-site/build/ — link check clean (0 broken)",
        emitted.len()
    );
}

/// Build the error-index page and the committed per-code pages. A code
/// with a committed `docs/errors/LNTxxxx.md` page is served at `/e/LNTxxxx/`
/// exactly as `Code::docs_url` promises; the rest are listed as pending
/// rather than linked (no dead links, no stubs).
fn error_index() -> (String, Vec<(String, String)>) {
    let mut rows = String::new();
    let mut pages: Vec<(String, String)> = Vec::new();
    for (code, summary) in ERRORS {
        let lnt = code.lnt();
        let page_src = Path::new(ROOT).join(format!("docs/errors/{lnt}.md"));
        if page_src.is_file() {
            let markdown = std::fs::read_to_string(&page_src).expect("read error page");
            pages.push((format!("/e/{lnt}/index.html"), markdown));
            writeln!(
                rows,
                "| [{lnt}](/e/{lnt}/) | `{}` | {summary} |",
                code.docs_url(),
            )
            .expect("format error index row");
        } else {
            writeln!(rows, "| {lnt} | — | {summary} (docs page pending) |")
                .expect("format error index row");
        }
    }
    let index_md = format!(
        "# Error index\n\nEvery diagnostic carries an `LNTxxxx` code and a docs URL\n\
         (`https://lintel.sh/e/LNTxxxx`, `Code::docs_url`) — this site serves\n\
         that path for every page already written; the rest are listed as\n\
         pending rather than linked.\n\n| code | docs URL | what it covers |\n\
         |---|---|---|\n{rows}\n",
    );
    (index_md, pages)
}

/// Render one markdown body into the shared site template.
fn render_page(markdown: &str, active: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(markdown, options);
    let mut body = String::with_capacity(markdown.len() * 2);
    html::push_html(&mut body, parser);

    let mut nav = String::new();
    for (href, label) in NAV {
        let is_active = *href == "/" && active == "index"
            || format!("/{active}/") == *href
            || (active == "errors" && href.starts_with("/e/"));
        let _ = write!(
            nav,
            "<a href=\"{href}\"{}>{label}</a>",
            if is_active { " class=\"active\"" } else { "" },
        );
    }
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>Lintel — JSX for PHP</title>\n<link rel=\"stylesheet\" href=\"/style.css\">\n\
         </head>\n<body>\n<header class=\"topnav\">\n<a class=\"brand\" href=\"/\">Lintel</a>\n\
         <nav>{nav}</nav>\n</header>\n<main>\n{body}\n</main>\n\
         <footer><p>Lintel compiles JSX-in-PHP to plain PHP string concatenation — \
         auto-escaped, line-preserving, no runtime past a handful of helpers. \
         The <a href=\"/playground/\">playground</a> runs the compiler in your browser.</p></footer>\n\
         </body>\n</html>\n",
    )
}

/// The benchmarks page: committed artifacts embedded verbatim, methodology
/// included, publishing rule stated.
fn benchmarks_page() -> String {
    let results_dir = Path::new(ROOT).join("bench/results");
    let mut summaries: Vec<String> = std::fs::read_dir(&results_dir)
        .expect("bench results dir")
        .filter_map(|entry| {
            let name = entry
                .expect("results entry")
                .file_name()
                .to_string_lossy()
                .to_string();
            name.starts_with("summary-").then_some(name)
        })
        .collect();
    summaries.sort();
    let summary = summaries
        .last()
        .unwrap_or_else(|| panic!("bench/results/ has no committed summary markdown"));
    let summary_md = std::fs::read_to_string(results_dir.join(&**summary)).expect("summary md");
    let mut raws: Vec<String> = std::fs::read_dir(&results_dir)
        .expect("bench results dir")
        .filter_map(|entry| {
            let name = entry
                .expect("results entry")
                .file_name()
                .to_string_lossy()
                .to_string();
            name.starts_with("results-").then_some(name)
        })
        .collect();
    raws.sort();
    let raw = raws.last().expect("committed raw JSON");

    format!(
        "# Benchmarks\n\n## Methodology\n\nThe full harness methodology and the §9.2 \
         publishing rule live in `bench/` and are reproduced below verbatim; \
         `make bench` regenerates everything under \
         [`/bench/results/`](/bench/results/{raw}).\n\n{}\n\n## Committed results\n\n{}\n\
         The per-scenario charts: [S1](/bench/chart-S1.svg) · [S2](/bench/chart-S2.svg) · \
         [S3](/bench/chart-S3.svg) · [S4](/bench/chart-S4.svg) · [S5](/bench/chart-S5.svg).\n\
         The raw JSON every number here comes from: \
         [`/bench/results/{raw}`](/bench/results/{raw}).\n",
        include_str!("../../../bench/README.md"),
        markdown_to_html(&summary_md),
    )
}

/// Render a markdown fragment (the committed bench summary) without the
/// site template — the page template wraps it.
fn markdown_to_html(markdown: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    let mut body = String::with_capacity(markdown.len() * 2);
    html::push_html(&mut body, Parser::new_ext(markdown, options));
    body
}

/// Write one emitted file at `path` (an absolute-on-site path) under `out`.
fn write_out(out: &Path, path: &str, body: &str) {
    let target = out.join(path.trim_start_matches('/'));
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).expect("create page dir");
    }
    std::fs::write(target, body).expect("write page");
}

/// Copy a committed repo file into the build tree at `site_path`.
fn copy_repo_file(out: &Path, site_path: &str, from: &Path) {
    let target = out.join(site_path.trim_start_matches('/'));
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).expect("create asset dir");
    }
    std::fs::copy(from, target).expect("copy repo file");
}

/// Copy an in-crate static asset.
fn copy_file(out: &Path, name: &str, contents: &str) {
    std::fs::write(out.join(name.trim_start_matches('/')), contents).expect("write asset");
}

/// Recursively copy `from` (the playground) into `to`.
fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create copy target");
    for entry in std::fs::read_dir(from).expect("read copy source") {
        let entry = entry.expect("copy entry");
        let kind = entry.file_type().expect("copy entry type");
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).expect("copy file");
        }
    }
}

/// Flatten a copied tree into emitted site paths (for the link check).
fn collect_tree(root: &Path, prefix: &str, emitted: &mut Vec<String>) {
    for entry in std::fs::read_dir(root).expect("walk copy tree") {
        let path = entry.expect("walk entry").path();
        let name = path
            .file_name()
            .expect("walk name")
            .to_string_lossy()
            .to_string();
        if path.is_dir() {
            collect_tree(&path, &format!("{prefix}/{name}"), emitted);
        } else {
            emitted.push(format!("{prefix}/{name}"));
        }
    }
}

/// Every internal link in every emitted page must resolve to an emitted
/// path (a directory link resolves through its `index.html`). `lintel.sh/e/…`
/// is the diagnostics' `docs_url` contract and is checked as `/e/…`.
/// Fragment-only links and external `http(s)` links pass (no network in a
/// build gate).
fn check_links(out: &Path, emitted: &[String]) -> Vec<String> {
    let mut broken: Vec<String> = Vec::new();
    for path in emitted {
        if !path.to_ascii_lowercase().ends_with(".html") {
            continue;
        }
        let file = out.join(path.trim_start_matches('/'));
        let body = std::fs::read_to_string(&file).expect("read emitted page");
        for attribute in ["href", "src"] {
            for target in attribute_values(&body, attribute) {
                if target.starts_with('#')
                    || target.starts_with("http://")
                    || target.starts_with("https://github.com/")
                    || target.starts_with("mailto:")
                    || target.starts_with("data:")
                {
                    continue;
                }
                let stripped = target.strip_prefix("https://lintel.sh").unwrap_or(&target);
                let Some(resolved) = resolve_link(path, stripped) else {
                    broken.push(format!("{path} -> {target} (escapes the site root)"));
                    continue;
                };
                if !resolves(out, &resolved, emitted) {
                    broken.push(format!("{path} -> {target}"));
                }
            }
        }
    }
    broken
}

/// Resolve one link against its emitting page: absolute-on-site paths root
/// at the build tree, page-relative paths resolve against the page's
/// directory; `.` and `..` segments normalize lexically (the emitted HTML
/// carries no query strings), and `None` means the link escapes the root.
fn resolve_link(page: &str, target: &str) -> Option<String> {
    let without_fragment = target.split('#').next().unwrap_or(target);
    if without_fragment.is_empty() {
        return None;
    }
    let joined = if let Some(rest) = without_fragment.strip_prefix('/') {
        rest.to_string()
    } else {
        let dir = page
            .rsplit_once('/')
            .map_or(String::new(), |(dir, _)| dir.to_string());
        if dir.is_empty() {
            without_fragment.to_string()
        } else {
            format!("{dir}/{without_fragment}")
        }
    };
    let mut parts: Vec<&str> = Vec::new();
    for segment in joined.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    if parts.is_empty() {
        return Some("/".to_string());
    }
    Some(format!("/{}", parts.join("/")))
}

/// Whether `resolved` (an absolute-on-site path) hits an emitted file or a
/// directory whose `index.html` was emitted.
fn resolves(out: &Path, resolved: &str, emitted: &[String]) -> bool {
    let target = resolved.trim_end_matches('/');
    if emitted.iter().any(|path| {
        path == target
            || path == &format!("{target}/index.html")
            || (target == "/" && path == "/index.html")
    }) {
        return true;
    }
    out.join(target.trim_start_matches('/'))
        .join("index.html")
        .is_file()
}

/// Extract the values of one attribute from emitted HTML. The emitted HTML
/// is generated by this builder (template + pulldown-cmark), so plain
/// attribute scanning is exact for its own output.
fn attribute_values(html: &str, attribute: &str) -> Vec<String> {
    let needle = format!("{attribute}=\"");
    let mut values = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find(&needle) {
        let after = &rest[start + needle.len()..];
        let Some(end) = after.find('"') else { break };
        values.push(after[..end].to_string());
        rest = &after[end + 1..];
    }
    values
}
