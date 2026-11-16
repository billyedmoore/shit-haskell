# Specialising local functions at constant dictionaries: native runtime

[Raw samples, commands and metadata](runtime-local-specialisation.json).

[Hosted run](https://github.com/billyedmoore/shit-haskell/actions/runs/43884638611); AMD EPYC 7763 64-Core Processor, CPU 0, Rust 1.92.0 release, GHC 9.12.2.
Before `847afc77d2980aa3a5afcc1bd893719df9bb82d1`, after `fc54172e25b6f6c6bb98fb03f29d7719d2ca6890`.

Native runtime; median of 3 alternating samples, 1 untimed correctness warmups per build.
Builds excluded; process startup included. Every stdout/stderr/exit result agrees.
CPU affinity: 0. GHC uses default RTS settings.

| Program | Before ms | After ms | GHC -O0 ms | GHC -O1 ms | 0.9×O1 ms |
|---|---:|---:|---:|---:|---:|
| fib | 19.079 | 23.858 | 33.031 | 4.892 | 4.403 |
| integer | 30.747 | 30.647 | 8.064 | 7.175 | 6.457 |
| ioref | 297.297 | 297.535 | 83.763 | 7.807 | 7.026 |
| mapops | 145.577 | 137.887 | 95.449 | 51.108 | 45.997 |
| mvar | 195.057 | 189.183 | 23.201 | 10.097 | 9.087 |
| nqueens | 132.896 | 100.374 | 33.307 | 5.637 | 5.073 |
| strings | 1153.769 | 1130.950 | 106.718 | 139.725 | 125.752 |
| sumloop | 288.320 | 284.541 | 336.688 | 15.741 | 14.167 |

```mermaid
xychart-beta
    title "fib: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.036334
    bar [0.019079, 0.023858, 0.033031, 0.004892, 0.004403]
```

```mermaid
xychart-beta
    title "integer: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.033822
    bar [0.030747, 0.030647, 0.008064, 0.007175, 0.006457]
```

```mermaid
xychart-beta
    title "ioref: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.327288
    bar [0.297297, 0.297535, 0.083763, 0.007807, 0.007026]
```

```mermaid
xychart-beta
    title "mapops: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.160135
    bar [0.145577, 0.137887, 0.095449, 0.051108, 0.045997]
```

```mermaid
xychart-beta
    title "mvar: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.214563
    bar [0.195057, 0.189183, 0.023201, 0.010097, 0.009087]
```

```mermaid
xychart-beta
    title "nqueens: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.146186
    bar [0.132896, 0.100374, 0.033307, 0.005637, 0.005073]
```

```mermaid
xychart-beta
    title "strings: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 1.269146
    bar [1.153769, 1.130950, 0.106718, 0.139725, 0.125752]
```

```mermaid
xychart-beta
    title "sumloop: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.370357
    bar [0.288320, 0.284541, 0.336688, 0.015741, 0.014167]
```
