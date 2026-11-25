<?php

declare(strict_types=1);

namespace Markup;

/**
 * The RFC's fragment value (docs/rfc-alignment.md: "Fragments `<>…</>`
 * exist in the RFC syntax"). The compiler flattens nested fragments and
 * lowers a *root* fragment to a plain PHP array (alignment decision 4) —
 * both render through the same walk, so this class completes the RFC's
 * value model for hand-authored trees and component authors.
 *
 * @param  list<mixed>  $children
 */
final class Fragment implements \Stringable
{
    /** @param list<mixed> $children */
    public function __construct(public readonly array $children = []) {}

    public function render(): string
    {
        $out = '';
        foreach ($this->children as $child) {
            $out .= render($child);
        }

        return $out;
    }

    public function __toString(): string
    {
        return $this->render();
    }
}
