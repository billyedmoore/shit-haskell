# List fusion (RULES on by default): native runtime

[Raw samples, commands and metadata](runtime-fusion.json).

[Hosted run](https://github.com/billyedmoore/shit-haskell/actions/runs/43987035197); AMD EPYC 9V45 96-Core Processor, CPU 0, Rust 1.92.0 release, GHC 9.12.2.
Before `337ed00be3793d6d7db69e2b20c8872404e89a60`, after `f13d173333ef9feb0c3fb5dee1821b4051360aa4`.

Native runtime; median of 3 alternating samples, 1 untimed correctness warmups per build.
Builds excluded; process startup included. Every stdout/stderr/exit result agrees.
CPU affinity: 0. GHC uses default RTS settings.

| Program | Before ms | After ms | GHC -O0 ms | GHC -O1 ms | 0.9×O1 ms |
|---|---:|---:|---:|---:|---:|
| fib | 6.182 | 5.190 | 16.607 | 3.846 | 3.461 |
| integer | 19.125 | 17.514 | 6.156 | 5.633 | 5.069 |
| ioref | 83.699 | 78.250 | 34.694 | 5.643 | 5.079 |
| mapops | 76.283 | 74.030 | 53.851 | 33.276 | 29.949 |
| mvar | 143.316 | 144.225 | 10.695 | 5.928 | 5.335 |
| nqueens | 57.499 | 54.394 | 17.594 | 4.266 | 3.840 |
| strings | 425.747 | 402.304 | 67.952 | 90.143 | 81.129 |
| sumloop | 42.722 | 52.106 | 136.157 | 14.955 | 13.459 |

```mermaid
xychart-beta
    title "fib: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.018268
    bar [0.006182, 0.005190, 0.016607, 0.003846, 0.003461]
```

```mermaid
xychart-beta
    title "integer: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.021037
    bar [0.019125, 0.017514, 0.006156, 0.005633, 0.005069]
```

```mermaid
xychart-beta
    title "ioref: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.092068
    bar [0.083699, 0.078250, 0.034694, 0.005643, 0.005079]
```

```mermaid
xychart-beta
    title "mapops: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.083911
    bar [0.076283, 0.074030, 0.053851, 0.033276, 0.029949]
```

```mermaid
xychart-beta
    title "mvar: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.158648
    bar [0.143316, 0.144225, 0.010695, 0.005928, 0.005335]
```

```mermaid
xychart-beta
    title "nqueens: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.063249
    bar [0.057499, 0.054394, 0.017594, 0.004266, 0.003840]
```

```mermaid
xychart-beta
    title "strings: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.468321
    bar [0.425747, 0.402304, 0.067952, 0.090143, 0.081129]
```

```mermaid
xychart-beta
    title "sumloop: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.149773
    bar [0.042722, 0.052106, 0.136157, 0.014955, 0.013459]
```
