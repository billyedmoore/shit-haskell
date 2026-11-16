# Evaluated scrutinees skip hsc_eval; PAP and over-application in asm: native runtime

[Raw samples, commands and metadata](runtime-eval-apply-paths.json).

[Hosted run](https://github.com/billyedmoore/shit-haskell/actions/runs/43891530934); AMD EPYC 9V74 80-Core Processor, CPU 0, Rust 1.92.0 release, GHC 9.12.2.
Before `13c0c02c289c587ed014e876c5fc669fee9b14e4`, after `61b2ddd61ced2ce98cc809115c4e27783230160d`.

Native runtime; median of 3 alternating samples, 1 untimed correctness warmups per build.
Builds excluded; process startup included. Every stdout/stderr/exit result agrees.
CPU affinity: 0. GHC uses default RTS settings.

| Program | Before ms | After ms | GHC -O0 ms | GHC -O1 ms | 0.9×O1 ms |
|---|---:|---:|---:|---:|---:|
| fib | 19.172 | 18.945 | 23.505 | 4.953 | 4.458 |
| integer | 34.942 | 38.284 | 10.180 | 8.254 | 7.428 |
| ioref | 286.099 | 279.025 | 76.001 | 9.142 | 8.228 |
| mapops | 152.542 | 157.786 | 93.927 | 59.411 | 53.470 |
| mvar | 213.832 | 213.947 | 20.764 | 11.682 | 10.514 |
| nqueens | 90.455 | 91.983 | 29.142 | 6.578 | 5.920 |
| strings | 1262.559 | 1307.456 | 107.315 | 141.598 | 127.438 |
| sumloop | 270.211 | 283.391 | 255.856 | 17.633 | 15.870 |

```mermaid
xychart-beta
    title "fib: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.025855
    bar [0.019172, 0.018945, 0.023505, 0.004953, 0.004458]
```

```mermaid
xychart-beta
    title "integer: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.042113
    bar [0.034942, 0.038284, 0.010180, 0.008254, 0.007428]
```

```mermaid
xychart-beta
    title "ioref: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.314709
    bar [0.286099, 0.279025, 0.076001, 0.009142, 0.008228]
```

```mermaid
xychart-beta
    title "mapops: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.173564
    bar [0.152542, 0.157786, 0.093927, 0.059411, 0.053470]
```

```mermaid
xychart-beta
    title "mvar: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.235342
    bar [0.213832, 0.213947, 0.020764, 0.011682, 0.010514]
```

```mermaid
xychart-beta
    title "nqueens: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.101181
    bar [0.090455, 0.091983, 0.029142, 0.006578, 0.005920]
```

```mermaid
xychart-beta
    title "strings: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 1.438202
    bar [1.262559, 1.307456, 0.107315, 0.141598, 0.127438]
```

```mermaid
xychart-beta
    title "sumloop: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.311730
    bar [0.270211, 0.283391, 0.255856, 0.017633, 0.015870]
```
