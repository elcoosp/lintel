# Benchmarks

Lintel's numbers come from one reproducible harness (`bench/`, the §9.2
specification): five scenarios, the competitor engines (hand-written `echo`
as the floor, Blade, Twig, Plates, the `--target=rfc` lowering), an
OPcache/JIT settings matrix, 30 timed renders per point, and committed raw
JSON + summary + charts. **No number appears anywhere in launch material
unless `make bench` reproduces it and the methodology links the raw JSON**
— that publishing rule is the harness's reason to exist.

What the scenarios measure:

| Scenario | Why it exists |
|---|---|
| S1 static-heavy page (95% static) | literal folding |
| S2 1,000-row table with escaping | `c()`/`a()` helper overhead |
| S3 component tree, depth 10 × fan-out 3 | `Html` allocation |
| S4 attribute-heavy links | helper cost per attribute |
| S5 streaming TTFB, 3 slow boundaries | the `stream` target: time-to-first-chunk vs the blocking sync render |

The methodology and the committed summary are embedded below, verbatim,
from the latest committed run. Where a metric is not measurable on a given
runtime (the opcode-count probe on a stripped static CLI build), the JSON
records `null` and the CI bench job re-probes on the pinned image — the
harness records an honest hole instead of inventing a number.
