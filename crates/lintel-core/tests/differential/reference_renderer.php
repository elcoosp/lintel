<?php
declare(strict_types=1);

/**
 * The T-09 differential reference renderer (architecture doc §13.2).
 *
 * Hand-written from the *specification text* — §4 (language spec), §7.1
 * (context-aware escaping matrix) and §6.8's helper contract — NOT
 * transcribed from php/runtime/src/functions.php. `differential.rs` feeds
 * this script the same random tree JSON and the same argument values it
 * gave the compiled template, then compares stdout byte-for-byte:
 *
 *     compile -> php -l -> execute -> compare stdout
 *
 * Usage: php reference_renderer.php <payload.json>
 * Payload: {"tree": <node>, "args": [<spec>, ...]}
 * On a contract throw both sides print `THROW:InvalidArgumentException`.
 *
 * Spec notes baked in (each traced to the doc, §21.2 records the verdicts):
 * - Text children are JSX *markup text*: verbatim, no entity decode, no
 *   re-escaping (§4.5 escapes only `{expr}` children; the static row of
 *   §4.4 keeps literal values, re-escaping only `"` -> `&quot;`).
 * - Void elements (§4.2) serialize `<br>`; a non-void self-closing tag
 *   expands `<div></div>`; fragments add no wrapper (§4.3).
 * - URL attributes go through the scheme allowlist (§7.1) on dynamic
 *   values; static values fold verbatim (§4.4's static row).
 * - class/style/spread routing, bool/null attribute omission, child-value
 *   rules and every throw follow §4.4/§4.5/§7.1 exactly.
 */

const R_FLAGS = ENT_QUOTES | ENT_SUBSTITUTE | ENT_HTML5;

/** §7.1 HTML-text escaping: htmlspecialchars(ENT_QUOTES|SUBSTITUTE|HTML5). */
function r_e(string $s): string
{
    return \htmlspecialchars($s, R_FLAGS, 'UTF-8');
}

/** Marker wrapper standing in for the runtime `Html` value type (§4.5). */
final class RHtml
{
    public function __construct(public readonly string $s) {}
}

/** Stringable stand-in (§4.5: Stringable children escape). */
final class RStringable implements \Stringable
{
    public function __construct(public readonly string $s) {}
    public function __toString(): string { return $this->s; }
}

/** §4.5 child-value rules. */
function r_child(mixed $v): string
{
    if ($v instanceof RHtml) return $v->s;
    if (\is_string($v)) return r_e($v);
    if ($v === null || \is_bool($v)) return '';
    if (\is_int($v) || \is_float($v)) return (string) $v;
    if (\is_iterable($v)) { $o = ''; foreach ($v as $x) { $o .= r_child($x); } return $o; }
    if ($v instanceof \Stringable) return r_e((string) $v);
    throw new \InvalidArgumentException('Cannot render child');
}

/** §7.1 `a()`: bool/null omission, scalar escaping. */
function r_a(string $name, mixed $v): string
{
    if ($v === null || $v === false) return '';
    if ($v === true) return ' ' . $name;
    if (\is_array($v)) throw new \InvalidArgumentException("Attribute \"$name\" received an array");
    return ' ' . $name . '="' . r_e((string) $v) . '"';
}

/** §7.1 `u()`: strip ws/ctl for probing -> allowlist + relative, else blocked. */
function r_u(string $name, mixed $v): string
{
    if ($v === null || $v === false) return '';
    $s = (string) $v;
    $probe = \preg_replace('/[\x00-\x20\x7f]+/', '', $s) ?? '';
    if (\preg_match('/^([a-z][a-z0-9+.\-]*):/i', $probe, $m)
        && !\in_array(\strtolower($m[1]), ['http', 'https', 'mailto', 'tel', 'sms'], true)) {
        $s = 'about:invalid#blocked';
    }
    return ' ' . $name . '="' . r_e($s) . '"';
}

/** §7.1 `srcset()`: split candidates, scheme-check each URL. */
function r_srcset(string $name, mixed $v): string
{
    if ($v === null || $v === false) return '';
    $s = (string) $v;
    $out = [];
    foreach (\preg_split('/,(?=\s|$)/', $s) ?: [] as $candidate) {
        $candidate = \trim($candidate);
        if ($candidate === '') continue;
        $tokens = \preg_split('/\s+/', $candidate) ?: [];
        if ($tokens === []) continue;
        $probe = \preg_replace('/[\x00-\x20\x7f]+/', '', $candidate) ?? '';
        if (\preg_match('/^([a-z][a-z0-9+.\-]*):/i', $probe, $m)
            && !\in_array(\strtolower($m[1]), ['http', 'https', 'mailto', 'tel', 'sms'], true)) {
            $last = $tokens[\count($tokens) - 1];
            $desc = \count($tokens) > 1 && \preg_match('/^\d+(?:\.\d+)?[wx]$/', $last) === 1 ? $last : '';
            $out[] = $desc === '' ? 'about:invalid#blocked' : 'about:invalid#blocked ' . $desc;
            continue;
        }
        $out[] = \implode(' ', $tokens);
    }
    if ($out === []) return '';
    return ' ' . $name . '="' . r_e(\implode(', ', $out)) . '"';
}

/** §4.4/§7.1 `cls()`: string | list | ['cls' => bool], nested lists. */
function r_cls(mixed $v): string
{
    $out = [];
    $walk = static function (mixed $x) use (&$out, &$walk): void {
        if (\is_string($x) && $x !== '') { $out[] = $x; }
        elseif (\is_array($x)) {
            foreach ($x as $k => $val) {
                if (\is_int($k)) { $walk($val); }
                elseif ($val) { $out[] = $k; }
            }
        }
    };
    $walk($v);
    return $out === [] ? '' : ' class="' . r_e(\implode(' ', $out)) . '"';
}

/** §7.1 `sty()`: map with charset-restricted values. */
function r_sty(mixed $v): string
{
    if (!\is_array($v)) return r_a('style', $v);
    $o = [];
    foreach ($v as $k => $val) {
        if ($val === null || $val === false) continue;
        $val = (string) $val;
        if (!\preg_match('/^[\w\s#%.,()\-\/+*]*$/u', $val) || !\preg_match('/^[a-z\-]+$/i', (string) $k)) {
            throw new \InvalidArgumentException("Unsafe style value for \"$k\"");
        }
        $o[] = $k . ':' . $val;
    }
    return $o === [] ? '' : ' style="' . r_e(\implode(';', $o)) . '"';
}

/** §4.4 spread: per-key routing + `on*`/unsafe-name rejection (§7.2). */
function r_spread(array $m): string
{
    $o = '';
    foreach ($m as $k => $v) {
        $k = (string) $k;
        if (!\preg_match('/^[a-zA-Z_:][\w:.\-]*$/', $k) || \str_starts_with(\strtolower($k), 'on')) {
            throw new \InvalidArgumentException("Unsafe attribute name \"$k\"");
        }
        $lk = \strtolower($k);
        $o .= match (true) {
            \in_array($lk, ['href', 'src', 'action', 'formaction', 'poster', 'cite', 'data'], true) => r_u($k, $v),
            $lk === 'class' => r_cls($v),
            $lk === 'style' => r_sty($v),
            $lk === 'srcset' => r_srcset($k, $v),
            default => r_a($k, $v),
        };
    }
    return $o;
}

/** One attribute-form node (mirrors the §4.4 table). */
function r_attr_node(array $a, array $args): string
{
    return match ($a['a']) {
        'static' => ' ' . $a['name'] . '="' . \str_replace('"', '&quot;', (string) $a['v']) . '"',
        'dyn' => r_route($a['name'], r_materialize($a['spec'], $args), $args),
        'bare' => ' ' . $a['name'],
        'spread' => r_spread(r_materialize($a['spec'], $args)),
        'class' => r_cls(r_materialize($a['spec'], $args)),
        'style' => r_sty(r_materialize($a['spec'], $args)),
        'srcset' => r_srcset($a['name'], r_materialize($a['spec'], $args)),
        default => throw new \LogicException('bad attr node'),
    };
}

/** Dynamic-value routing by attribute name (§4.4 URL/class/style rows). */
function r_route(string $name, mixed $v, array $args): string
{
    $lk = \strtolower($name);
    if ($lk === 'class') return r_cls($v);
    if ($lk === 'style') return r_sty($v);
    if ($lk === 'srcset') return r_srcset($name, $v);
    if (\in_array($lk, ['href', 'src', 'action', 'formaction', 'poster', 'cite', 'data'], true)) {
        return r_u($name, $v);
    }
    return r_a($name, $v);
}

/** Value-spec materialisation: arg back-reference or inline constant. */
function r_materialize(array $spec, array $args): mixed
{
    if (isset($spec['arg'])) {
        return $args[$spec['arg']];
    }
    return match ($spec['k']) {
        'string' => (string) $spec['v'],
        'int', 'float', 'bool' => $spec['v'],
        'null' => null,
        'array' => $spec['v'],
        'stringable' => new RStringable((string) $spec['v']),
        'html' => new RHtml((string) $spec['v']),
        'clstpl' => r_clstpl($spec['v'], $args),
        'stylepairs' => \array_column($spec['v'], 1, 0),
        default => throw new \LogicException('bad spec'),
    };
}

/** Class-list template (§4.4): values, `'cls' => cond`, nested lists. */
function r_clstpl(array $entries, array $args): array
{
    $v = [];
    foreach ($entries as $e) {
        if (\is_string($e)) {
            $v[] = $e;
        } elseif (isset($e['nest'])) {
            $v[] = r_clstpl($e['nest'], $args);
        } elseif (isset($e['kv'])) {
            if (r_materialize($e['cond'], $args)) {
                $v[$e['kv']] = true;
            }
        } else {
            $v[] = r_materialize($e, $args);
        }
    }
    return $v;
}

/** Fixed component bodies the generator emits (§4.7 named-arg contract). */
function r_component(string $name, array $props, array $args): string
{
    if ($name === 'Card') {
        $cls = isset($props['cls']) ? r_materialize($props['cls'], $args) : null;
        $class = r_cls(['card', 'card--' . (string) r_materialize($props['variant'], $args), $cls]);
        $children = '';
        if (isset($props['children_nodes'])) {
            $children = \array_reduce($props['children_nodes'], fn(string $c, array $n): string => $c . r_node($n, $args), '');
        } elseif (isset($props['children'])) {
            $children = r_child(r_materialize($props['children'], $args));
        }
        return '<section' . $class . '>' . $children . '</section>';
    }
    if ($name === 'Badge') {
        $label = r_materialize($props['label'], $args);
        $on = isset($props['on']) ? r_materialize($props['on'], $args) : false;
        return '<b' . r_a('data-on', $on) . '>' . r_child($label) . '</b>';
    }
    throw new \LogicException("unknown component $name");
}

/** One JSX tree node. Text nodes stay verbatim (see the header note). */
function r_node(array $n, array $args): string
{
    switch ($n['t']) {
        case 'el':
            $attrs = '';
            foreach ($n['attrs'] as $a) { $attrs .= r_attr_node($a, $args); }
            $open = '<' . $n['tag'] . $attrs . '>';
            if ($n['void']) return $open;
            $children = '';
            foreach ($n['children'] as $c) { $children .= r_node($c, $args); }
            return $open . $children . '</' . $n['tag'] . '>';
        case 'frag':
            $children = '';
            foreach ($n['children'] as $c) { $children .= r_node($c, $args); }
            return $children;
        case 'text':
            return (string) $n['src'];
        case 'expr':
            return r_child(r_materialize($n['spec'], $args));
        case 'comment':
            return '';
        case 'comp':
            return r_component($n['name'], $n['props'], $args);
        default:
            throw new \LogicException('bad node');
    }
}

$payload = \json_decode(\file_get_contents($argv[1] ?? '-'), true);
if (!\is_array($payload) || !isset($payload['tree'], $payload['args'])) {
    fwrite(STDERR, "usage: php reference_renderer.php <payload.json>\n");
    exit(2);
}
$args = \array_map(static fn(array $spec): mixed => r_materialize($spec, $payload['args']), $payload['args']);
try {
    echo r_node($payload['tree'], $args);
} catch (\InvalidArgumentException $e) {
    echo 'THROW:InvalidArgumentException';
}
