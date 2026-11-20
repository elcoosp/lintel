<?php declare(strict_types=1);
namespace Lintel;

final class Html implements \Stringable
{
    public function __construct(public readonly string $s) {}
    public function __toString(): string { return $this->s; }
    /** Greppable escape hatch. */
    public static function unsafeRaw(string $html): self { return new self($html); }
}

// Author-facing signatures are written unqualified (`function Badge(): Html`,
// doc §6.9 / the `lintel init` demo), and compiled output is global-namespace
// PHP: alias the global name so those type hints resolve to the runtime class.
if (!\class_exists('Html', false)) {
    \class_alias(Html::class, 'Html');
}

const FLAGS = ENT_QUOTES | ENT_SUBSTITUTE | ENT_HTML5;

function e(string $s): string { return \htmlspecialchars($s, FLAGS, 'UTF-8'); }

/** Render any child value to an HTML string. */
function c(mixed $v): string
{
    if ($v instanceof Html) return $v->s;
    if (\is_string($v))     return \htmlspecialchars($v, FLAGS, 'UTF-8');
    if ($v === null || \is_bool($v)) return '';
    if (\is_int($v) || \is_float($v)) return (string) $v;
    if (\is_iterable($v)) { $o = ''; foreach ($v as $x) { $o .= c($x); } return $o; }
    if ($v instanceof \Stringable) return \htmlspecialchars((string) $v, FLAGS, 'UTF-8');
    throw new \InvalidArgumentException('Cannot render ' . \get_debug_type($v) . ' as a child');
}

/** Generic attribute. */
function a(string $name, mixed $v): string
{
    if ($v === null || $v === false) return '';
    if ($v === true) return ' ' . $name;
    if (\is_array($v)) throw new \InvalidArgumentException("Attribute \"$name\" received an array");
    return ' ' . $name . '="' . \htmlspecialchars((string) $v, FLAGS, 'UTF-8') . '"'; // @phpstan-ignore cast.string (doc §6.8: any scalar attribute value renders — the cast is the contract)
}

/** URL attribute: scheme allowlist. Percent-encoding is NOT a defense against javascript:. */
function u(string $name, mixed $v): string
{
    if ($v === null || $v === false) return '';
    $s = (string) $v; // @phpstan-ignore cast.string (doc §6.8: stringable URL values render — the cast is the contract)
    $probe = \preg_replace('/[\x00-\x20\x7f]+/', '', $s) ?? '';   // browsers ignore ws/ctl inside schemes
    if (\preg_match('/^([a-z][a-z0-9+.\-]*):/i', $probe, $m)
        && !\in_array(\strtolower($m[1]), ['http', 'https', 'mailto', 'tel', 'sms'], true)) {
        $s = 'about:invalid#blocked';
    }
    return ' ' . $name . '="' . \htmlspecialchars($s, FLAGS, 'UTF-8') . '"';
}

/** srcset attribute: split candidates and scheme-check each URL (§7.1; docs/srcset-alignment.md). */
function srcset(string $name, mixed $v): string
{
    if ($v === null || $v === false) return '';
    $s = (string) $v; // @phpstan-ignore cast.string (docs/srcset-alignment.md: the candidate list arrives as any scalar — the cast is the contract)
    $out = [];
    // A comma separates candidates only when followed by whitespace or the
    // end of the value — `data:text/html,x 4x` keeps its URL-internal comma.
    foreach (\preg_split('/,(?=\s|$)/', $s) ?: [] as $candidate) {
        $candidate = \trim($candidate);
        if ($candidate === '') continue;
        $tokens = \preg_split('/\s+/', $candidate) ?: [];
        if ($tokens === []) continue;
        // Probe the whole candidate: browsers ignore ws/ctl inside schemes,
        // so "java\tscript:x 2x" is one obfuscated URL, not URL + descriptor.
        $probe = \preg_replace('/[\x00-\x20\x7f]+/', '', $candidate) ?? '';
        if (\preg_match('/^([a-z][a-z0-9+.\-]*):/i', $probe, $m)
            && !\in_array(\strtolower($m[1]), ['http', 'https', 'mailto', 'tel', 'sms'], true)) {
            // Hostile URL: keep only a trailing width/density descriptor.
            $last = $tokens[\count($tokens) - 1];
            $desc = \count($tokens) > 1 && \preg_match('/^\d+(?:\.\d+)?[wx]$/', $last) === 1 ? $last : '';
            $out[] = $desc === '' ? 'about:invalid#blocked' : 'about:invalid#blocked ' . $desc;
            continue;
        }
        $out[] = \implode(' ', $tokens);
    }
    if ($out === []) return '';
    return ' ' . $name . '="' . \htmlspecialchars(\implode(', ', $out), FLAGS, 'UTF-8') . '"';
}

/** class={…}: string | list | ['cls' => bool] */
function cls(mixed $v): string
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
    return $out === [] ? '' : ' class="' . \htmlspecialchars(\implode(' ', $out), FLAGS, 'UTF-8') . '"';
}

/** style={['prop' => 'value']} — values restricted to a safe charset. */
function sty(mixed $v): string
{
    if (!\is_array($v)) return a('style', $v);
    $o = [];
    foreach ($v as $k => $val) {
        if ($val === null || $val === false) continue;
        $val = (string) $val; // @phpstan-ignore cast.string (doc §6.8: style values are scalar strings — the cast is the contract)
        if (!\preg_match('/^[\w\s#%.,()\-\/+*]*$/u', $val) || !\preg_match('/^[a-z\-]+$/i', (string) $k)) {
            throw new \InvalidArgumentException("Unsafe style value for \"$k\"");
        }
        $o[] = $k . ':' . $val;
    }
    return $o === [] ? '' : ' style="' . \htmlspecialchars(\implode(';', $o), FLAGS, 'UTF-8') . '"';
}

/** {...$attrs}: routes each entry through the right attribute function. *
 * @param array<string|int, mixed> $m
 */
function attrs(array $m): string
{
    $o = '';
    foreach ($m as $k => $v) {
        $k = (string) $k;
        if (!\preg_match('/^[a-zA-Z_:][\w:.\-]*$/', $k) || \str_starts_with(\strtolower($k), 'on')) {
            throw new \InvalidArgumentException("Unsafe attribute name \"$k\"");
        }
        $lk = \strtolower($k);
        $o .= match (true) {
            \in_array($lk, ['href', 'src', 'action', 'formaction', 'poster', 'cite', 'data'], true) => u($k, $v),
            $lk === 'class' => cls($v),
            $lk === 'style' => sty($v),
            default => a($k, $v),
        };
    }
    return $o;
}
