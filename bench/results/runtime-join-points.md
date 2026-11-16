# Join points and single-use lets: native runtime

[Raw samples, commands and metadata](runtime-join-points.json).

[Hosted run](https://github.com/billyedmoore/shit-haskell/actions/runs/43832393868); AMD EPYC 9V74 80-Core Processor, CPU 0, Rust 1.92.0 release, GHC 9.12.2.
Before `3de863db6f96bb3ed2531a7af4a2b2a327c4519b`, after `6a0ef289f17040704869749bf5ac84ef752f5642`.

Native runtime; median of 3 alternating samples, 1 untimed correctness warmups per build.
Builds excluded; process startup included. Every stdout/stderr/exit result agrees.
CPU affinity: 0. GHC uses default RTS settings.

| Program | Before ms | After ms | GHC -O0 ms | GHC -O1 ms | 0.9×O1 ms |
|---|---:|---:|---:|---:|---:|
| fib | 120.184 | 119.010 | 21.588 | 4.112 | 3.701 |
| integer | 77.959 | 78.843 | 7.815 | 6.967 | 6.270 |
| ioref | 1406.420 | 1259.790 | 61.649 | 6.919 | 6.227 |
| mapops | 389.329 | 367.844 | 70.805 | 41.690 | 37.521 |
| mvar | 191.089 | 183.261 | 13.301 | 7.025 | 6.322 |
| nqueens | 220.391 | 219.803 | 22.802 | 4.648 | 4.183 |
| strings | 4631.147 | 4530.547 | 85.810 | 121.053 | 108.948 |
| sumloop | 10703.219 | 3773.815 | 202.624 | 13.999 | 12.599 |

```mermaid
xychart-beta
    title "fib: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.132202
    bar [0.120184, 0.119010, 0.021588, 0.004112, 0.003701]
```

```mermaid
xychart-beta
    title "integer: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.086727
    bar [0.077959, 0.078843, 0.007815, 0.006967, 0.006270]
```

```mermaid
xychart-beta
    title "ioref: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 1.547062
    bar [1.406420, 1.259790, 0.061649, 0.006919, 0.006227]
```

```mermaid
xychart-beta
    title "mapops: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.428262
    bar [0.389329, 0.367844, 0.070805, 0.041690, 0.037521]
```

```mermaid
xychart-beta
    title "mvar: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.210198
    bar [0.191089, 0.183261, 0.013301, 0.007025, 0.006322]
```

```mermaid
xychart-beta
    title "nqueens: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.242430
    bar [0.220391, 0.219803, 0.022802, 0.004648, 0.004183]
```

```mermaid
xychart-beta
    title "strings: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 5.094262
    bar [4.631147, 4.530547, 0.085810, 0.121053, 0.108948]
```

```mermaid
xychart-beta
    title "sumloop: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 11.773540
    bar [10.703219, 3.773815, 0.202624, 0.013999, 0.012599]
```
