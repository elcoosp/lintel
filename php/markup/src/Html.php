<?php

declare(strict_types=1);

namespace Markup;

/**
 * The RFC's trusted sink (docs/rfc-alignment.md: "`\Markup\Html` is a
 * runtime concern outside the compiler's output shape"). Author-trusted
 * markup rides through rendering verbatim; everything else — every plain
 * string child and scalar attribute value — prints escaped. Mirrors the
 * shape of `\Lintel\Html` so the two runtimes stay interchangeable at the
 * call sites the string target compiles to.
 */
final class Html implements \Stringable
{
    public function __construct(public readonly string $s) {}

    public function __toString(): string
    {
        return $this->s;
    }

    /** Greppable escape hatch — the only way raw markup enters a render tree. */
    public static function unsafeRaw(string $html): self
    {
        return new self($html);
    }
}
