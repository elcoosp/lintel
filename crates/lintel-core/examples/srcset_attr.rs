//! Consumer example: the `srcset` per-URL attribute routing (architecture
//! doc §4.4 URL-attribute table, §7.1 escaping matrix; the exact contract is
//! recorded in `docs/srcset-alignment.md`). A dynamic `srcset={expr}`
//! attribute lowers to the dedicated `\Lintel\srcset('srcset', expr)` helper
//! call — the runtime helper splits the candidate list and scheme-checks
//! each URL — instead of the generic `\Lintel\a()` attribute escape. Static
//! string values fold verbatim like every other author-trusted attribute,
//! and component attributes stay named arguments.
//!
//! Run:
//!
//! ```sh
//! cargo run -p lintel-core --example srcset_attr
//! ```

use lintel_core::{compile, Options};

const SOURCE: &str = r#"<?php
function Gallery(array $variants, array $sources): Html {
    return <main class="gallery">
        <img src={$sources[0]} srcset={$variants[0]} alt="hero" />
        <img srcset="a.png 1x, b.png 2x" alt="static" />
        <img src={$sources[1]} alt="plain" />
    </main>;
}
return <Gallery variants={$v} sources={$s} />;
"#;

fn main() {
    let out = compile(SOURCE, &Options::default()).expect("the gallery compiles");
    println!("{SOURCE}");
    println!("── emitted PHP ─────────────────────────────────────────");
    println!("{}", out.php);
    println!("── routing ─────────────────────────────────────────────");
    println!("dynamic srcset={{…}} → \\Lintel\\srcset('srcset', …)  (per-candidate URL checks)");
    println!("static  srcset=\"…\"  → folded verbatim (author-trusted)");
    println!("dynamic src={{…}}   → \\Lintel\\u('src', …)          (single-URL scheme allowlist)");
    println!("component attribute → named argument, no helper");
}
