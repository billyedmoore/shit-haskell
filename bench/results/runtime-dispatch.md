# Primitive dispatch runtime measurements

Raw samples and normalized commands: [runtime-dispatch.json](runtime-dispatch.json).

Native runtime; median of 3 alternating samples, 1 untimed correctness warmups per build.
Builds excluded; process startup included. Every stdout/stderr/exit result agrees.
CPU affinity: 0. GHC uses default RTS settings.

| Program | Before ms | After ms | GHC -O0 ms | GHC -O1 ms | 0.9×O1 ms |
|---|---:|---:|---:|---:|---:|
| fib | 230.701 | 188.379 | 17.291 | 3.780 | 3.402 |
| integer | 107.000 | 103.236 | 5.539 | 5.280 | 4.752 |
| ioref | 2521.493 | 2211.243 | 54.354 | 5.349 | 4.814 |
| mapops | 930.193 | 848.991 | 80.466 | 50.332 | 45.299 |
| mvar | 163.967 | 166.357 | 12.200 | 5.501 | 4.951 |
| nqueens | 335.361 | 307.606 | 19.792 | 3.925 | 3.533 |
| strings | 7400.412 | 6458.716 | 76.723 | 100.196 | 90.176 |
| sumloop | 23506.639 | 20941.389 | 201.086 | 17.627 | 15.864 |

```mermaid
xychart-beta
    title "fib: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.253771
    bar [0.230701, 0.188379, 0.017291, 0.003780, 0.003402]
```

```mermaid
xychart-beta
    title "integer: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.117700
    bar [0.107000, 0.103236, 0.005539, 0.005280, 0.004752]
```

```mermaid
xychart-beta
    title "ioref: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 2.773642
    bar [2.521493, 2.211243, 0.054354, 0.005349, 0.004814]
```

```mermaid
xychart-beta
    title "mapops: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 1.023212
    bar [0.930193, 0.848991, 0.080466, 0.050332, 0.045299]
```

```mermaid
xychart-beta
    title "mvar: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.182993
    bar [0.163967, 0.166357, 0.012200, 0.005501, 0.004951]
```

```mermaid
xychart-beta
    title "nqueens: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.368897
    bar [0.335361, 0.307606, 0.019792, 0.003925, 0.003533]
```

```mermaid
xychart-beta
    title "strings: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 8.140453
    bar [7.400412, 6.458716, 0.076723, 0.100196, 0.090176]
```

```mermaid
xychart-beta
    title "sumloop: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 25.857303
    bar [23.506639, 20.941389, 0.201086, 0.017627, 0.015864]
```

The eight-program geometric-mean speedup is 1.103×. This is a new paired run;
compare the Before and After columns here rather than timings from earlier sessions.
The 0.9× GHC -O1 target remains unmet on every program.

MVar is variable: the primary three-pair median is 1.5% slower; a seven-pair
same-binary follow-up is 190.894 → 175.126 ms (8.3% faster). Both sets of raw samples
are retained; no consistent MVar gain is claimed.

Reproduce from the repository root, with no competing local builds/tests/profiles:

```sh
HSC_LIBDIR="$PWD/libs" python3 tools/runtimebench.py \
  --before /path/to/before/hsc --after /path/to/after/hsc \
  --ghc /path/to/ghc-9.12.2 --cpu 0 --runs 3 --work target/runtime-dispatch
```

hsc uses release builds of the before commit and this change. GHC uses its default
RTS settings. Every warmup and timed sample agrees on stdout, stderr and exit status.
