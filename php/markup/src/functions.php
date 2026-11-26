<?php

declare(strict_types=1);

namespace Markup;

/**
 * The RFC-parity rendering contract (docs/rfc-alignment.md: "Escaping is
 * the runtime's contract (escape-by-default)"). The compiler emits raw PHP
 * expressions — no `\Lintl\` helpers — so this runtime owns escaping at
 * print time under the SAME rules the string target's helpers enforce
 * (doc §6.8 attribute routing, §7.1 srcset): children escape by default
 * with `\Markup\Html` as the trusted sink; attribute values route by
 * attribute name (URL allowlist / class list / style charset / srcset
 * candidates); `on*` handlers and unsafe names throw.
 *
 * This package is deliberately standalone (no Lintl dependency): RFC
 * output must run with zero lock-in — the pitch of `lintl migrate
 * --to=native` (doc §0.1/§17).
 */
const FLAGS = ENT_QUOTES | ENT_SUBSTITUTE | ENT_HTML5;

/** Attribute names routed to the URL scheme allowlist — the same list the
 * string lowering emits `\Lintl\u()` for (emit.rs URL_ATTRS, doc §4.4). */
const URL_ATTRS = [
    'href' => true, 'src' => true, 'action' => true, 'formaction' => true,
    'poster' => true, 'cite' => true, 'data' => true, 'manifest' => true,
    'ping' => true, 'background' => true,
];

function e(string $s): string
{
    return \htmlspecialchars($s, FLAGS, 'UTF-8');
}

/** Render any RFC child value to an HTML string (escape-by-default). */
function render(mixed $v): string
{
    if ($v instanceof Element || $v instanceof Fragment) {
        return $v->render();
    }
    if ($v instanceof Html) {
        return $v->s;
    }
    if ($v instanceof \Traversable) {
        $o = '';
        foreach ($v as $x) {
            $o .= render($x);
        }

        return $o;
    }
    if (\is_string($v)) {
        return e($v);
    }
    if ($v === null || \is_bool($v)) {
        return '';
    }
    if (\is_int($v) || \is_float($v)) {
        return (string) $v;
    }
    if (\is_iterable($v)) {
        $o = '';
        foreach ($v as $x) {
            $o .= render($x);
        }

        return $o;
    }
    if ($v instanceof \Stringable) {
        return e((string) $v);
    }
    throw new Exception('Cannot render '.\get_debug_type($v).' as a markup child');
}

/** URL attribute value: scheme allowlist (browsers ignore ws/ctl inside schemes). */
function url_attr(string $name, mixed $v): string
{
    if ($v === null || $v === false) {
        return '';
    }
    // @phpstan-ignore cast.string (the RFC contract: any scalar attribute value renders — doc §6.8)
    $s = (string) $v; // @pun-ignore analyse/types/invalid-cast -- the RFC contract: any scalar attribute value renders — doc §6.8
    $probe = \preg_replace('/[\x00-\x20\x7f]+/', '', $s) ?? '';
    if (\preg_match('/^([a-z][a-z0-9+.\-]*):/i', $probe, $m)
        && ! \in_array(\strtolower($m[1]), ['http', 'https', 'mailto', 'tel', 'sms'], true)) {
        $s = 'about:invalid#blocked';
    }

    return ' '.$name.'="'.e($s).'"';
}

/** srcset attribute: split candidates and scheme-check each URL (doc §7.1). */
function srcset_attr(string $name, mixed $v): string
{
    if ($v === null || $v === false) {
        return '';
    }
    // @phpstan-ignore cast.string (docs/srcset-alignment.md: the candidate list arrives as any scalar — the cast is the contract)
    $s = (string) $v; // @pun-ignore analyse/types/invalid-cast -- docs/srcset-alignment.md: the candidate list arrives as any scalar — the cast is the contract
    $out = [];
    foreach (\preg_split('/,(?=\s|$)/', $s) ?: [] as $candidate) {
        $candidate = \trim($candidate);
        if ($candidate === '') {
            continue;
        }
        $tokens = \preg_split('/\s+/', $candidate) ?: [];
        if ($tokens === []) {
            continue;
        }
        $probe = \preg_replace('/[\x00-\x20\x7f]+/', '', $candidate) ?? '';
        if (\preg_match('/^([a-z][a-z0-9+.\-]*):/i', $probe, $m)
            && ! \in_array(\strtolower($m[1]), ['http', 'https', 'mailto', 'tel', 'sms'], true)) {
            $last = $tokens[\count($tokens) - 1];
            $desc = \count($tokens) > 1 && \preg_match('/^\d+(?:\.\d+)?[wx]$/', $last) === 1 ? $last : '';
            $out[] = $desc === '' ? 'about:invalid#blocked' : 'about:invalid#blocked '.$desc;

            continue;
        }
        $out[] = \implode(' ', $tokens);
    }
    if ($out === []) {
        return '';
    }

    return ' '.$name.'="'.e(\implode(', ', $out)).'"';
}

/** class={…}: string | list | ['cls' => bool] — the \Lintl\cls() shapes. */
function cls_attr(mixed $v): string
{
    $out = [];
    $walk = static function (mixed $x) use (&$out, &$walk): void {
        if (\is_string($x) && $x !== '') {
            $out[] = $x;
        } elseif (\is_array($x)) {
            foreach ($x as $k => $val) {
                if (\is_int($k)) {
                    $walk($val);
                } elseif ($val) {
                    $out[] = (string) $k;
                }
            }
        }
    };
    $walk($v);

    return $out === [] ? '' : ' class="'.e(\implode(' ', $out)).'"';
}

/** style={['prop' => 'value']} — values restricted to a safe charset (doc §6.8). */
function sty_attr(mixed $v): string
{
    if (! \is_array($v)) {
        return plain_attr('style', $v);
    }
    $o = [];
    foreach ($v as $k => $val) {
        if ($val === null || $val === false) {
            continue;
        }
        // @phpstan-ignore cast.string (doc §6.8: style values are scalar strings — the cast is the contract)
        $val = (string) $val; // @pun-ignore analyse/types/invalid-cast -- doc §6.8: style values are scalar strings — the cast is the contract
        if (! \preg_match('/^[\w\s#%.,()\-\/+*]*$/u', $val) || ! \preg_match('/^[a-z\-]+$/i', (string) $k)) {
            throw new Exception("Unsafe style value for \"$k\"");
        }
        $o[] = $k.':'.$val;
    }

    return $o === [] ? '' : ' style="'.e(\implode(';', $o)).'"';
}

/** Generic attribute: true → bare, false/null → omitted, scalar → quoted+escaped. */
function plain_attr(string $name, mixed $v): string
{
    if ($v === null || $v === false) {
        return '';
    }
    if ($v === true) {
        return ' '.$name;
    }
    if (\is_array($v)) {
        throw new Exception("Attribute \"$name\" received an array");
    }

    // @phpstan-ignore cast.string (doc §6.8: any scalar attribute value renders — the cast is the contract)
    return ' '.$name.'="'.e((string) $v).'"'; // @pun-ignore analyse/types/invalid-cast -- doc §6.8: any scalar attribute value renders — the cast is the contract
}

/** The attribute walk: name validation first, then the routing above.
 * @param  array<string|int, mixed>  $m
 */
function attr_string(array $m): string
{
    $o = '';
    foreach ($m as $k => $v) {
        $k = (string) $k;
        if (! \preg_match('/^[a-zA-Z_:][\w:.\-]*$/', $k) || \str_starts_with(\strtolower($k), 'on')) {
            throw new Exception("Unsafe attribute name \"$k\"");
        }
        $lk = \strtolower($k);
        $o .= match (true) {
            (URL_ATTRS[$lk] ?? false) === true => url_attr($k, $v),
            $lk === 'class' => cls_attr($v),
            $lk === 'style' => sty_attr($v),
            $lk === 'srcset' => srcset_attr($k, $v),
            default => plain_attr($k, $v),
        };
    }

    return $o;
}
