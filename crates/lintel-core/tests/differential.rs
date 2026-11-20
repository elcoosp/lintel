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
//! isolated cases in [`differential_isolated_cases`], and every reconciled
//! case stays here as a permanent regression.
//!
//! The PHP binary comes from `PHP_BIN` (default `php` on `PATH`); without
//! one the gate prints a skip note (CI's `php` job always sets it).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use proptest::prelude::*;
use serde_json::{json, Value};

use lintel_core::{compile, Options, Target};

// ---------------------------------------------------------------------------
// Value specs — one JSON shape shared by generator, driver and reference.
// `{"arg": N}` references template argument N; otherwise `k` names an
// inline constant. `clstpl` is the class-list template shape (§4.4:
// strings, lists, `['cls' => bool]`, nested lists).
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum Spec {
    Arg(usize),
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Null,
    /// class={[ 'a', 'b' => cond, [nest], arg ]} — ordered entries.
    ClsTpl(Vec<ClsEntry>),
    /// style={['k' => 'v', …]} — ordered pairs (JSON objects sort keys).
    StylePairs(Vec<(String, String)>),
}

#[derive(Clone, Debug)]
enum ClsEntry {
    Val(Spec),
    /// `'name' => cond` — emitted iff the cond spec is truthy.
    Cond {
        key: String,
        cond: Spec,
    },
    Nest(Vec<ClsEntry>),
}

fn cls_entry_json(e: &ClsEntry) -> Value {
    match e {
        ClsEntry::Val(s) => spec_json(s),
        ClsEntry::Cond { key, cond } => json!({ "kv": key, "cond": spec_json(cond) }),
        ClsEntry::Nest(entries) => {
            json!({ "nest": entries.iter().map(cls_entry_json).collect::<Vec<_>>() })
        }
    }
}

fn spec_json(spec: &Spec) -> Value {
    match spec {
        Spec::Arg(i) => json!({ "arg": i }),
        Spec::Str(s) => json!({ "k": "string", "v": s }),
        Spec::Int(i) => json!({ "k": "int", "v": i }),
        Spec::Float(f) => json!({ "k": "float", "v": f }),
        Spec::Bool(b) => json!({ "k": "bool", "v": b }),
        Spec::Null => json!({ "k": "null" }),
        Spec::ClsTpl(entries) => {
            json!({ "k": "clstpl", "v": entries.iter().map(cls_entry_json).collect::<Vec<_>>() })
        }
        Spec::StylePairs(pairs) => json!({
            "k": "stylepairs",
            "v": pairs.iter().map(|(k, v)| json!([k, v])).collect::<Vec<_>>()
        }),
    }
}

// ---------------------------------------------------------------------------
// Tree nodes.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum Attr {
    /// `name="v"` — the §4.4 static row: folded verbatim, `"` re-escaped.
    Static { name: String, value: String },
    /// `name={expr}` — routed by name (URL/class/style/srcset/generic).
    Dyn { name: String, spec: Spec },
    /// `name` — boolean true.
    Bare { name: String },
    /// `{...$attrs}` — spread, routed per key inside the runtime helper.
    Spread(Spec),
}

fn attr_json(a: &Attr) -> Value {
    match a {
        Attr::Static { name, value } => json!({ "a": "static", "name": name, "v": value }),
        Attr::Dyn { name, spec } => json!({ "a": "dyn", "name": name, "spec": spec_json(spec) }),
        Attr::Bare { name } => json!({ "a": "bare", "name": name }),
        Attr::Spread(spec) => json!({ "a": "spread", "spec": spec_json(spec) }),
    }
}

#[derive(Clone, Debug)]
enum Node {
    El {
        tag: String,
        attrs: Vec<Attr>,
        children: Vec<Node>,
    },
    Frag {
        children: Vec<Node>,
    },
    Text {
        src: String,
    },
    Expr {
        spec: Spec,
    },
    Comment,
    Comp {
        name: &'static str,
        props: Vec<(&'static str, Spec)>,
        children: Vec<Node>,
    },
}

impl Node {
    fn to_json(&self) -> Value {
        match self {
            Node::El {
                tag,
                attrs,
                children,
            } => json!({
                "t": "el", "tag": tag, "void": VOID_TAGS.contains(&tag.as_str()),
                "attrs": attrs.iter().map(attr_json).collect::<Vec<_>>(),
                "children": children.iter().map(Self::to_json).collect::<Vec<_>>(),
            }),
            Node::Frag { children } => json!({
                "t": "frag",
                "children": children.iter().map(Self::to_json).collect::<Vec<_>>(),
            }),
            Node::Text { src } => json!({ "t": "text", "src": src }),
            Node::Expr { spec } => json!({ "t": "expr", "spec": spec_json(spec) }),
            Node::Comment => json!({ "t": "comment" }),
            Node::Comp {
                name,
                props,
                children,
            } => {
                let mut map = serde_json::Map::new();
                for (key, spec) in props {
                    map.insert((*key).to_string(), spec_json(spec));
                }
                if !children.is_empty() {
                    map.insert(
                        "children_nodes".to_string(),
                        Value::Array(children.iter().map(Self::to_json).collect()),
                    );
                }
                json!({ "t": "comp", "name": name, "props": Value::Object(map) })
            }
        }
    }
}

fn node_json(n: &Node) -> Value {
    n.to_json()
}

// ---------------------------------------------------------------------------
// proptest strategies (the "property-based: random trees" of §13.2).
// ---------------------------------------------------------------------------

const VOID_TAGS: &[&str] = &["br", "hr", "img", "input", "meta", "link"];
const PAIRED_TAGS: &[&str] = &[
    "div", "span", "p", "section", "b", "i", "em", "h1", "ul", "li", "td",
];
/// §4.4's URL-attribute row; generated trees stay inside this shared set.
const URL_ATTRS: &[&str] = &[
    "href",
    "src",
    "action",
    "formaction",
    "poster",
    "cite",
    "data",
];
const BARE_ATTRS: &[&str] = &["checked", "disabled", "hidden", "required", "open"];
const STATIC_NAMES: &[&str] = &[
    "title",
    "alt",
    "id",
    "lang",
    "data-kind",
    "width",
    "aria-label",
];

fn text_pool() -> impl Strategy<Value = String> {
    prop_oneof![
        4 => Just("hello world".to_string()),
        2 => Just("a & b".to_string()),
        1 => Just("a &amp; b".to_string()),
        1 => Just("q\"s".to_string()),
        1 => Just("it's".to_string()),
        1 => Just("x > y".to_string()),
        1 => Just("&lt;ent&gt;".to_string()),
        1 => Just("caf\u{e9} na\u{ef}ve".to_string()),
        1 => Just("0".to_string()),
    ]
}

fn url_pool() -> impl Strategy<Value = String> {
    prop_oneof![
        2 => Just("https://e.test/p?x=1&y=2".to_string()),
        1 => Just("/rel/a".to_string()),
        1 => Just("mailto:a@b.test".to_string()),
        1 => Just("tel:+1555".to_string()),
        1 => Just("sms:+1555".to_string()),
        1 => Just("http://e.test/".to_string()),
        1 => Just("#frag".to_string()),
        1 => Just("javascript:alert(1)".to_string()),
        1 => Just("data:text/html,x".to_string()),
        1 => Just("vbscript:x".to_string()),
        1 => Just("JaVaScRiPt:x".to_string()),
        1 => Just("java\tscript:x".to_string()),
        1 => Just(" javascript:x".to_string()),
        1 => Just(" &#x6A;avascript:alert(1)".to_string()),
        1 => Just("ftp://e.test/x".to_string()),
    ]
}

fn spread_pool() -> impl Strategy<Value = Value> {
    prop_oneof![
        2 => Just(json!({ "title": "t", "data-x": 1 })),
        2 => Just(json!({ "href": "javascript:alert(1)", "alt": null, "hidden": true })),
        2 => Just(json!({ "class": ["c1", "c2"], "style": { "color": "red" } })),
        1 => Just(json!({ "class": { "only": false, "kept": true } })),
        2 => Just(json!({ "src": "data:text/html,x", "title": "q\"t" })),
        1 => Just(json!({ "lang": "en" })),
        1 => Just(json!({})),
    ]
}

fn child_array_pool() -> impl Strategy<Value = Value> {
    prop_oneof![
        2 => Just(json!(["x", 1, null, true])),
        1 => Just(json!([["nested"], "y & z"])),
        1 => Just(json!([])),
        1 => Just(json!([false, "z & q", 3])),
    ]
}

/// The ten template arguments (signature in [`TEMPLATE_SIGNATURE`]).
fn args_strategy() -> impl Strategy<Value = Vec<Value>> {
    (
        text_pool(),
        url_pool(),
        prop_oneof![Just(0i64), Just(42), Just(-7), Just(123_456), Just(7)],
        prop_oneof![Just(1.5f64), Just(2.25), Just(0.5), Just(100.125), Just(3.0)],
        proptest::bool::ANY,
        prop_oneof![
            1 => Just(json!({ "k": "null" })),
            1 => Just(json!({ "k": "bool", "v": false })),
            1 => Just(json!({ "k": "bool", "v": true })),
            1 => Just(json!({ "k": "int", "v": 9 })),
            2 => Just(json!({ "k": "string", "v": "mix & ed" })),
        ],
        child_array_pool(),
        prop_oneof![1 => Just("st & <v>".to_string()), 1 => Just("plain".to_string())],
        prop_oneof![1 => Just("<i>raw & <b>bold</b></i>".to_string()), 1 => Just("<em>ok</em>".to_string())],
        spread_pool(),
    )
        .prop_map(
            |(w0, w1, w2, w3, w4, w5, w6, w7, w8, w9)| {
                vec![
                    json!({ "k": "string", "v": w0 }),
                    json!({ "k": "string", "v": w1 }),
                    json!({ "k": "int", "v": w2 }),
                    json!({ "k": "float", "v": w3 }),
                    json!({ "k": "bool", "v": w4 }),
                    w5,
                    json!({ "k": "array", "v": w6 }),
                    json!({ "k": "stringable", "v": w7 }),
                    json!({ "k": "html", "v": w8 }),
                    json!({ "k": "array", "v": w9 }),
                ]
            },
        )
}

fn attr_strategy() -> impl Strategy<Value = Attr> {
    prop_oneof![
        3 => (proptest::sample::select(STATIC_NAMES), static_value_pool())
            .prop_map(|(name, value)| Attr::Static { name: name.to_string(), value }),
        3 => (proptest::sample::select(URL_ATTRS), Just(Spec::Arg(1)))
            .prop_map(|(name, spec)| Attr::Dyn { name: name.to_string(), spec }),
        2 => (
            proptest::sample::select(STATIC_NAMES),
            prop_oneof![
                2 => Just(Spec::Arg(0)),
                1 => Just(Spec::Arg(2)),
                1 => Just(Spec::Arg(4)),
                1 => Just(Spec::Null),
                1 => Just(Spec::Bool(true)),
            ]
        )
            .prop_map(|(name, spec)| Attr::Dyn { name: name.to_string(), spec }),
        2 => proptest::sample::select(BARE_ATTRS)
            .prop_map(|name| Attr::Bare { name: name.to_string() }),
        1 => Just(Attr::Spread(Spec::Arg(9))),
        2 => class_list_pool()
            .prop_map(|entries| Attr::Dyn { name: "class".to_string(), spec: Spec::ClsTpl(entries) }),
        1 => style_pool()
            .prop_map(|pairs| Attr::Dyn { name: "style".to_string(), spec: Spec::StylePairs(pairs) }),
        1 => Just(Attr::Dyn { name: "srcset".to_string(), spec: Spec::Arg(1) }),
    ]
}

fn static_value_pool() -> impl Strategy<Value = String> {
    prop_oneof![
        2 => Just("a & b".to_string()),
        1 => Just("q\"r".to_string()),
        1 => Just("x".to_string()),
        1 => Just(String::new()),
        1 => Just("it's".to_string()),
    ]
}

fn class_list_pool() -> impl Strategy<Value = Vec<ClsEntry>> {
    prop_oneof![
        2 => Just(vec![
            ClsEntry::Val(Spec::Str("a".to_string())),
            ClsEntry::Val(Spec::Str("b".to_string())),
        ]),
        2 => Just(vec![
            ClsEntry::Val(Spec::Str("card".to_string())),
            ClsEntry::Cond { key: "open".to_string(), cond: Spec::Arg(4) },
        ]),
        1 => Just(vec![
            ClsEntry::Nest(vec![ClsEntry::Val(Spec::Str("x".to_string()))]),
            ClsEntry::Nest(vec![
                ClsEntry::Cond { key: "off".to_string(), cond: Spec::Bool(false) },
                ClsEntry::Cond { key: "on".to_string(), cond: Spec::Bool(true) },
            ]),
            ClsEntry::Val(Spec::Str("z".to_string())),
        ]),
        1 => Just(vec![
            ClsEntry::Val(Spec::Str(String::new())),
            ClsEntry::Cond { key: "maybe".to_string(), cond: Spec::Arg(4) },
        ]),
        1 => Just(vec![ClsEntry::Val(Spec::Arg(0))]),
        1 => Just(vec![]),
    ]
}

fn style_pool() -> impl Strategy<Value = Vec<(String, String)>> {
    prop_oneof![
        2 => Just(vec![("color".to_string(), "red".to_string())]),
        1 => Just(vec![
            ("background-color".to_string(), "#fff".to_string()),
            ("margin".to_string(), "10px auto".to_string()),
        ]),
        1 => Just(vec![("font-size".to_string(), "1.5em".to_string())]),
        1 => Just(vec![]),
    ]
}

fn expr_spec_pool() -> impl Strategy<Value = Spec> {
    prop_oneof![
        3 => Just(Spec::Arg(0)),
        1 => Just(Spec::Arg(2)),
        1 => Just(Spec::Arg(3)),
        2 => Just(Spec::Arg(4)),
        2 => Just(Spec::Arg(5)),
        2 => Just(Spec::Arg(6)),
        1 => Just(Spec::Arg(7)),
        1 => Just(Spec::Arg(8)),
        1 => Just(Spec::Str("lit & \"q\"".to_string())),
        1 => Just(Spec::Int(42)),
        1 => Just(Spec::Float(1.5)),
    ]
}

fn leaf_strategy() -> impl Strategy<Value = Node> {
    prop_oneof![
        4 => text_pool().prop_map(|src| Node::Text { src }),
        4 => expr_spec_pool().prop_map(|spec| Node::Expr { spec }),
        1 => Just(Node::Comment),
    ]
}

fn tree_strategy() -> impl Strategy<Value = Node> {
    leaf_strategy().prop_recursive(
        3,  // no more than 3 levels deep
        24, // at most 24 nodes total
        4,  // each element has up to 4 children
        |inner| {
            prop_oneof![
                4 => (
                    prop_oneof![
                        proptest::sample::select(PAIRED_TAGS),
                        proptest::sample::select(VOID_TAGS),
                    ],
                    proptest::collection::vec(attr_strategy(), 0..=4),
                    proptest::collection::vec(inner.clone(), 0..=3),
                )
                    .prop_map(|(tag, attrs, children)| {
                        // Void elements have no children (§4.2).
                        let children = if VOID_TAGS.contains(&tag) { vec![] } else { children };
                        Node::El { tag: tag.to_string(), attrs, children }
                    }),
                1 => proptest::collection::vec(inner.clone(), 0..=3)
                    .prop_map(|children| Node::Frag { children }),
                2 => (comp_pool(), proptest::collection::vec(inner, 0..=2))
                    .prop_map(|((name, props), children)| {
                        // Badge takes no children param (the compiler's
                        // UnexpectedChildren validation would reject it).
                        let children = if name == "Badge" { vec![] } else { children };
                        Node::Comp { name, props, children }
                    }),
            ]
        },
    )
}

fn comp_pool() -> impl Strategy<Value = (&'static str, Vec<(&'static str, Spec)>)> {
    prop_oneof![
        1 => (
            Just("Card"),
            prop_oneof![
                1 => Just(vec![("variant", Spec::Str("wide".to_string()))]),
                1 => Just(vec![("variant", Spec::Arg(0)), ("cls", Spec::Str("padded".to_string()))]),
                1 => Just(vec![("variant", Spec::Str("card".to_string())), ("cls", Spec::Arg(0))]),
            ]
        ),
        1 => (
            Just("Badge"),
            prop_oneof![
                1 => Just(vec![("label", Spec::Arg(0))]),
                1 => Just(vec![("label", Spec::Str("hi & by".to_string())), ("on", Spec::Bool(true))]),
                1 => Just(vec![("label", Spec::Arg(2)), ("on", Spec::Arg(4))]),
                1 => Just(vec![("label", Spec::Arg(0)), ("on", Spec::Bool(false))]),
            ]
        ),
    ]
}

// ---------------------------------------------------------------------------
// Source + payload generation.
// ---------------------------------------------------------------------------

/// The fixed template signature. Args (indices into `args_strategy`):
/// w0 text-ish string, w1 URL string, w2 int, w3 float, w4 bool, w5 mixed
/// scalar, w6 iterable child, w7 Stringable, w8 Html, w9 spread map.
/// No return type: the string target yields an `Html`, the stream target a
/// `Generator` (which `c()` walks as a markup stream, §10), so the same
/// source executes under both.
const TEMPLATE_SIGNATURE: &str =
    "function T(string $w0, string $w1, int $w2, float $w3, bool $w4, $w5, array $w6, $w7, $w8, array $w9)";

/// The two fixed component bodies (§4.7 named-arg contract) the reference
/// model mirrors exactly (`r_component` in `reference_renderer.php`).
const COMPONENT_DEFS: &str = "\
function Card(string $variant, $cls = null, $children = null) {
    return <section class={['card', \"card--$variant\", $cls]}>{$children}</section>;
}
function Badge($label, bool $on = false) {
    return <b data-on={$on}>{$label}</b>;
}
";

fn attr_source(a: &Attr) -> String {
    match a {
        Attr::Static { name, value } => {
            if value.contains('"') {
                format!(" {name}='{value}'")
            } else {
                format!(" {name}=\"{value}\"")
            }
        }
        Attr::Dyn { name, spec } => format!(" {name}={{{}}}", expr_source(spec)),
        Attr::Bare { name } => format!(" {name}"),
        Attr::Spread(_) => " {...$w9}".to_string(),
    }
}

fn class_entries_source(entries: &[ClsEntry]) -> String {
    entries
        .iter()
        .map(|e| match e {
            ClsEntry::Val(spec) => expr_source(spec),
            ClsEntry::Cond { key, cond } => format!("'{key}' => {}", expr_source(cond)),
            ClsEntry::Nest(nested) => format!("[{}]", class_entries_source(nested)),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn php_single_quoted(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
}

fn expr_source(spec: &Spec) -> String {
    match spec {
        Spec::Arg(i) => format!("$w{i}"),
        Spec::Str(s) => php_single_quoted(s),
        Spec::Int(i) => i.to_string(),
        Spec::Float(f) => format!("{f}"),
        Spec::Bool(b) => {
            if *b {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        Spec::Null => "null".to_string(),
        Spec::StylePairs(pairs) => format!(
            "[{}]",
            pairs
                .iter()
                .map(|(k, v)| format!("{} => {}", php_single_quoted(k), php_single_quoted(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Spec::ClsTpl(entries) => format!("[{}]", class_entries_source(entries)),
    }
}

fn node_source(n: &Node) -> String {
    match n {
        Node::El {
            tag,
            attrs,
            children,
        } => {
            let attrs = attrs.iter().map(attr_source).collect::<String>();
            if VOID_TAGS.contains(&tag.as_str()) {
                // JSX has no bare-void source form (Babel rejects `<br>` with
                // "Expected corresponding JSX closing tag"); self-close.
                return format!("<{tag}{attrs} />");
            }
            let children = children.iter().map(node_source).collect::<String>();
            format!("<{tag}{attrs}>{children}</{tag}>")
        }
        Node::Frag { children } => {
            let children = children.iter().map(node_source).collect::<String>();
            format!("<>{children}</>")
        }
        Node::Text { src } => src.clone(),
        Node::Expr { spec } => format!("{{{}}}", expr_source(spec)),
        Node::Comment => "{/* c */}".to_string(),
        Node::Comp {
            name,
            props,
            children,
        } => {
            let attrs = props
                .iter()
                .map(|(key, spec)| match spec {
                    // `on` bare = boolean true (§4.4 bare row)
                    Spec::Bool(true) if *key == "on" => format!(" {key}"),
                    _ => format!(" {key}={{{}}}", expr_source(spec)),
                })
                .collect::<String>();
            let children = children.iter().map(node_source).collect::<String>();
            format!("<{name}{attrs}>{children}</{name}>")
        }
    }
}

#[derive(Debug)]
struct Case {
    source: String,
    payload: Value,
}

fn case_strategy() -> impl Strategy<Value = Case> {
    (tree_strategy(), args_strategy()).prop_map(|(tree, args)| {
        // JSX is recognized only in expression-start position (§4.1): a
        // bare text/expression root would emit plain PHP, not JSX — so a
        // non-element root rides in a fragment, which adds no wrapper.
        let tree = match tree {
            Node::Text { .. } | Node::Expr { .. } | Node::Comment => Node::Frag {
                children: vec![tree],
            },
            other => other,
        };
        let source = format!(
            "<?php\n{COMPONENT_DEFS}{TEMPLATE_SIGNATURE} {{\n    return {};\n}}\n",
            node_source(&tree)
        );
        Case {
            source,
            payload: json!({ "tree": node_json(&tree), "args": args }),
        }
    })
}

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
            std::env::temp_dir().join(format!("lintel-differential-{}-{seq}", std::process::id()));
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

// ---------------------------------------------------------------------------
// Isolated reconciliation cases — one minimal case per known divergence
// class (§21.2: class lists, spreads, self-closing shapes), kept forever.
// Each is (source, payload) compared like the generated cases, or a
// compile-rejection pinned to its diagnostic code.
// ---------------------------------------------------------------------------

enum IsoCase {
    Compare {
        name: String,
        source: String,
        payload: Value,
    },
    /// The compiler must reject the source with this diagnostic code.
    Reject {
        name: String,
        source: String,
        code: &'static str,
    },
}

/// Wrap one JSX `return` body in the shared template source.
fn iso_wrapped(body: &str) -> String {
    format!("<?php\n{COMPONENT_DEFS}{TEMPLATE_SIGNATURE} {{\n    return {body};\n}}\n")
}

/// The shared argument list (indices = the `$wN` of [`TEMPLATE_SIGNATURE`]).
fn base_args() -> Value {
    json!([
        { "k": "string", "v": "a & b" },
        { "k": "string", "v": "https://e.test/x" },
        { "k": "int", "v": 7 },
        { "k": "float", "v": 1.5 },
        { "k": "bool", "v": true },
        { "k": "null" },
        { "k": "array", "v": ["one", 2, null] },
        { "k": "stringable", "v": "st & <v>" },
        { "k": "html", "v": "<i>raw</i>" },
        { "k": "array", "v": { "title": "t" } },
    ])
}

fn with_arg(index: usize, spec: Value) -> Value {
    let mut merged = base_args();
    merged.as_array_mut().expect("args array")[index] = spec;
    merged
}

fn iso_text(src: &str) -> Value {
    json!({ "t": "text", "src": src })
}

fn iso_obj(pairs: Vec<(&str, Value)>) -> Value {
    let mut o = serde_json::Map::new();
    for (key, value) in pairs {
        o.insert(key.to_string(), value);
    }
    Value::Object(o)
}

fn iso_el(tag: &str, attrs: Value, children: Value) -> Value {
    iso_obj(vec![
        ("t", json!("el")),
        ("tag", json!(tag)),
        ("void", json!(VOID_TAGS.contains(&tag))),
        ("attrs", attrs),
        ("children", children),
    ])
}

fn iso_frag(children: Value) -> Value {
    iso_obj(vec![("t", json!("frag")), ("children", children)])
}

fn iso_stat(name: &str, v: &str) -> Value {
    iso_obj(vec![
        ("a", json!("static")),
        ("name", json!(name)),
        ("v", json!(v)),
    ])
}

fn iso_dyna(name: &str, spec: Value) -> Value {
    iso_obj(vec![
        ("a", json!("dyn")),
        ("name", json!(name)),
        ("spec", spec),
    ])
}

fn iso_bare(name: &str) -> Value {
    iso_obj(vec![("a", json!("bare")), ("name", json!(name))])
}

fn iso_spread(spec: Value) -> Value {
    iso_obj(vec![("a", json!("spread")), ("spec", spec)])
}

fn iso_expr(spec: Value) -> Value {
    iso_obj(vec![("t", json!("expr")), ("spec", spec)])
}

fn iso_arg(i: usize) -> Value {
    json!({ "arg": i })
}

fn iso_comp(name: &str, props: Value, children: Value) -> Value {
    // children_nodes rides inside the props map (Node::Comp::to_json shape).
    let mut props = props;
    props
        .as_object_mut()
        .expect("comp props object")
        .insert("children_nodes".to_string(), children);
    json!({ "t": "comp", "name": name, "props": props })
}

/// The §21.2 class-list divergence class: conditional + nested arrays.
fn iso_class_cases() -> Vec<IsoCase> {
    let args = base_args();
    let clstpl = |entries: Value| iso_dyna("class", json!({ "k": "clstpl", "v": entries }));
    vec![
        IsoCase::Compare {
            name: "class_conditional_arg".to_string(),
            source: iso_wrapped("<div class={['a', 'b' => $w4]}>t</div>"),
            payload: json!({
                "tree": iso_el("div", json!([clstpl(json!(["a", { "kv": "b", "cond": iso_arg(4) }]))]), json!([iso_text("t")])),
                "args": args,
            }),
        },
        IsoCase::Compare {
            name: "class_nested_arrays".to_string(),
            source: iso_wrapped("<div class={[['x'], ['off' => false, 'on' => true], 'z', ['deep' => $w4]]}>t</div>"),
            payload: json!({
                "tree": iso_el("div", json!([clstpl(json!([
                    { "nest": [{ "k": "string", "v": "x" }] },
                    { "nest": [
                        { "kv": "off", "cond": { "k": "bool", "v": false } },
                        { "kv": "on", "cond": { "k": "bool", "v": true } },
                    ] },
                    { "k": "string", "v": "z" },
                    { "nest": [{ "kv": "deep", "cond": iso_arg(4) }] },
                ]))]), json!([iso_text("t")])),
                "args": args,
            }),
        },
        IsoCase::Compare {
            name: "class_empty_and_dynamic_entries".to_string(),
            source: iso_wrapped("<div class={['', $w0]}>t</div>"),
            payload: json!({
                "tree": iso_el("div", json!([clstpl(json!([
                    { "k": "string", "v": "" },
                    iso_arg(0),
                ]))]), json!([iso_text("t")])),
                "args": args,
            }),
        },
        IsoCase::Compare {
            name: "class_dynamic_string_value".to_string(),
            source: iso_wrapped("<div class={$w0}>t</div>"),
            payload: json!({
                "tree": iso_el("div", json!([iso_dyna("class", iso_arg(0))]), json!([iso_text("t")])),
                "args": args,
            }),
        },
    ]
}

/// The §21.2 spread divergence class: per-key routing + `on*`/unsafe-name
/// rejection + srcset routing (§4.4 "the same attribute rules").
fn iso_spread_cases() -> Vec<IsoCase> {
    let spread_body = "<div {...$w9}>t</div>";
    let spread_tree = iso_el(
        "div",
        json!([iso_spread(iso_arg(9))]),
        json!([iso_text("t")]),
    );
    let with_w9 = |map: Value| with_arg(9, json!({ "k": "array", "v": map }));
    vec![
        IsoCase::Compare {
            name: "spread_routing_url_class_style".to_string(),
            source: iso_wrapped(spread_body),
            payload: json!({
                "tree": spread_tree,
                "args": with_w9(json!({
                    "href": "javascript:alert(1)",
                    "class": ["c", "d"],
                    "style": { "color": "red" },
                    "title": "t",
                })),
            }),
        },
        IsoCase::Compare {
            name: "spread_on_key_rejected".to_string(),
            source: iso_wrapped(spread_body),
            payload: json!({
                "tree": spread_tree,
                "args": with_w9(json!({ "onclick": "track('x')" })),
            }),
        },
        IsoCase::Compare {
            name: "spread_unsafe_name_rejected".to_string(),
            source: iso_wrapped(spread_body),
            payload: json!({
                "tree": spread_tree,
                "args": with_w9(json!({ "bad name": "x" })),
            }),
        },
        IsoCase::Compare {
            name: "spread_srcset_routes_like_the_attribute".to_string(),
            source: iso_wrapped(spread_body),
            payload: json!({
                "tree": spread_tree,
                "args": with_w9(json!({ "srcset": "a.png 1x, javascript:x 2x" })),
            }),
        },
        IsoCase::Compare {
            name: "spread_url_values_blocked_per_key".to_string(),
            source: iso_wrapped(spread_body),
            payload: json!({
                "tree": spread_tree,
                "args": with_w9(json!({
                    "src": "vbscript:x",
                    "cite": "data:text/html,x",
                    "href": " java\tscript:x",
                })),
            }),
        },
    ]
}

/// The §21.2 self-closing divergence class: voids serialize `<br>`, a
/// non-void self-closing tag expands `<div></div>` (§4.2).
fn iso_shape_cases() -> Vec<IsoCase> {
    let args = base_args();
    vec![
        IsoCase::Compare {
            name: "void_self_closing_serializes_bare".to_string(),
            source: iso_wrapped(
                "<div><br /><hr /><img src=\"a.png\" alt=\"i\" /><input checked /></div>",
            ),
            payload: json!({
                "tree": iso_el("div", json!([]), json!([
                    iso_el("br", json!([]), json!([])),
                    iso_el("hr", json!([]), json!([])),
                    iso_el("img", json!([iso_stat("src", "a.png"), iso_stat("alt", "i")]), json!([])),
                    iso_el("input", json!([iso_bare("checked")]), json!([])),
                ])),
                "args": args,
            }),
        },
        IsoCase::Compare {
            name: "non_void_self_closing_expands".to_string(),
            source: iso_wrapped("<><div /><span /></>"),
            payload: json!({
                "tree": iso_frag(json!([
                    iso_el("div", json!([]), json!([])),
                    iso_el("span", json!([]), json!([])),
                ])),
                "args": args,
            }),
        },
    ]
}

/// URL attributes: scheme allowlist on the dynamic path, verbatim static
/// folds (§7.1, §4.4 static row).
fn iso_url_cases() -> Vec<IsoCase> {
    let href_tree = || {
        iso_el(
            "a",
            json!([iso_dyna("href", iso_arg(1))]),
            json!([iso_text("t")]),
        )
    };
    vec![
        IsoCase::Compare {
            name: "url_dynamic_allowlist".to_string(),
            source: iso_wrapped("<a href={$w1}>t</a>"),
            payload: json!({
                "tree": href_tree(),
                "args": with_arg(1, json!({ "k": "string", "v": "java\tscript:x" })),
            }),
        },
        IsoCase::Compare {
            name: "url_entity_encoded_stays_inert".to_string(),
            source: iso_wrapped("<a href={$w1}>t</a>"),
            payload: json!({
                "tree": href_tree(),
                "args": with_arg(1, json!({ "k": "string", "v": "&#x6A;avascript:alert(1)" })),
            }),
        },
        IsoCase::Compare {
            name: "static_url_folds_verbatim".to_string(),
            source: iso_wrapped("<a href=\"javascript:alert(1)\">t</a>"),
            payload: json!({
                "tree": iso_el("a", json!([iso_stat("href", "javascript:alert(1)")]), json!([iso_text("t")])),
                "args": base_args(),
            }),
        },
    ]
}

/// Children of every §4.5 value class, components (§4.7 named-arg contract),
/// raw-text elements (§4.7.1), style maps, fragments and comments.
fn iso_value_cases() -> Vec<IsoCase> {
    let args = base_args();
    vec![
        IsoCase::Compare {
            name: "children_value_classes".to_string(),
            source: iso_wrapped("<div>{$w0}{$w2}{$w3}{$w4}{$w5}{$w6}{$w7}{$w8}</div>"),
            payload: json!({
                "tree": iso_el("div", json!([]), json!([
                    iso_expr(iso_arg(0)), iso_expr(iso_arg(2)), iso_expr(iso_arg(3)),
                    iso_expr(iso_arg(4)), iso_expr(iso_arg(5)), iso_expr(iso_arg(6)),
                    iso_expr(iso_arg(7)), iso_expr(iso_arg(8)),
                ])),
                "args": args,
            }),
        },
        IsoCase::Compare {
            name: "component_named_args_and_children".to_string(),
            source: iso_wrapped("<Card variant={$w0} cls=\"padded\"><b>in</b></Card>"),
            payload: json!({
                "tree": iso_comp("Card", json!({ "variant": iso_arg(0), "cls": { "k": "string", "v": "padded" } }),
                    json!([iso_el("b", json!([]), json!([iso_text("in")]))])),
                "args": args,
            }),
        },
        IsoCase::Compare {
            name: "component_bare_on_and_int_label".to_string(),
            source: iso_wrapped("<Badge label={$w2} on />"),
            payload: json!({
                "tree": iso_comp("Badge", json!({ "label": iso_arg(2), "on": { "k": "bool", "v": true } }), json!([])),
                "args": args,
            }),
        },
        IsoCase::Compare {
            name: "component_self_closing_no_children".to_string(),
            source: iso_wrapped("<Card variant=\"wide\" />"),
            payload: json!({
                "tree": iso_comp("Card", json!({ "variant": { "k": "string", "v": "wide" } }), json!([])),
                "args": args,
            }),
        },
        IsoCase::Compare {
            name: "raw_text_script_json_island".to_string(),
            source: iso_wrapped(
                "<div><script>{\"a\": 1}</script><style>a[href] { color: red }</style></div>",
            ),
            payload: json!({
                "tree": iso_el("div", json!([]), json!([
                    iso_text("<script>{\"a\": 1}</script>"),
                    iso_text("<style>a[href] { color: red }</style>"),
                ])),
                "args": args,
            }),
        },
        IsoCase::Compare {
            name: "style_map_and_scalar".to_string(),
            source: iso_wrapped(
                "<div style={['color' => 'red', 'margin' => '10px auto']} style2={$w0}>t</div>",
            ),
            payload: json!({
                "tree": iso_el("div", json!([
                    iso_dyna("style", json!({ "k": "stylepairs", "v": [["color", "red"], ["margin", "10px auto"]] })),
                    iso_dyna("style2", iso_arg(0)),
                ]), json!([iso_text("t")])),
                "args": args,
            }),
        },
        IsoCase::Compare {
            name: "fragment_comment_and_text_mix".to_string(),
            source: iso_wrapped("<>{/* c */}a & b{$w2}<b>x</b></>"),
            payload: json!({
                "tree": iso_frag(json!([
                    json!({ "t": "comment" }),
                    iso_text("a & b"),
                    iso_expr(iso_arg(2)),
                    iso_el("b", json!([]), json!([iso_text("x")])),
                ])),
                "args": args,
            }),
        },
    ]
}

/// Compile rejections pinned to their diagnostic codes.
fn iso_reject_cases() -> Vec<IsoCase> {
    vec![
        IsoCase::Reject {
            name: "dynamic_on_handler_rejected".to_string(),
            source: iso_wrapped("<div onclick={$w1}>t</div>"),
            code: "LNT0301",
        },
        IsoCase::Reject {
            name: "hyphenated_component_prop_rejected".to_string(),
            source: iso_wrapped("<Card data-x=\"1\" />"),
            code: "LNT0202",
        },
    ]
}

fn iso_cases() -> Vec<IsoCase> {
    let mut cases = Vec::new();
    cases.extend(iso_class_cases());
    cases.extend(iso_spread_cases());
    cases.extend(iso_shape_cases());
    cases.extend(iso_url_cases());
    cases.extend(iso_value_cases());
    cases.extend(iso_reject_cases());
    cases
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
