# Fused foldl loops take their accumulator directly (oneShot): native runtime

[Raw samples, commands and metadata](runtime-one-shot.json).

[Hosted run](https://github.com/billyedmoore/shit-haskell/actions/runs/43996144741); Intel(R) Xeon(R) 6975P-C, CPU 0, Rust 1.92.0 release, GHC 9.12.2.
Before `87117b9d4240c6730f7173e03373759e5333a9c3`, after `038e9aa7f82109ef98eeaf0941789383c948d74f`.

Native runtime; median of 3 alternating samples, 1 untimed correctness warmups per build.
Builds excluded; process startup included. Every stdout/stderr/exit result agrees.
CPU affinity: 0. GHC uses default RTS settings.

| Program | Before ms | After ms | GHC -O0 ms | GHC -O1 ms | 0.9×O1 ms |
|---|---:|---:|---:|---:|---:|
| fib | 4.435 | 4.690 | 17.159 | 3.585 | 3.227 |
| integer | 17.329 | 16.695 | 4.948 | 4.372 | 3.935 |
| ioref | 82.481 | 86.213 | 52.497 | 5.445 | 4.900 |
| mapops | 74.519 | 82.486 | 71.472 | 37.032 | 33.329 |
| mvar | 69.517 | 69.195 | 11.234 | 5.246 | 4.721 |
| nqueens | 55.317 | 54.320 | 23.409 | 3.826 | 3.443 |
| strings | 363.842 | 355.478 | 76.083 | 92.836 | 83.552 |
| sumloop | 50.920 | 51.770 | 202.890 | 15.532 | 13.979 |

```mermaid
xychart-beta
    title "fib: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.018875
    bar [0.004435, 0.004690, 0.017159, 0.003585, 0.003227]
```

```mermaid
xychart-beta
    title "integer: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.019062
    bar [0.017329, 0.016695, 0.004948, 0.004372, 0.003935]
```

```mermaid
xychart-beta
    title "ioref: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.094835
    bar [0.082481, 0.086213, 0.052497, 0.005445, 0.004900]
```

```mermaid
xychart-beta
    title "mapops: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.090735
    bar [0.074519, 0.082486, 0.071472, 0.037032, 0.033329]
```

```mermaid
xychart-beta
    title "mvar: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.076468
    bar [0.069517, 0.069195, 0.011234, 0.005246, 0.004721]
```

```mermaid
xychart-beta
    title "nqueens: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.060849
    bar [0.055317, 0.054320, 0.023409, 0.003826, 0.003443]
```

```mermaid
xychart-beta
    title "strings: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.400226
    bar [0.363842, 0.355478, 0.076083, 0.092836, 0.083552]
```

```mermaid
xychart-beta
    title "sumloop: native runtime (seconds)"
    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]
    y-axis "Seconds" 0 --> 0.223179
    bar [0.050920, 0.051770, 0.202890, 0.015532, 0.013979]
```
