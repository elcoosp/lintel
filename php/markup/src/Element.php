<?php

declare(strict_types=1);

namespace Markup;

/**
 * The RFC's element value (`docs/rfc-alignment.md`: markup lowers to
 * `new \Markup\Element('button', ['class' => 'btn'], ['Sign in'])`).
 *
 * Rendering is escape-by-default: children print through
 * {@see Markup\render()} (strings escaped, `\Markup\Html` trusted,
 * nested values walked) and attribute values print through the same
 * attribute routing the string target's emitted `\Lintel\a()/u()/cls()/
 * sty()/srcset()` calls enforce — URL attributes scheme-check, class
 * lists/maps join, style maps ride the safe charset, srcset candidates
 * split and scheme-check, `on*` handlers and invalid names are rejected.
 * Void elements render themselves (`<br>`); giving them children is a bug
 * and throws.
 */
final class Element implements \Stringable
{
    private const VOID_TAGS = [
        'area' => true, 'base' => true, 'br' => true, 'col' => true,
        'embed' => true, 'hr' => true, 'img' => true, 'input' => true,
        'link' => true, 'meta' => true, 'param' => true, 'source' => true,
        'track' => true, 'wbr' => true,
    ];

    /**
     * @param  array<string|int, mixed>  $attrs
     * @param  list<mixed>  $children
     */
    public function __construct(
        public readonly string $tag,
        public readonly array $attrs = [],
        public readonly array $children = [],
    ) {}

    public function render(): string
    {
        $tag = $this->tag;
        if (\preg_match('/^[a-zA-Z][a-zA-Z0-9\-]*$/', $tag) !== 1) {
            throw new Exception("Unsafe element tag \"$tag\"");
        }
        $lower = \strtolower($tag);
        $out = '<'.$tag.attr_string($this->attrs);
        if (self::VOID_TAGS[$lower] ?? false) {
            if ($this->children !== []) {
                throw new Exception("Void element <$tag> cannot have children");
            }

            return $out.'>';
        }
        $out .= '>';
        foreach ($this->children as $child) {
            $out .= render($child);
        }

        return $out.'</'.$tag.'>';
    }

    public function __toString(): string
    {
        return $this->render();
    }
}
