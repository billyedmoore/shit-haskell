# Inline Char#/Word#/tagToEnum# and static small Integer literals: native runtime

[Raw samples, commands and metadata](runtime-char-word-integer.json).

[Hosted run](https://github.com/billyedmoore/shit-haskell/actions/runs/43849023490); Intel(R) Xeon(R) 6975P-C, CPU 0, Rust 1.92.0 release, GHC 9.12.2.
Before `5aaec4fb4536f72198528ea4b24968bb6b171320`, after `c013e6228441340940bebb005cbf6dd57fec8b96`.

Native runtime; median of 3 alternating samples, 1 untimed correctness warmups per build.
Builds excluded; process startup included. Every stdout/stderr/exit result agrees.
CPU affinity: 0. GHC uses default RTS settings.

| Program | Before ms | After ms | GHC -O0 ms | GHC -O1 ms | 0.9×O1 ms |
|---|---:|---:|---:|---:|---:|
| fib | 15.406 | 11.483 | 16.589 | 3.112 | 2.801 |
| integer | 56.820 | 56.163 | 4.883 | 4.653 | 4.188 |
| ioref | 639.862 | 667.896 | 52.753 | 5.562 | 5.006 |
| mapops | 111.970 | 89.287 | 69.184 | 37.275 | 33.547 |
| mvar | 80.202 | 79.724 | 11.416 | 5.021 | 4.519 |
| nqueens | 109.709 | 81.195 | 22.332 | 4.397 | 3.957 |
| strings | 2384.350 | 798.630 | 76.236 | 105.622 | 95.060 |
| sumloop | 164.017 | 155.628 | 204.355 | 16.055 | 14.449 |

```mermaid
xychart-beta
    title "fib: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.018248
    bar [0.015406, 0.011483, 0.016589, 0.003112, 0.002801]
```

```mermaid
xychart-beta
    title "integer: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.062502
    bar [0.056820, 0.056163, 0.004883, 0.004653, 0.004188]
```

```mermaid
xychart-beta
    title "ioref: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.734686
    bar [0.639862, 0.667896, 0.052753, 0.005562, 0.005006]
```

```mermaid
xychart-beta
    title "mapops: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.123167
    bar [0.111970, 0.089287, 0.069184, 0.037275, 0.033547]
```

```mermaid
xychart-beta
    title "mvar: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.088222
    bar [0.080202, 0.079724, 0.011416, 0.005021, 0.004519]
```

```mermaid
xychart-beta
    title "nqueens: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.120680
    bar [0.109709, 0.081195, 0.022332, 0.004397, 0.003957]
```

```mermaid
xychart-beta
    title "strings: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 2.622785
    bar [2.384350, 0.798630, 0.076236, 0.105622, 0.095060]
```

```mermaid
xychart-beta
    title "sumloop: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.224791
    bar [0.164017, 0.155628, 0.204355, 0.016055, 0.014449]
```
