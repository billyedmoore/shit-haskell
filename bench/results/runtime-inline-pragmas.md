# INLINE/NOINLINE pragmas, prim identities, preemption at function entry: native runtime

[Raw samples, commands and metadata](runtime-inline-pragmas.json).

[Hosted run](https://github.com/billyedmoore/shit-haskell/actions/runs/44038880568); AMD EPYC 9V74, Rust 1.92.0 release, GHC 9.12.2.
Before `a584e1d`, after `87eecaa`. sumloop 60.9 → 35.9 ms (its `mod` inlines), fib 6.9 → 5.2 ms;
ioref 104 → 108 ms (a check at each function's entry); the rest within noise.

Native runtime; median of 3 alternating samples, 1 untimed correctness warmups per build.
Builds excluded; process startup included. Every stdout/stderr/exit result agrees.
CPU affinity: 0. GHC uses default RTS settings.

| Program | Before ms | After ms | GHC -O0 ms | GHC -O1 ms | 0.9×O1 ms |
|---|---:|---:|---:|---:|---:|
| fib | 6.908 | 5.166 | 19.383 | 3.647 | 3.282 |
| integer | 19.783 | 20.060 | 8.191 | 6.466 | 5.820 |
| ioref | 104.188 | 108.344 | 57.128 | 7.248 | 6.523 |
| mapops | 87.343 | 88.018 | 79.302 | 42.745 | 38.470 |
| mvar | 153.800 | 154.991 | 15.222 | 6.514 | 5.863 |
| nqueens | 62.403 | 63.700 | 22.090 | 4.398 | 3.958 |
| strings | 476.772 | 473.789 | 88.081 | 116.768 | 105.091 |
| sumloop | 60.948 | 35.913 | 190.771 | 13.670 | 12.303 |

```mermaid
xychart-beta
    title "fib: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.021322
    bar [0.006908, 0.005166, 0.019383, 0.003647, 0.003282]
```

```mermaid
xychart-beta
    title "integer: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.022066
    bar [0.019783, 0.020060, 0.008191, 0.006466, 0.005820]
```

```mermaid
xychart-beta
    title "ioref: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.119178
    bar [0.104188, 0.108344, 0.057128, 0.007248, 0.006523]
```

```mermaid
xychart-beta
    title "mapops: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.096820
    bar [0.087343, 0.088018, 0.079302, 0.042745, 0.038470]
```

```mermaid
xychart-beta
    title "mvar: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.170490
    bar [0.153800, 0.154991, 0.015222, 0.006514, 0.005863]
```

```mermaid
xychart-beta
    title "nqueens: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.070070
    bar [0.062403, 0.063700, 0.022090, 0.004398, 0.003958]
```

```mermaid
xychart-beta
    title "strings: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.524449
    bar [0.476772, 0.473789, 0.088081, 0.116768, 0.105091]
```

```mermaid
xychart-beta
    title "sumloop: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.209848
    bar [0.060948, 0.035913, 0.190771, 0.013670, 0.012303]
```
