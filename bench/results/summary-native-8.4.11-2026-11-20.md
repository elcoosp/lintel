# Bench results (native) — PHP 8.4.11, 2026-11-20

Hardware/OS: `Linux 5.10.134-013.15.kangaroo.al8.x86_64 x86_64`, host `c-6aff8d49-140d72b5-7a3143c35d5a`; lintel `lintel 0.1.0`; 30 runs per point, 3 warm-up renders; S5 boundaries sleep 5 ms each. Raw JSON: `results-native-8.4.11-2026-11-20.json`.

> Publishing rule (§9.2): no number appears in README/launch posts unless `make bench` reproduces it and the methodology page links this raw JSON.

### S1 static-heavy page — ns/render

| ns/render (median) | opcache-off | opcache-on | opcache-jit |
| --- | --- | --- | --- |
| lintel | 914 ns | 922 ns | 923 ns |
| rfc | 18.25 µs | 18.19 µs | 18.16 µs |
| echo | 511 ns | 512 ns | 512 ns |
| blade | 13.84 µs | 13.90 µs | 14.02 µs |
| twig | 5.16 µs | 5.17 µs | 5.13 µs |
| plates | 12.19 µs | 12.25 µs | 12.28 µs |

### S2 1,000-row table — ns/render

| ns/render (median) | opcache-off | opcache-on | opcache-jit |
| --- | --- | --- | --- |
| lintel | 1.27 ms | 1.26 ms | 1.26 ms |
| rfc | 2.79 ms | 2.78 ms | 2.80 ms |
| echo | 774.84 µs | 777.52 µs | 779.64 µs |
| blade | 1.58 ms | 1.58 ms | 1.56 ms |
| twig | 1.83 ms | 1.83 ms | 1.82 ms |
| plates | 932.86 µs | 933.59 µs | 933.25 µs |

### S3 component tree (depth 10 × fan-out 3) — ns/render

| ns/render (median) | opcache-off | opcache-on | opcache-jit |
| --- | --- | --- | --- |
| lintel | 31.29 ms | 31.28 ms | 31.61 ms |
| rfc | 78.99 ms | 80.04 ms | 81.19 ms |
| echo | 7.66 ms | 7.91 ms | 7.87 ms |
| blade | 1630.01 ms | 1626.07 ms | 1651.69 ms |
| twig | 152.56 ms | 153.95 ms | 307.96 ms |
| plates | 997.16 ms | 1000.84 ms | 2200.60 ms |

### S4 attribute-heavy links — ns/render

| ns/render (median) | opcache-off | opcache-on | opcache-jit |
| --- | --- | --- | --- |
| lintel | 673.73 µs | 702.65 µs | 742.87 µs |
| rfc | 491.75 µs | 498.21 µs | 492.18 µs |
| echo | 181.36 µs | 183.46 µs | 183.61 µs |
| blade | 372.67 µs | 433.77 µs | 386.52 µs |
| twig | 611.67 µs | 666.03 µs | 613.18 µs |
| plates | 215.81 µs | 218.99 µs | 221.89 µs |

### S5 streaming TTFB (time to first chunk)

| TTFB (median) | opcache-off | opcache-on | opcache-jit |
| --- | --- | --- | --- |
| lintel-sync | 15.19 ms | 15.21 ms | 15.25 ms |
| lintel-stream | 1.86 µs | 2.22 µs | 2.76 µs |
| echo-stream | 4.60 µs | 4.94 µs | 5.81 µs |

### S5 total render time

| total (median) | opcache-off | opcache-on | opcache-jit |
| --- | --- | --- | --- |
| lintel-sync | 15.19 ms | 15.21 ms | 15.25 ms |
| lintel-stream | 15.20 ms | 15.21 ms | 15.28 ms |
| echo-stream | 15.20 ms | 15.21 ms | 15.24 ms |

### Compile throughput (lintel build, 100 sources)

| metric | value |
| --- | --- |
| cold files/s | 9728 |
| warm files/s | 7400 |

### Opcode counts (opcache.opt_debug_level=0x20000)

- S1: not measurable on this runtime (instrumentation unavailable) — see bench/README.md
- S2: not measurable on this runtime (instrumentation unavailable) — see bench/README.md
- S3: not measurable on this runtime (instrumentation unavailable) — see bench/README.md
- S4: not measurable on this runtime (instrumentation unavailable) — see bench/README.md
- S5: not measurable on this runtime (instrumentation unavailable) — see bench/README.md

The runtime in use does not expose the opcode dump; the metric stays null here and is attempted again by the CI bench job on the pinned image.
