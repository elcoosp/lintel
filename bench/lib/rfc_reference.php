<?php

/**
 * Measurement-only reference shape of the Native Markup Expressions RFC
 * lowering (`new \Markup\Element(tag, attrs, children)` — docs/
 * rfc-alignment.md). The RFC itself is still under discussion and ships no
 * runtime; this stub exists so the `--target=rfc` competitor is *executed*
 * in the bench harness instead of skipped, under the SAME escaping contract
 * the doc pins (§4.9: "Escape-by-default ✔ + context-aware" on the Lintel
 * side, escape-by-default is the RFC runtime's job). It is not part of the
 * compiler, the runtime or any shipped package.
 */

declare(strict_types=1);

namespace Markup;

final class Element implements \Stringable
{
    /** @param array<string|int, mixed> $attrs @param array<int, mixed> $children */
    public function __construct(
        public readonly string $tag,
        public readonly array $attrs = [],
        public readonly array $children = [],
    ) {}

    public function __toString(): string
    {
        return '<' . $this->tag . self::attrs($this->attrs) . '>'
            . self::children($this->children)
            . '</' . $this->tag . '>';
    }

    /** @param array<string|int, mixed> $attrs */
    private static function attrs(array $attrs): string
    {
        $o = '';
        foreach ($attrs as $name => $value) {
            $name = (string) $name;
            if ($value === null || $value === false) {
                continue;
            }
            if ($value === true) {
                $o .= ' ' . $name;
                continue;
            }
            // Class-list arrays follow the same list/map semantics as the
            // §6.8 cls() helper (['a', 'b' => bool]).
            if (\is_array($value)) {
                $parts = [];
                foreach ($value as $k => $v) {
                    if (\is_int($k)) {
                        if (\is_string($v) && $v !== '') {
                            $parts[] = $v;
                        }
                    } elseif ($v) {
                        $parts[] = (string) $k;
                    }
                }
                $value = \implode(' ', $parts);
            }
            $o .= ' ' . $name . '="' . \htmlspecialchars((string) $value, \ENT_QUOTES | \ENT_SUBSTITUTE | \ENT_HTML5, 'UTF-8') . '"';
        }

        return $o;
    }

    /** @param array<int, mixed> $children */
    private static function children(array $children): string
    {
        $o = '';
        foreach ($children as $child) {
            if ($child instanceof \Stringable) {
                $o .= (string) $child;
            } elseif (\is_string($child)) {
                $o .= \htmlspecialchars($child, \ENT_QUOTES | \ENT_SUBSTITUTE | \ENT_HTML5, 'UTF-8');
            } elseif (\is_iterable($child)) {
                $o .= self::children($child);
            } elseif ($child === null || \is_bool($child)) {
                continue;
            } elseif (\is_int($child) || \is_float($child)) {
                $o .= (string) $child;
            } else {
                throw new \InvalidArgumentException('Cannot render ' . \get_debug_type($child) . ' as an RFC child');
            }
        }

        return $o;
    }
}
