# The daemon protocol (`lintel watch --socket`)

The `lintel watch` daemon keeps one resident compiler process behind a
socket so the dev loader's per-request compile skips the process spawn
(T-12's measured p50/p99 vs spawn). Since the Phase 10 hardening pass, the
socket speaks one **length-prefixed JSON protocol** on both directions —
the §6.7 sketch `{"path":…} → {"php":…}|{"diag":[…]}` made real — and the
daemon's recompiles run through the same content-addressed build cache the
`lintel build` CLI uses.

## Transport

- **Unix:** a Unix domain socket at the path given with `--socket` (the
  background default is a per-workspace hash-named path under the system
  temp dir). A stale socket file from a killed daemon is removed on bind.
- **Windows fallback:** a loopback TCP listener; the ephemeral port is
  written to `<socket-path>.port` (the same protocol, one transport
  difference). The Windows lane compiles everywhere; its socket tests are
  accepted DEFERRED-TO-CI/MAINTAINER (no Windows runner in the dev
  sandbox) — the T-12 card names this exact limit.

## Framing

Every message — request **and** response — is framed the same way:

```text
<10 ASCII decimal digits>\n<payload bytes>
```

The ten-byte prefix is the payload's byte length, zero-padded to a fixed
width so the reader can `read_exact` the prefix and then the body without
any delimiter inside the payload. The payload is exactly that many bytes
of UTF-8 JSON. A daemon caps the length it accepts at 64 MiB: a larger or
non-decimal prefix is a malformed frame (below).

Example request and answer, as bytes:

```text
0000000027\n{"path":"/app/src/Page.lintel.php"}
0000000063\n{"ok":true,"php":"<?php\nreturn (static function ..."}
```

## Requests

- **Compile a file** — `{"path": "<absolute path>"}`:

  ```json
  {"path": "/app/src/Page.lintel.php"}
  ```

- **Stop the daemon** — `{"quit": true}`. The daemon answers (below) and
  exits; the socket file is removed.

Unknown keys are ignored; unknown *shapes* (no `path`, no `quit`, invalid
JSON) are a request error (below), not a crash.

## Responses

- **Compiled** — `{"ok": true, "php": "<compiled php>"}`.
- **Compile or stat failure** — `{"ok": false, "diag": ["<rendered
  diagnostic>", …]}`. The strings are the same rendered text the CLI's
  stderr carries (one entry per diagnostic; a stat/read failure is a
  single plain-text entry). A failed *compile* is an answer, not a
  transport failure — the loader surfaces it the way the `proc_open`
  fallback surfaces a non-zero exit.
- **Request error** — `{"ok": false, "diag": ["lintel watch: <problem>"]}`
  for a malformed shape; the connection stays usable.
- **Quit** — `{"ok": true, "quit": true}` before the daemon stops.

## Error handling

| Failure | Daemon behavior | Client behavior |
|---|---|---|
| Malformed frame (short prefix, non-decimal digits, length over the 64 MiB cap) | closes the connection, keeps serving | treats it as a transport failure |
| Invalid JSON / unknown request shape | answers the request-error response | surfaces the diag text |
| Client vanished mid-request | write failure ignored, keeps serving | — |
| Connection closed before a full response | — | transport failure → reconnect, then fall back |

One request per connection: the client connects, sends exactly one framed
request, reads exactly one framed answer, closes. A readiness probe opens
the connection and closes it without sending anything — the daemon treats
the empty read as a probe and serves on. (A quit frame here would stop
the daemon the moment it came up, which is the Phase 2 background-spawn
bug this protocol pass fixed: the probe used to send `quit`.)

## Reconnection logic

`lintel_cli::watch::request` — the client the Rust integration tests and
the PHP loader's socket lane mirror — retries a *connect* failure up to
three times with a short backoff (0 ms, 50 ms, 250 ms), which covers a
daemon restart window; once connected, a protocol-level failure is final
for that request. The PHP loader
(`php/lintel/src/Dev/Loader.php`) retries the connect twice (100 ms
apart) and then falls back to the `proc_open` lane — the socket is an
optimization, never a staleness or availability source.

## The daemon's build cache

The daemon serves from an in-memory mtime+size map; a miss reads the
source and consults the workspace's content-addressed cache at
`<workspace>/.lintel-cache/` — the same engine `lintel build` uses, keyed
by a SHA-256 of the source content with the compile target, so a
watch-triggered rebuild of an unchanged content never recompiles and a
reverted edit serves instantly. The daemon's counters accumulate in
`.lintel-cache/stats.json` (`compiled`, `cache_hits`) and are recorded
after every disk-cache consult; both `stats.json` and the LSP index's
`.lintel-cache/lsp-index.json` are written under an exclusive `flock(2)`
so concurrent writers (a watch daemon and a `lintel build`, two LSP
sessions) cannot interleave a partial write. Watcher events under
`.lintel-cache/` never invalidate entries — the cache's own writes must
not churn the in-memory map.
