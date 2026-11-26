# Bench results (native) — PHP 8.4.11, 2026-11-20

Hardware/OS: `Linux 5.10.134-013.15.kangaroo.al8.x86_64 x86_64`, host `c-6b005be3-140d72b5-7bc55e3733be`; lintl `lintl 0.1.0`; 30 runs per point, 3 warm-up renders; S5 boundaries sleep 5 ms each. Raw JSON: `results-native-8.4.11-2026-11-20.json`.

> Publishing rule (§9.2): no number appears in README/launch posts unless `make bench` reproduces it and the methodology page links this raw JSON.

### S1 static-heavy page — ns/render

| ns/render (median) | opcache-off | opcache-on | opcache-jit |
| --- | --- | --- | --- |
| lintl | 915 ns | 918 ns | 918 ns |
| rfc | 18.08 µs | 18.13 µs | 18.13 µs |
| echo | 520 ns | 516 ns | 509 ns |
| blade | 13.93 µs | 13.75 µs | 13.79 µs |
| twig | 5.16 µs | 5.10 µs | 5.12 µs |
| plates | 12.34 µs | 15.31 µs | 12.26 µs |

### S2 1,000-row table — ns/render

| ns/render (median) | opcache-off | opcache-on | opcache-jit |
| --- | --- | --- | --- |
| lintl | 1.29 ms | 1.26 ms | 1.27 ms |
| rfc | 2.85 ms | 2.80 ms | 2.82 ms |
| echo | 786.03 µs | 777.86 µs | 775.34 µs |
| blade | 3.71 ms | 1.57 ms | 1.59 ms |
| twig | 1.88 ms | 1.83 ms | 1.84 ms |
| plates | 982.39 µs | 932.62 µs | 954.90 µs |

### S3 component tree (depth 10 × fan-out 3) — ns/render

| ns/render (median) | opcache-off | opcache-on | opcache-jit |
| --- | --- | --- | --- |
| lintl | 56.12 ms | 31.28 ms | 31.81 ms |
| rfc | 151.71 ms | 79.73 ms | 82.74 ms |
| echo | 10.77 ms | 7.70 ms | 7.77 ms |
| blade | 1649.48 ms | 1636.33 ms | 1645.10 ms |
| twig | 153.10 ms | 152.51 ms | 151.88 ms |
| plates | 992.28 ms | 994.35 ms | 997.75 ms |

### S4 attribute-heavy links — ns/render

| ns/render (median) | opcache-off | opcache-on | opcache-jit |
| --- | --- | --- | --- |
| lintl | 671.15 µs | 671.84 µs | 680.62 µs |
| rfc | 490.20 µs | 491.48 µs | 490.44 µs |
| echo | 179.57 µs | 181.47 µs | 181.26 µs |
| blade | 369.48 µs | 371.81 µs | 374.81 µs |
| twig | 613.53 µs | 612.73 µs | 612.45 µs |
| plates | 215.56 µs | 215.57 µs | 215.64 µs |

### S5 streaming TTFB (time to first chunk)

| TTFB (median) | opcache-off | opcache-on | opcache-jit |
| --- | --- | --- | --- |
| lintl-sync | 15.20 ms | 15.29 ms | 15.20 ms |
| lintl-stream | 3.06 µs | 3.56 µs | 3.08 µs |
| echo-stream | 4.72 µs | 5.51 µs | 4.96 µs |

### S5 total render time

| total (median) | opcache-off | opcache-on | opcache-jit |
| --- | --- | --- | --- |
| lintl-sync | 15.20 ms | 15.29 ms | 15.20 ms |
| lintl-stream | 15.22 ms | 15.27 ms | 15.22 ms |
| echo-stream | 15.20 ms | 15.21 ms | 15.20 ms |

### Compile throughput (lintl build, 100 sources)

| metric | value |
| --- | --- |
| cold files/s | 23054 |
| warm files/s | 24040 |

### Opcode counts (opcache.opt_debug_level=0x20000)

- S1: not measurable on this runtime (instrumentation unavailable) — see bench/README.md
- S2: not measurable on this runtime (instrumentation unavailable) — see bench/README.md
- S3: not measurable on this runtime (instrumentation unavailable) — see bench/README.md
- S4: not measurable on this runtime (instrumentation unavailable) — see bench/README.md
- S5: not measurable on this runtime (instrumentation unavailable) — see bench/README.md

The runtime in use does not expose the opcode dump; the metric stays null here and is attempted again by the CI bench job on the pinned image.
