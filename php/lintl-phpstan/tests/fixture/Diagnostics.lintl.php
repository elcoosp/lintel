<?php

declare(strict_types=1);

namespace Lintl\Fixtures;

use Lintl\Html;

// The diagnostics fixture (the LNL-mapping half): two compiler rejections
// — the missing required parameter (LNL0205, line 7) and the unknown prop
// (LNL0203, line 8) — recorded by the bridge and reported by the PHPStan
// rule at these source lines.
function Card(string $title, int $count, ?Html $children = null): Html
{
    return <div class="card">{$title}{$children}</div>;
}

echo <Card />;
echo <Card title="X" extra="y" />;
