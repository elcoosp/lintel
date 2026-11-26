#!/usr/bin/env python3
"""M1 release-readiness: expand the §7.2 threat corpus from ~31 to 200+ REAL
published XSS payloads, attributed to OWASP and PortSwigger, each executed
under real PHP against the compiled artifact and asserted inert.

Sources (both public, attribution preserved in the emitted corpus file):
  - OWASP Cheat Sheet Series, "XSS Filter Evasion" (CC BY-SA 4.0)
    https://cheatsheetseries.owasp.org/cheatsheets/XSS_Filter_Evasion_Cheat_Sheet.html
  - PortSwigger Web Security Academy, "XSS cheat sheet"
    https://portswigger.net/web-security/cross-site-scripting/cheat-sheet

Each payload rides a dynamic child (the `c()` lane) AND, when it is
URL/scheme-shaped, a dynamic URL attribute (the `u()` lane). The corpus
is emitted as a PHP data-provider class consumed by the new
ExpandedThreatCorpusTest; that test compiles each `.lintl.php` source
with the real `lintl` binary, executes the compiled PHP under real PHP,
and asserts the rendered output carries NO live attack surface:
  - no raw `<script>` tag from a dynamic value
  - no `javascript:`/`vbscript:`/`data:text/html` scheme in a rendered URL
  - no unescaped `on\w+=` event-handler attribute from a dynamic value
  - no `</` close-tag breakout from a dynamic child

If a payload's source fails to COMPILE (a validator rejection — e.g. an
`onclick={...}` spread is LNL0301), the test asserts the rejection code
rather than the rendered output: a compile-time refusal is the strongest
inertness. The corpus counts BOTH classes (compiled-inert + rejected).

Usage:
    python3 scripts/gen-expanded-threat-corpus.py
Emits: php/runtime/tests/ExpandedThreatCorpus.php (the data provider)
"""
from __future__ import annotations

import html
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
OWASP_FILE = Path("/tmp/owasp_payloads.txt")
PORTSWIGGER_FILE = Path("/tmp/portswigger_payloads.txt")
OUT = REPO_ROOT / "php/runtime/tests/ExpandedThreatCorpus.php"


def load_owasp() -> list[str]:
    payloads: list[str] = []
    seen: set[str] = set()
    for line in OWASP_FILE.read_text(encoding="utf-8").splitlines():
        p = line.rstrip("\n")
        if p and p not in seen:
            seen.add(p)
            payloads.append(p)
    return payloads


def load_portswigger() -> list[str]:
    payloads: list[str] = []
    seen: set[str] = set()
    for line in PORTSWIGGER_FILE.read_text(encoding="utf-8").splitlines():
        p = line.rstrip("\n")
        if p and p not in seen:
            seen.add(p)
            payloads.append(p)
    return payloads


# A payload is "URL/scheme-shaped" if it could ride a URL attribute and
# meaningfully test the scheme allowlist. We pick the ones containing a
# scheme token or a `href`/`src` shape; the rest ride the child lane only.
URL_SHAPE = re.compile(
    r"javascript:|vbscript:|data:text/html|data:|^\s*[a-z]+script:|href|src=|HREF|SRC",
    re.IGNORECASE,
)


def php_string_literal(s: str) -> str:
    """Emit a PHP single-quoted string literal, escaping only `'` and `\\`."""
    return "'" + s.replace("\\", "\\\\").replace("'", "\\'") + "'"


def main() -> int:
    owasp = load_owasp()
    portswigger = load_portswigger()
    print(f"owasp payloads loaded: {len(owasp)}", file=sys.stderr)
    print(f"portswigger payloads loaded: {len(portswigger)}", file=sys.stderr)

    # Deduplicate across both sources, keeping attribution.
    seen: set[str] = set()
    vectors: list[tuple[str, str, str]] = []  # (name, source, attribution)
    for p in owasp:
        if p not in seen and 0 < len(p) < 250:
            seen.add(p)
            vectors.append((p, "OWASP"))
    for p in portswigger:
        if p not in seen and 0 < len(p) < 250:
            seen.add(p)
            vectors.append((p, "PortSwigger"))

    print(f"distinct vectors after dedup: {len(vectors)}", file=sys.stderr)

    # We want 200+. If dedup left us short, that's a real signal — but both
    # lists together comfortably exceed 200.
    if len(vectors) < 200:
        print(
            f"WARNING: only {len(vectors)} distinct vectors — below the 200 target",
            file=sys.stderr,
        )

    # Build the PHP data-provider file. Each vector becomes a named case
    # with: the raw payload, the attribution, and the two source shapes
    # (child + optional URL). The test compiles + executes + asserts inert.
    lines: list[str] = []
    lines.append("<?php")
    lines.append("")
    lines.append("declare(strict_types=1);")
    lines.append("")
    lines.append("// M1 release-readiness: the expanded §7.2 threat corpus.")
    lines.append("//")
    lines.append("// Sources (public, attribution preserved per-vector):")
    lines.append("//   - OWASP Cheat Sheet Series, \"XSS Filter Evasion\" (CC BY-SA 4.0)")
    lines.append("//     https://cheatsheetseries.owasp.org/cheatsheets/XSS_Filter_Evasion_Cheat_Sheet.html")
    lines.append("//   - PortSwigger Web Security Academy, \"XSS cheat sheet\"")
    lines.append("//     https://portswigger.net/web-security/cross-site-scripting/cheat-sheet")
    lines.append("//")
    lines.append("// Every payload rides a dynamic child (the c() lane) and, when")
    lines.append("// URL/scheme-shaped, a dynamic URL attribute (the u() lane). The")
    lines.append("// companion PHPUnit test (ExpandedThreatCorpusTest) compiles each")
    lines.append("// source with the real lintl binary, executes the compiled PHP under")
    lines.append("// real PHP, and asserts the rendered output carries NO live attack")
    lines.append("// surface. A compile-time validator rejection (LNL0301 etc.) is the")
    lines.append("// strongest inertness and is counted as a passing vector.")
    lines.append("")
    lines.append("namespace Lintl\\Tests;")
    lines.append("")
    lines.append("final class ExpandedThreatCorpus")
    lines.append("{")
    lines.append("    /** @return array<string, array{payload: string, source: string, attribution: string}> */")
    lines.append("    public static function vectors(): array")
    lines.append("    {")
    lines.append("        return [")

    for idx, (payload, attribution) in enumerate(vectors):
        # Stable, readable name: attribution + index.
        name = f"{attribution.lower()}-{idx:03d}"
        # Determine the source shape. URL-shaped payloads ride BOTH lanes.
        url_shaped = bool(URL_SHAPE.search(payload))
        lines.append(f"            {php_string_literal(name)} => [")
        lines.append(f"                'payload' => {php_string_literal(payload)},")
        lines.append(f"                'attribution' => {php_string_literal(attribution)},")
        lines.append(f"                'url_shaped' => {'true' if url_shaped else 'false'},")
        lines.append("            ],")

    lines.append("        ];")
    lines.append("    }")
    lines.append("}")
    lines.append("")

    OUT.write_text("\n".join(lines), encoding="utf-8")
    print(
        f"emitted {OUT} — {len(vectors)} vectors "
        f"({sum(1 for _, a in vectors if a == 'OWASP')} OWASP, "
        f"{sum(1 for _, a in vectors if a == 'PortSwigger')} PortSwigger)",
        file=sys.stderr,
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
