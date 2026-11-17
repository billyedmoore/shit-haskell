# Green threads (PR #64)

Hosted (AMD EPYC 7763, median of 3, [run](https://github.com/billyedmoore/shit-haskell/actions/runs/44185719437)):

| program | before ms | after ms | × GHC -O1 | × GHC -O0 |
|---|---:|---:|---:|---:|
| fib | 5.4 | 5.6 | 1.20 | 0.19 |
| sumloop | 34.0 | 34.5 | 2.10 | 0.10 |
| mapops | 95.0 | 94.3 | 1.93 | 1.03 |
| integer | 33.1 | 30.4 | 4.19 | 3.93 |
| strings | 556.8 | 551.1 | 4.15 | 5.48 |
| ioref | 86.4 | 81.6 | 10.18 | 1.08 |
| nqueens | 69.9 | 69.5 | 12.87 | 2.29 |
| mvar | 172.7 | 51.3 | 5.20 | 2.37 |

Geometric mean: 3.94× GHC -O1, 1.12× GHC -O0. Locally mvar 815 → 32 ms (GHC -O1: 6 ms).
