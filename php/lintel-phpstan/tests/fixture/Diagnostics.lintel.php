<?php

declare(strict_types=1);

namespace Lintel\Fixtures;

use Lintel\Html;

// The diagnostics fixture (the LNT-mapping half): two compiler rejections
// — the missing required parameter (LNT0205, line 7) and the unknown prop
// (LNT0203, line 8) — recorded by the bridge and reported by the PHPStan
// rule at these source lines.
function Card(string $title, int $count, ?Html $children = null): Html
{
    return <div class="card">{$title}{$children}</div>;
}

echo <Card />;
echo <Card title="X" extra="y" />;
