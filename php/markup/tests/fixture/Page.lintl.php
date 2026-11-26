<?php

// The RFC-convergence e2e fixture (MarkupE2eTest): one source compiles to
// BOTH targets and renders to identical markup — the string target through
// \Lintl\c()/Html, the rfc target through the \Markup\ runtime. The
// component bodies stay target-neutral: `{$children}` compiles to
// \Lintl\c($children) under the string target and to a raw $children
// array entry under the rfc target, which the runtime walks (see the
// lowering contracts in docs/rfc-alignment.md).
function Card(string $title, $children = null) {
    return <div class="card">
        <h2>{$title}</h2>
        {$children}
        <hr />
    </div>;
}

function Page(string $name, string $home) {
    return <Card title={$name}>
        <p>visit <a href={$home}>home</a></p>
        <b class="tag">t</b>
        {$name ? <em class="who">{$name}</em> : null}
    </Card>;
}
