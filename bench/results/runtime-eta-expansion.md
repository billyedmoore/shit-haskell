# Eta-expansion, constructor fields as globals: native runtime

[Raw samples, commands and metadata](runtime-eta-expansion.json).

[Hosted run](https://github.com/billyedmoore/shit-haskell/actions/runs/44050640988); AMD EPYC 9V45, Rust 1.92.0 release, GHC 9.12.2.
Before `c2abcf9`, after `e00ab36`. ioref 77.3 → 52.2 ms, nqueens 53.7 → 48.3 ms, integer
19.5 → 16.5 ms, mvar 141 → 135 ms, fib 5.8 → 5.4 ms; the rest within noise.

Native runtime; median of 3 alternating samples, 1 untimed correctness warmups per build.
Builds excluded; process startup included. Every stdout/stderr/exit result agrees.
CPU affinity: 0. GHC uses default RTS settings.

| Program | Before ms | After ms | GHC -O0 ms | GHC -O1 ms | 0.9×O1 ms |
|---|---:|---:|---:|---:|---:|
| fib | 5.793 | 5.412 | 18.383 | 3.987 | 3.588 |
| integer | 19.502 | 16.460 | 6.710 | 5.990 | 5.391 |
| ioref | 77.255 | 52.248 | 39.099 | 5.812 | 5.231 |
| mapops | 75.764 | 75.894 | 59.240 | 35.208 | 31.688 |
| mvar | 141.062 | 134.705 | 13.023 | 6.474 | 5.827 |
| nqueens | 53.665 | 48.328 | 17.529 | 4.129 | 3.716 |
| strings | 405.762 | 398.167 | 69.737 | 94.723 | 85.251 |
| sumloop | 24.953 | 24.471 | 136.417 | 14.679 | 13.211 |

```mermaid
xychart-beta
    title "fib: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.020221
    bar [0.005793, 0.005412, 0.018383, 0.003987, 0.003588]
```

```mermaid
xychart-beta
    title "integer: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.021452
    bar [0.019502, 0.016460, 0.006710, 0.005990, 0.005391]
```

```mermaid
xychart-beta
    title "ioref: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.084981
    bar [0.077255, 0.052248, 0.039099, 0.005812, 0.005231]
```

```mermaid
xychart-beta
    title "mapops: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.083483
    bar [0.075764, 0.075894, 0.059240, 0.035208, 0.031688]
```

```mermaid
xychart-beta
    title "mvar: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.155168
    bar [0.141062, 0.134705, 0.013023, 0.006474, 0.005827]
```

```mermaid
xychart-beta
    title "nqueens: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.059031
    bar [0.053665, 0.048328, 0.017529, 0.004129, 0.003716]
```

```mermaid
xychart-beta
    title "strings: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.446339
    bar [0.405762, 0.398167, 0.069737, 0.094723, 0.085251]
```

```mermaid
xychart-beta
    title "sumloop: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.150059
    bar [0.024953, 0.024471, 0.136417, 0.014679, 0.013211]
```
