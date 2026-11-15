# Local variable-alias thunks: native runtime

Raw samples, commands and metadata: [runtime-alias-thunks.json](runtime-alias-thunks.json).

[Hosted run](https://github.com/billyedmoore/shit-haskell/actions/runs/43740944344); AMD EPYC 7763 64-Core Processor, CPU 0, Rust 1.92.0 release, GHC 9.12.2.
Before `01c36cf1dab6089353a3db0b5384e3d99821276d`, after `4d5f3db1192ff90b5b43b5709d41cbb6d63797e3`.

Native runtime; median of 3 alternating samples, 1 untimed correctness warmups per build.
Builds excluded; process startup included. Every stdout/stderr/exit result agrees.
CPU affinity: 0. GHC uses default RTS settings.

| Program | Before ms | After ms | GHC -O0 ms | GHC -O1 ms | 0.9×O1 ms |
|---|---:|---:|---:|---:|---:|
| fib | 225.211 | 169.073 | 28.944 | 4.811 | 4.330 |
| integer | 123.492 | 116.416 | 8.690 | 8.169 | 7.352 |
| ioref | 2865.224 | 1804.312 | 80.040 | 7.670 | 6.903 |
| mapops | 850.951 | 532.875 | 92.902 | 54.231 | 48.808 |
| mvar | 262.330 | 216.720 | 18.280 | 8.487 | 7.638 |
| nqueens | 384.994 | 315.725 | 32.721 | 4.997 | 4.497 |
| strings | 8027.980 | 6111.663 | 112.681 | 142.335 | 128.101 |
| sumloop | 18864.813 | 14778.604 | 337.084 | 16.463 | 14.817 |

```mermaid
xychart-beta
    title "fib: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.247732
    bar [0.225211, 0.169073, 0.028944, 0.004811, 0.004330]
```

```mermaid
xychart-beta
    title "integer: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.135842
    bar [0.123492, 0.116416, 0.008690, 0.008169, 0.007352]
```

```mermaid
xychart-beta
    title "ioref: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 3.151747
    bar [2.865224, 1.804312, 0.080040, 0.007670, 0.006903]
```

```mermaid
xychart-beta
    title "mapops: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.936046
    bar [0.850951, 0.532875, 0.092902, 0.054231, 0.048808]
```

```mermaid
xychart-beta
    title "mvar: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.288563
    bar [0.262330, 0.216720, 0.018280, 0.008487, 0.007638]
```

```mermaid
xychart-beta
    title "nqueens: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.423494
    bar [0.384994, 0.315725, 0.032721, 0.004997, 0.004497]
```

```mermaid
xychart-beta
    title "strings: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 8.830778
    bar [8.027980, 6.111663, 0.112681, 0.142335, 0.128101]
```

```mermaid
xychart-beta
    title "sumloop: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 20.751294
    bar [18.864813, 14.778604, 0.337084, 0.016463, 0.014817]
```

Eight-program geometric-mean speedup: 1.313647×. Every paired median improved; the 0.9× GHC -O1 target remains unmet. This runner differs from earlier reports; compare paired columns within each report.

Reproduce with `tools/runtimebench.py --before /path/to/before/hsc --after /path/to/after/hsc --ghc ghc --cpu 0 --runs 3`. Use common vendored sources through `HSC_LIBDIR`; builds and correctness warmups are untimed.
