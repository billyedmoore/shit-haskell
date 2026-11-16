# Workers with numeric results return them raw: native runtime

[Raw samples, commands and metadata](runtime-raw-returns.json).

[Hosted run](https://github.com/billyedmoore/shit-haskell/actions/runs/43944860185); Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz, CPU 0, Rust 1.92.0 release, GHC 9.12.2.
Before `d3258c5df27f2d24653e2eb3b61017ca3ea73bb0`, after `63f72e613bd55aff72eaf24fc10db690c59e71b4`.

Native runtime; median of 3 alternating samples, 1 untimed correctness warmups per build.
Builds excluded; process startup included. Every stdout/stderr/exit result agrees.
CPU affinity: 0. GHC uses default RTS settings.

| Program | Before ms | After ms | GHC -O0 ms | GHC -O1 ms | 0.9×O1 ms |
|---|---:|---:|---:|---:|---:|
| fib | 16.507 | 8.922 | 23.403 | 3.898 | 3.508 |
| integer | 24.651 | 24.648 | 6.619 | 6.284 | 5.655 |
| ioref | 242.995 | 256.649 | 68.487 | 7.206 | 6.485 |
| mapops | 97.793 | 100.062 | 91.220 | 51.185 | 46.067 |
| mvar | 110.908 | 124.499 | 23.083 | 8.261 | 7.435 |
| nqueens | 82.842 | 71.904 | 31.492 | 6.646 | 5.981 |
| strings | 443.522 | 455.320 | 92.948 | 117.868 | 106.081 |
| sumloop | 113.659 | 78.963 | 229.752 | 18.263 | 16.437 |

```mermaid
xychart-beta
    title "fib: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.025743
    bar [0.016507, 0.008922, 0.023403, 0.003898, 0.003508]
```

```mermaid
xychart-beta
    title "integer: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.027116
    bar [0.024651, 0.024648, 0.006619, 0.006284, 0.005655]
```

```mermaid
xychart-beta
    title "ioref: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.282314
    bar [0.242995, 0.256649, 0.068487, 0.007206, 0.006485]
```

```mermaid
xychart-beta
    title "mapops: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.110069
    bar [0.097793, 0.100062, 0.091220, 0.051185, 0.046067]
```

```mermaid
xychart-beta
    title "mvar: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.136949
    bar [0.110908, 0.124499, 0.023083, 0.008261, 0.007435]
```

```mermaid
xychart-beta
    title "nqueens: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.091126
    bar [0.082842, 0.071904, 0.031492, 0.006646, 0.005981]
```

```mermaid
xychart-beta
    title "strings: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.500852
    bar [0.443522, 0.455320, 0.092948, 0.117868, 0.106081]
```

```mermaid
xychart-beta
    title "sumloop: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.252727
    bar [0.113659, 0.078963, 0.229752, 0.018263, 0.016437]
```
