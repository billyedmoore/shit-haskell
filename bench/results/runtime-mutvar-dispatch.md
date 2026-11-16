# Inline MutVar reads/writes; cheaper primitive dispatch: native runtime

[Raw samples, commands and metadata](runtime-mutvar-dispatch.json).

[Hosted run](https://github.com/billyedmoore/shit-haskell/actions/runs/43951984448); AMD EPYC 9V74 80-Core Processor, CPU 0, Rust 1.92.0 release, GHC 9.12.2.
Before `0c289712f04b49f02207e62a387bc23262282f6a`, after `9762d039d274f6242a5f82f2da01cbe29f8e1cc6`.

Native runtime; median of 3 alternating samples, 1 untimed correctness warmups per build.
Builds excluded; process startup included. Every stdout/stderr/exit result agrees.
CPU affinity: 0. GHC uses default RTS settings.

| Program | Before ms | After ms | GHC -O0 ms | GHC -O1 ms | 0.9×O1 ms |
|---|---:|---:|---:|---:|---:|
| fib | 5.653 | 6.459 | 21.301 | 3.802 | 3.422 |
| integer | 24.456 | 22.043 | 7.423 | 5.847 | 5.262 |
| ioref | 216.001 | 107.957 | 55.816 | 7.108 | 6.397 |
| mapops | 93.456 | 92.267 | 71.659 | 41.170 | 37.053 |
| mvar | 174.277 | 156.952 | 17.354 | 7.481 | 6.732 |
| nqueens | 66.618 | 67.943 | 23.048 | 4.456 | 4.011 |
| strings | 499.797 | 492.469 | 83.872 | 112.704 | 101.434 |
| sumloop | 74.211 | 72.681 | 196.934 | 14.286 | 12.857 |

```mermaid
xychart-beta
    title "fib: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.023432
    bar [0.005653, 0.006459, 0.021301, 0.003802, 0.003422]
```

```mermaid
xychart-beta
    title "integer: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.026902
    bar [0.024456, 0.022043, 0.007423, 0.005847, 0.005262]
```

```mermaid
xychart-beta
    title "ioref: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.237601
    bar [0.216001, 0.107957, 0.055816, 0.007108, 0.006397]
```

```mermaid
xychart-beta
    title "mapops: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.102801
    bar [0.093456, 0.092267, 0.071659, 0.041170, 0.037053]
```

```mermaid
xychart-beta
    title "mvar: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.191704
    bar [0.174277, 0.156952, 0.017354, 0.007481, 0.006732]
```

```mermaid
xychart-beta
    title "nqueens: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.074738
    bar [0.066618, 0.067943, 0.023048, 0.004456, 0.004011]
```

```mermaid
xychart-beta
    title "strings: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.549776
    bar [0.499797, 0.492469, 0.083872, 0.112704, 0.101434]
```

```mermaid
xychart-beta
    title "sumloop: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.216628
    bar [0.074211, 0.072681, 0.196934, 0.014286, 0.012857]
```
