<?php

declare(strict_types=1);

namespace Lintl\Fixtures;

use Lintl\Html;

// The clean fixture (the type-inference half): compiles with zero
// diagnostics — the compiler proves no type mismatch — but the stub-typed
// signature makes PHPStan flag the int passed where the component declares
// `string $title`, reported on THIS source line by line preservation.
function Card(string $title, int $count, ?Html $children = null): Html
{
    return <div class="card">{$title}{$children}</div>;
}

echo <Card title={42} count={3}><p>Body</p></Card>;
