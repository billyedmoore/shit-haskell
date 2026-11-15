# Numeric literal runtime measurements

Raw samples and normalized commands: [runtime-literals.json](runtime-literals.json).

Native runtime; median of 3 alternating samples, 1 untimed correctness warmups per build.
Builds excluded; process startup included. Every stdout/stderr/exit result agrees.
CPU affinity: 0. GHC uses default RTS settings.

| Program | Before ms | After ms | GHC -O0 ms | GHC -O1 ms | 0.9×O1 ms |
|---|---:|---:|---:|---:|---:|
| fib | 541.072 | 247.086 | 16.560 | 3.471 | 3.124 |
| integer | 119.116 | 102.735 | 5.429 | 5.246 | 4.722 |
| ioref | 2139.022 | 2279.883 | 50.576 | 5.464 | 4.918 |
| mapops | 1029.347 | 959.790 | 74.079 | 46.954 | 42.258 |
| mvar | 163.243 | 168.658 | 10.682 | 5.462 | 4.916 |
| nqueens | 383.228 | 331.353 | 18.948 | 4.252 | 3.827 |
| strings | 7535.894 | 7128.742 | 82.763 | 100.507 | 90.456 |
| sumloop | 33254.132 | 20223.921 | 251.093 | 16.620 | 14.958 |

```mermaid
xychart-beta
    title "fib: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.595179
    bar [0.541072, 0.247086, 0.016560, 0.003471, 0.003124]
```

```mermaid
xychart-beta
    title "integer: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.131028
    bar [0.119116, 0.102735, 0.005429, 0.005246, 0.004722]
```

```mermaid
xychart-beta
    title "ioref: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 2.507872
    bar [2.139022, 2.279883, 0.050576, 0.005464, 0.004918]
```

```mermaid
xychart-beta
    title "mapops: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 1.132282
    bar [1.029347, 0.959790, 0.074079, 0.046954, 0.042258]
```

```mermaid
xychart-beta
    title "mvar: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.185524
    bar [0.163243, 0.168658, 0.010682, 0.005462, 0.004916]
```

```mermaid
xychart-beta
    title "nqueens: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.421551
    bar [0.383228, 0.331353, 0.018948, 0.004252, 0.003827]
```

```mermaid
xychart-beta
    title "strings: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 8.289483
    bar [7.535894, 7.128742, 0.082763, 0.100507, 0.090456]
```

```mermaid
xychart-beta
    title "sumloop: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 36.579545
    bar [33.254132, 20.223921, 0.251093, 0.016620, 0.014958]
```

Follow-up checks reuse the same binaries with seven alternating samples. Original three-run medians showed ioref 6.6% slower and mvar 3.3% slower. Follow-up medians were:

| Program | Before ms | After ms | Change |
|---|---:|---:|---:|
| ioref | 2035.292 | 2057.569 | +1.1% |
| mvar | 163.400 | 165.004 | +1.0% |
