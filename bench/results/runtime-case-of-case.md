# Case-of-case into several results; local calls by value: native runtime

[Raw samples, commands and metadata](runtime-case-of-case.json).

[Hosted run](https://github.com/billyedmoore/shit-haskell/actions/runs/44028297830); AMD EPYC 7763, Rust 1.92.0 release, GHC 9.12.2.
Before `c33d82f3496fd7532d44afebe7d69e8237424b1b`, after `625f8ec4ef94880c6a3da894e67d9784e8d4e4f5`.
This runner is slower than #59's (every program, GHC's too, takes 1.5–2× as long), so compare
before with after, not with earlier runs. No benchmark here has a fused local loop (the change's
target: `sum (map (\x -> x * x) [1 .. 10^6])` takes 483 → 138 M instructions); the
geometric mean of after/before is 0.99, every program within noise.

Native runtime; median of 3 alternating samples, 1 untimed correctness warmups per build.
Builds excluded; process startup included. Every stdout/stderr/exit result agrees.
CPU affinity: 0. GHC uses default RTS settings.

| Program | Before ms | After ms | GHC -O0 ms | GHC -O1 ms | 0.9×O1 ms |
|---|---:|---:|---:|---:|---:|
| fib | 8.786 | 8.936 | 30.646 | 4.301 | 3.871 |
| integer | 23.656 | 22.809 | 7.900 | 7.038 | 6.334 |
| ioref | 142.303 | 134.878 | 81.784 | 7.456 | 6.710 |
| mapops | 96.498 | 98.300 | 93.368 | 48.824 | 43.941 |
| mvar | 176.263 | 177.364 | 17.442 | 7.287 | 6.559 |
| nqueens | 85.521 | 87.633 | 35.757 | 5.704 | 5.134 |
| strings | 564.116 | 551.234 | 110.371 | 134.345 | 120.910 |
| sumloop | 133.211 | 132.849 | 348.284 | 15.102 | 13.592 |

```mermaid
xychart-beta
    title "fib: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.033711
    bar [0.008786, 0.008936, 0.030646, 0.004301, 0.003871]
```

```mermaid
xychart-beta
    title "integer: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.026022
    bar [0.023656, 0.022809, 0.007900, 0.007038, 0.006334]
```

```mermaid
xychart-beta
    title "ioref: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.156533
    bar [0.142303, 0.134878, 0.081784, 0.007456, 0.006710]
```

```mermaid
xychart-beta
    title "mapops: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.108130
    bar [0.096498, 0.098300, 0.093368, 0.048824, 0.043941]
```

```mermaid
xychart-beta
    title "mvar: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.195100
    bar [0.176263, 0.177364, 0.017442, 0.007287, 0.006559]
```

```mermaid
xychart-beta
    title "nqueens: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.096396
    bar [0.085521, 0.087633, 0.035757, 0.005704, 0.005134]
```

```mermaid
xychart-beta
    title "strings: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.620528
    bar [0.564116, 0.551234, 0.110371, 0.134345, 0.120910]
```

```mermaid
xychart-beta
    title "sumloop: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.383112
    bar [0.133211, 0.132849, 0.348284, 0.015102, 0.013592]
```
