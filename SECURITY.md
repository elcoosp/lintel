# Security Policy

Lintl's core promise is XSS-safety by default: every dynamic value is
escaped for its HTML context, URL attributes pass a scheme allowlist, and
the only raw-HTML sink is the greppable `Html::unsafeRaw()` (architecture
doc §7). If you find a way around that promise, we want to hear about it.

## Supported versions

| Version | Supported |
| --- | --- |
| 0.1.x | ✅ |

## Reporting a vulnerability

**Do not open a public GitHub issue for a security report.**

1. Use GitHub's *Report a vulnerability* flow on
   <https://github.com/elcoosp/lintl/security/advisories> (private).
2. Include: the affected version/commit, a minimal `.lintl.php` input that
   demonstrates the issue, the compiled output, and the rendered HTML —
   a differential-style repro makes triage fast.
3. You will get an acknowledgement within **7 days** and a first assessment
   within **14 days**. If the report is confirmed, a fix and advisory are
   released together, and you are credited (unless you prefer otherwise).

## What counts as a security issue

- Any dynamic value that lands in the output unescaped outside
  `Html::unsafeRaw()`.
- A way to smuggle a `javascript:`/`data:` URL past the `u()`/`srcset()`
  allowlists (including whitespace/control-byte tricks — those are already
  probed; we want new ones).
- Attribute-name injection through spread keys (e.g. an `on*` key or a
  broken-out `" onmouseover=` payload reaching the output).
- A compiler panic or hang reachable from untrusted input (the fuzz target
  treats both as bugs).
- Non-JSX PHP that does not pass through byte-identically.

## What is *not* an issue

- Static attribute values in the author's own source (e.g. a hand-written
  `href="javascript:…"`): those are code, not data — they fold verbatim per
  §4.4's static row. Lint rules may warn, but the compiler trusts the
  author's literals.
- Escaped-looking output that a browser renders inertly (e.g. `&`
  re-encoding of entity strings).
- `Html::unsafeRaw()` misuse — the sink is documented and deliberately
  greppable; what you feed it is your responsibility.
