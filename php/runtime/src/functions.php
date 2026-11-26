<?php

declare(strict_types=1);

namespace Lintl;

final class Html implements \Stringable
{
    public function __construct(public readonly string $s) {}

    public function __toString(): string
    {
        return $this->s;
    }

    /** Greppable escape hatch. */
    public static function unsafeRaw(string $html): self
    {
        return new self($html);
    }
}

// Author-facing signatures are written unqualified (`function Badge(): Html`,
// doc §6.9 / the `lintl init` demo), and compiled output is global-namespace
// PHP: alias the global name so those type hints resolve to the runtime class.
if (! \class_exists('Html', false)) {
    \class_alias(Html::class, 'Html');
}

/**
 * The §10 `<Suspense>` boundary value (architecture doc §10).
 *
 * A streaming component yields a Suspense instance where the boundary sits
 * (`yield new \Lintl\Suspense(fallback: …, work: …)`): the flush runtime
 * (`Lintl\stream()`) sinks the fallback in place immediately and appends
 * the boundary's resolved content out-of-order inside a `<template>` plus a
 * swap script. Consumed outside the flush runtime (through `c()`, i.e. the
 * sync target) the boundary runs inline: no fallback, the resolved value
 * renders whole — §10's "a page works everywhere" compat rule.
 *
 * `fallback` is a child VALUE (data — it renders through the escapers).
 * `work` is the async step: a closure returning the resolved child value
 * (data) or a generator of markup chunks (a streamed subtree). Inside the
 * flush runtime the closure runs in a fiber, so it may suspend while its
 * async work is pending.
 */
final class Suspense
{
    /** @param \Closure(): mixed $work the async step (see class docblock) */
    public function __construct(
        public readonly mixed $fallback,
        public readonly \Closure $work,
    ) {}
}

const FLAGS = ENT_QUOTES | ENT_SUBSTITUTE | ENT_HTML5;

function e(string $s): string
{
    return \htmlspecialchars($s, FLAGS, 'UTF-8');
}

/** Render any child value to an HTML string. */
function c(mixed $v): string
{
    if ($v instanceof Html) {
        return $v->s;
    }
    // §10 compat (sync target): a boundary runs inline — no fallback, the
    // resolved value renders whole through the same escaper.
    if ($v instanceof Suspense) {
        return c(($v->work)());
    }
    if ($v instanceof \Traversable) {
        // Stream target (§10): a Traversable child is a markup stream — its
        // chunks are compiler-produced markup (literals) or data already
        // escaped when the chunk was built, so they pass through raw. Plain
        // arrays stay data and take the is_iterable branch below.
        $o = '';
        foreach ($v as $x) {
            if ($x instanceof Suspense) {
                $o .= c(($x->work)());
            } elseif ($x instanceof \Traversable) {
                $o .= c($x);
            } elseif ($x instanceof Html) {
                $o .= $x->s;
            } elseif (\is_string($x)) {
                $o .= $x;
            } elseif (\is_int($x) || \is_float($x)) {
                $o .= (string) $x;
            } elseif ($x === null || \is_bool($x)) {
                $o .= '';
            } else {
                throw new \InvalidArgumentException('Cannot render '.\get_debug_type($x).' as a markup chunk');
            }
        }

        return $o;
    }
    if (\is_string($v)) {
        return \htmlspecialchars($v, FLAGS, 'UTF-8');
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
            $o .= c($x);
        }

        return $o;
    }
    if ($v instanceof \Stringable) {
        return \htmlspecialchars((string) $v, FLAGS, 'UTF-8');
    }
    throw new \InvalidArgumentException('Cannot render '.\get_debug_type($v).' as a child');
}

/** Generic attribute. */
function a(string $name, mixed $v): string
{
    if ($v === null || $v === false) {
        return '';
    }
    if ($v === true) {
        return ' '.$name;
    }
    if (\is_array($v)) {
        throw new \InvalidArgumentException("Attribute \"$name\" received an array");
    }

    // @phpstan-ignore cast.string (doc §6.8: any scalar attribute value renders — the cast is the contract)
    return ' '.$name.'="'.\htmlspecialchars((string) $v, FLAGS, 'UTF-8').'"'; // @pun-ignore analyse/types/invalid-cast -- doc §6.8: any scalar attribute value renders — the cast is the contract
}

/** URL attribute: scheme allowlist. Percent-encoding is NOT a defense against javascript:. */
function u(string $name, mixed $v): string
{
    if ($v === null || $v === false) {
        return '';
    }
    // @phpstan-ignore cast.string (doc §6.8: stringable URL values render — the cast is the contract)
    $s = (string) $v; // @pun-ignore analyse/types/invalid-cast -- doc §6.8: stringable URL values render — the cast is the contract
    $probe = \preg_replace('/[\x00-\x20\x7f]+/', '', $s) ?? '';   // browsers ignore ws/ctl inside schemes
    if (\preg_match('/^([a-z][a-z0-9+.\-]*):/i', $probe, $m)
        && ! \in_array(\strtolower($m[1]), ['http', 'https', 'mailto', 'tel', 'sms'], true)) {
        $s = 'about:invalid#blocked';
    }

    return ' '.$name.'="'.\htmlspecialchars($s, FLAGS, 'UTF-8').'"';
}

/** srcset attribute: split candidates and scheme-check each URL (§7.1; docs/srcset-alignment.md). */
function srcset(string $name, mixed $v): string
{
    if ($v === null || $v === false) {
        return '';
    }
    // @phpstan-ignore cast.string (docs/srcset-alignment.md: the candidate list arrives as any scalar — the cast is the contract)
    $s = (string) $v; // @pun-ignore analyse/types/invalid-cast -- docs/srcset-alignment.md: the candidate list arrives as any scalar — the cast is the contract
    $out = [];
    // A comma separates candidates only when followed by whitespace or the
    // end of the value — `data:text/html,x 4x` keeps its URL-internal comma.
    foreach (\preg_split('/,(?=\s|$)/', $s) ?: [] as $candidate) {
        $candidate = \trim($candidate);
        if ($candidate === '') {
            continue;
        }
        $tokens = \preg_split('/\s+/', $candidate) ?: [];
        if ($tokens === []) {
            continue;
        }
        // Probe the whole candidate: browsers ignore ws/ctl inside schemes,
        // so "java\tscript:x 2x" is one obfuscated URL, not URL + descriptor.
        $probe = \preg_replace('/[\x00-\x20\x7f]+/', '', $candidate) ?? '';
        if (\preg_match('/^([a-z][a-z0-9+.\-]*):/i', $probe, $m)
            && ! \in_array(\strtolower($m[1]), ['http', 'https', 'mailto', 'tel', 'sms'], true)) {
            // Hostile URL: keep only a trailing width/density descriptor.
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

    return ' '.$name.'="'.\htmlspecialchars(\implode(', ', $out), FLAGS, 'UTF-8').'"';
}

/** class={…}: string | list | ['cls' => bool] */
function cls(mixed $v): string
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
                    $out[] = $k;
                }
            }
        }
    };
    $walk($v);

    return $out === [] ? '' : ' class="'.\htmlspecialchars(\implode(' ', $out), FLAGS, 'UTF-8').'"';
}

/** style={['prop' => 'value']} — values restricted to a safe charset. */
function sty(mixed $v): string
{
    if (! \is_array($v)) {
        return a('style', $v);
    }
    $o = [];
    foreach ($v as $k => $val) {
        if ($val === null || $val === false) {
            continue;
        }
        // @phpstan-ignore cast.string (doc §6.8: style values are scalar strings — the cast is the contract)
        $val = (string) $val; // @pun-ignore analyse/types/invalid-cast -- doc §6.8: style values are scalar strings — the cast is the contract
        if (! \preg_match('/^[\w\s#%.,()\-\/+*]*$/u', $val) || ! \preg_match('/^[a-z\-]+$/i', (string) $k)) {
            throw new \InvalidArgumentException("Unsafe style value for \"$k\"");
        }
        $o[] = $k.':'.$val;
    }

    return $o === [] ? '' : ' style="'.\htmlspecialchars(\implode(';', $o), FLAGS, 'UTF-8').'"';
}

/** {...$attrs}: routes each entry through the right attribute function. *
 * @param  array<string|int, mixed>  $m
 */
function attrs(array $m): string
{
    $o = '';
    foreach ($m as $k => $v) {
        $k = (string) $k;
        if (! \preg_match('/^[a-zA-Z_:][\w:.\-]*$/', $k) || \str_starts_with(\strtolower($k), 'on')) {
            throw new \InvalidArgumentException("Unsafe attribute name \"$k\"");
        }
        $lk = \strtolower($k);
        $o .= match (true) {
            \in_array($lk, ['href', 'src', 'action', 'formaction', 'poster', 'cite', 'data'], true) => u($k, $v),
            $lk === 'class' => cls($v),
            $lk === 'style' => sty($v),
            // §4.4's spread row: "each entry goes through the same attribute
            // rules" — a srcset key must split candidates and scheme-check
            // each URL exactly like a direct srcset={…} attribute (§7.1).
            $lk === 'srcset' => srcset($k, $v),
            default => a($k, $v),
        };
    }

    return $o;
}

/**
 * The §10 flush loop (@internal — `Lintl\stream()` is the public entry).
 *
 * It walks a stream-target render tree exactly the way `c()` walks a
 * Traversable: chunk strings are compiler-produced markup (or data already
 * escaped when the chunk was built) and sink raw, `Html` is the trusted
 * sink, null/bool render empty, scalars stringify, anything else is
 * rejected. A `Suspense` boundary caught during the walk sinks its
 * fallback placeholder in place (`<div id="lt-N">fallback</div>` — the
 * fallback renders through `c()`, escaping by default), and its work runs
 * in a fiber. Once the shell finished, the fibers are pumped round-robin
 * (a suspension = "not ready this round"); each termination sinks the
 * resolved content inside `<template id="lt-N-r">…</template>` followed by
 * the swap script that replaces the placeholder in place — §10's
 * out-of-order protocol: a fast sibling flushes before a slow earlier
 * boundary. Resolved generators walk as markup streams (their chunks flush
 * progressively, nested boundaries join the pump); every other resolved
 * value is data and renders through `c()`, so no streaming path bypasses
 * the escapers. The ≈250-byte swap runtime is emitted once per render,
 * with the first resolved boundary.
 */
final class SuspenseFlusher
{
    /** The one-time in-place swap runtime (§10 step 3). */
    public const SWAP_RUNTIME = '<script>function lintlSwap(id){var t=document.getElementById(id+"-r"),p=document.getElementById(id);if(!t||!p)return;p.replaceChildren(t.content.cloneNode(true));t.remove();}</script>';

    /**
     * Process-global count of boundaries whose fallback placeholder was
     * flushed (the §11 profiler panels' "suspense boundaries flushed"
     * metric — snapshot it around a render window, or zero it directly
     * to bracket one; the flusher itself never resets it).
     */
    public static int $boundariesFlushed = 0;

    private string $buffer = '';

    private int $nextId = 1;

    private bool $runtimeEmitted = false;

    /** @var list<array{id: string, fiber: \Fiber<mixed, mixed, mixed, mixed>}> the not-yet-resolved boundaries */
    private array $pending = [];

    /** @var callable(string): void */
    private $sink;

    /** @param callable(string): void|null $flush the progressive sink */
    public function __construct(?callable $flush)
    {
        $this->sink = $flush ?? function (string $chunk): void {
            $this->buffer .= $chunk;
        };
    }

    /** Walk the tree, pump the pending works, return the captured bytes. */
    public function run(mixed $tree): string
    {
        $this->walk($tree);
        $this->pump();

        return $this->buffer;
    }

    private function emit(string $chunk): void
    {
        ($this->sink)($chunk);
    }

    /** The chunk walk — markup context, mirroring c()'s Traversable branch. */
    private function walk(mixed $node): void
    {
        if ($node instanceof \Traversable) {
            foreach ($node as $chunk) {
                $this->walk($chunk);
            }

            return;
        }
        if ($node instanceof Suspense) {
            $this->boundary($node);

            return;
        }
        if ($node instanceof Html) {
            $this->emit($node->s);

            return;
        }
        if (\is_string($node)) {
            $this->emit($node);

            return;
        }
        if ($node === null || \is_bool($node)) {
            return;
        }
        if (\is_int($node) || \is_float($node)) {
            $this->emit((string) $node);

            return;
        }
        if (\is_iterable($node)) {
            foreach ($node as $chunk) {
                $this->walk($chunk);
            }

            return;
        }
        throw new \InvalidArgumentException('Cannot render '.\get_debug_type($node).' as a stream chunk');
    }

    /** §10 step 1–2: flush the fallback placeholder now, register the work. */
    private function boundary(Suspense $b): void
    {
        $id = 'lt-'.$this->nextId;
        $this->nextId++;
        $this->emit('<div id="'.$id.'">');
        $this->emit(c($b->fallback));
        $this->emit('</div>');
        $this->pending[] = ['id' => $id, 'fiber' => new \Fiber($b->work)];
        self::$boundariesFlushed++;
    }

    /** Pump the pending works round-robin; each resolution flushes at once.
     * A boundary discovered while walking a resolved subtree joins the same
     * queue (resolve() may append to it), so nested boundaries pump too. */
    private function pump(): void
    {
        while ($this->pending !== []) {
            $entry = \array_shift($this->pending);
            $fiber = $entry['fiber'];
            if ($fiber->isStarted()) {
                $fiber->resume();
            } else {
                $fiber->start();
            }
            if ($fiber->isTerminated()) {
                $this->resolve($entry['id'], $fiber->getReturn());
            } else {
                $this->pending[] = $entry;
            }
        }
    }

    /** §10 step 3: the resolved boundary flushes template + swap script. */
    private function resolve(string $id, mixed $resolved): void
    {
        if (! $this->runtimeEmitted) {
            $this->emit(self::SWAP_RUNTIME);
            $this->runtimeEmitted = true;
        }
        $this->emit('<template id="'.$id.'-r">');
        if ($resolved instanceof \Generator || $resolved instanceof \Iterator) {
            $this->walk($resolved);
        } else {
            $this->emit(c($resolved));
        }
        $this->emit('</template>');
        $this->emit('<script>lintlSwap("'.$id.'")</script>');
    }
}

/**
 * §10 runtime: render a stream-target render tree (a generator yielding
 * chunks — `docs/stream-alignment.md`) progressively. Each chunk sinks
 * through `$flush` as it arrives; without a sink the bytes are captured and
 * returned (chunk order is still the flush order, which is what the tests
 * pin). Suspense boundaries flush their placeholder in place and their
 * resolved content out-of-order (§10's protocol, `SuspenseFlusher`).
 *
 * @param  callable(string): void|null  $flush  the progressive sink
 */
function stream(mixed $tree, ?callable $flush = null): string
{
    return (new SuspenseFlusher($flush))->run($tree);
}

/**
 * §10 runtimes: a flush transport owns how a chunk reaches the client.
 * FPM (flush + `X-Accel-Buffering: no`) and the CLI/CLI-server lane ship
 * here; FrankenPHP streams through the same `echo + flush` shape.
 * RoadRunner/Swoole/OpenSwoole and the ReactPHP/AMPHP event loops get
 * their own adapters on this seam where those runtimes run (CI).
 */
interface FlushTransport
{
    /** Once before the first chunk: headers and buffering-off belong here. */
    public function begin(): void;

    /** Sink one chunk; it must reach the client before write() returns. */
    public function write(string $chunk): void;

    /** Once after the last chunk. */
    public function end(): void;
}

/** The CLI / CLI-server lane: plain `echo` + flush per chunk. */
final class CliFlushTransport implements FlushTransport
{
    public function begin(): void {}

    public function write(string $chunk): void
    {
        echo $chunk;
        \flush();
    }

    public function end(): void
    {
        \flush();
    }
}

/** The FPM lane (§10): `X-Accel-Buffering: no`, buffered-output teardown, flush per chunk. */
final class FpmFlushTransport implements FlushTransport
{
    public function begin(): void
    {
        \header('Content-Type: text/html; charset=utf-8');
        // §10's FPM runtime: flush + `X-Accel-Buffering: no` (nginx off).
        \header('X-Accel-Buffering: no');
        while (\ob_get_level() > 0) {
            \ob_end_flush();
        }
    }

    public function write(string $chunk): void
    {
        echo $chunk;
        \flush();
    }

    public function end(): void
    {
        \flush();
    }
}

/**
 * §10: render a stream-target tree through a [`FlushTransport`] —
 * `begin()` fires before the first chunk, every chunk reaches `write()`
 * as it is produced (suspense protocol included), `end()` closes. Returns
 * `''`: the bytes went to the transport, not to a buffer.
 */
function stream_via(mixed $tree, FlushTransport $transport): string
{
    $transport->begin();
    (new SuspenseFlusher(static function (string $chunk) use ($transport): void {
        $transport->write($chunk);
    }))->run($tree);
    $transport->end();

    return '';
}

// The §10 server adapters (RoadRunner/Swoole/FrankenPHP/ReactPHP) — loaded
// with the runtime for every lane (composer files autoload AND direct
// requires like scripts/suspense-demo.php).
require_once __DIR__.'/transports.php';
