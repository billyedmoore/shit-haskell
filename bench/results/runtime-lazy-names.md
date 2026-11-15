# Lazy native diagnostic names: native runtime

Raw samples, commands and runner metadata: [runtime-lazy-names.json](runtime-lazy-names.json).

[Hosted run](https://github.com/billyedmoore/shit-haskell/actions/runs/43729922863); Intel(R) Xeon(R) 6973P-C, CPU 0, Rust 1.92.0 release, GHC 9.12.2.
Before `c7ad614a96129ec7564cbbe410e0a7e7bf93452c`, after `6ae25bbf48b3e1031ad4d546a0d395053230ae6c`.

Native runtime; median of 3 alternating samples, 1 untimed correctness warmups per build.
Builds excluded; process startup included. Every stdout/stderr/exit result agrees.
CPU affinity: 0. GHC uses default RTS settings.

| Program | Before ms | After ms | GHC -O0 ms | GHC -O1 ms | 0.9×O1 ms |
|---|---:|---:|---:|---:|---:|
| fib | 173.691 | 141.925 | 17.243 | 3.186 | 2.868 |
| integer | 108.523 | 71.245 | 5.150 | 4.757 | 4.281 |
| ioref | 2057.943 | 2001.596 | 58.872 | 6.871 | 6.184 |
| mapops | 683.301 | 664.362 | 67.217 | 36.002 | 32.401 |
| mvar | 164.714 | 129.568 | 11.816 | 5.628 | 5.065 |
| nqueens | 252.662 | 237.139 | 23.928 | 4.520 | 4.068 |
| strings | 5480.905 | 5565.834 | 74.230 | 96.535 | 86.882 |
| sumloop | 15525.695 | 15308.135 | 191.027 | 15.403 | 13.862 |

```mermaid
xychart-beta
    title "fib: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.191060
    bar [0.173691, 0.141925, 0.017243, 0.003186, 0.002868]
```

```mermaid
xychart-beta
    title "integer: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.119375
    bar [0.108523, 0.071245, 0.005150, 0.004757, 0.004281]
```

```mermaid
xychart-beta
    title "ioref: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 2.263737
    bar [2.057943, 2.001596, 0.058872, 0.006871, 0.006184]
```

```mermaid
xychart-beta
    title "mapops: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.751631
    bar [0.683301, 0.664362, 0.067217, 0.036002, 0.032401]
```

```mermaid
xychart-beta
    title "mvar: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.181185
    bar [0.164714, 0.129568, 0.011816, 0.005628, 0.005065]
```

```mermaid
xychart-beta
    title "nqueens: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.277928
    bar [0.252662, 0.237139, 0.023928, 0.004520, 0.004068]
```

```mermaid
xychart-beta
    title "strings: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 6.122418
    bar [5.480905, 5.565834, 0.074230, 0.096535, 0.086882]
```

```mermaid
xychart-beta
    title "sumloop: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 17.078265
    bar [15.525695, 15.308135, 0.191027, 0.015403, 0.013862]
```

Eight-program geometric-mean speedup: 1.130438×. The 0.9× GHC -O1 target remains unmet.
Strings measured 1.5% slower; long-program changes are small. No universal gain is claimed.

These are independent paired runs on different hosted CPU models; compare each Before/After pair, rather than chaining their absolute times.

Reproduce with `tools/runtimebench.py --before /path/to/before/hsc --after /path/to/after/hsc --ghc ghc --cpu 0 --runs 3`. Use a common vendored library tree through `HSC_LIBDIR`; builds and the correctness warmup are untimed. Hosted CI records the exact revisions and saves reports and output checks, without archiving executables.
