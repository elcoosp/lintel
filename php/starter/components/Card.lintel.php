<?php

declare(strict_types=1);

namespace App;

use Lintel\Html;

/**
 * A Card component — the starter's first real component. Demonstrates:
 *  - the `Html` return type (the value the string target emits)
 *  - a typed `string` parameter (the escaper folds it directly through
 *    htmlspecialchars, no generic helper — see docs/escape-elision.md)
 *  - a dynamic child `{$title}` (escaped by default)
 *  - a dynamic URL attribute `href={$url}` (scheme-allowlisted by u())
 *  - a boolean attribute `disabled={$disabled}` (true renders bare)
 */
function Card(string $title, string $url, bool $disabled = false): Html
{
    return <article class="card">
        <h2>{$title}</h2>
        <a href={$url}>Open</a>
        <button disabled={$disabled}>{$disabled ? 'Unavailable' : 'Click'}</button>
    </article>;
}
