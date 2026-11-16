# Raw (unboxed) frame slots for inline primitive results: native runtime

[Raw samples, commands and metadata](runtime-raw-slots.json).

[Hosted run](https://github.com/billyedmoore/shit-haskell/actions/runs/43913824249); AMD EPYC 9V74 80-Core Processor, CPU 0, Rust 1.92.0 release, GHC 9.12.2.
Before `f82fa4a826c12de5749b851854b0e9c85484ba16`, after `7e042dc3c7b03d0a43356a091865737281cdefaa`.

Native runtime; median of 3 alternating samples, 1 untimed correctness warmups per build.
Builds excluded; process startup included. Every stdout/stderr/exit result agrees.
CPU affinity: 0. GHC uses default RTS settings.

| Program | Before ms | After ms | GHC -O0 ms | GHC -O1 ms | 0.9×O1 ms |
|---|---:|---:|---:|---:|---:|
| fib | 20.081 | 14.542 | 23.053 | 4.345 | 3.910 |
| integer | 23.125 | 30.095 | 7.587 | 7.074 | 6.367 |
| ioref | 216.614 | 215.121 | 57.691 | 7.616 | 6.855 |
| mapops | 105.055 | 106.457 | 74.533 | 48.416 | 43.574 |
| mvar | 168.426 | 164.390 | 14.467 | 7.418 | 6.676 |
| nqueens | 79.134 | 67.054 | 23.702 | 4.319 | 3.887 |
| strings | 583.644 | 536.476 | 86.651 | 115.540 | 103.986 |
| sumloop | 184.022 | 122.397 | 204.114 | 14.247 | 12.823 |

```mermaid
xychart-beta
    title "fib: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.025358
    bar [0.020081, 0.014542, 0.023053, 0.004345, 0.003910]
```

```mermaid
xychart-beta
    title "integer: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.033104
    bar [0.023125, 0.030095, 0.007587, 0.007074, 0.006367]
```

```mermaid
xychart-beta
    title "ioref: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.238276
    bar [0.216614, 0.215121, 0.057691, 0.007616, 0.006855]
```

```mermaid
xychart-beta
    title "mapops: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.117103
    bar [0.105055, 0.106457, 0.074533, 0.048416, 0.043574]
```

```mermaid
xychart-beta
    title "mvar: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.185269
    bar [0.168426, 0.164390, 0.014467, 0.007418, 0.006676]
```

```mermaid
xychart-beta
    title "nqueens: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.087047
    bar [0.079134, 0.067054, 0.023702, 0.004319, 0.003887]
```

```mermaid
xychart-beta
    title "strings: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.642008
    bar [0.583644, 0.536476, 0.086651, 0.115540, 0.103986]
```

```mermaid
xychart-beta
    title "sumloop: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.224526
    bar [0.184022, 0.122397, 0.204114, 0.014247, 0.012823]
```
