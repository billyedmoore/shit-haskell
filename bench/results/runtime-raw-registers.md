# Raw numbers in registers (PR #63)

Instructions (callgrind, millions), main 6aa00e4 vs this branch; outputs identical.

| program | before | after | change |
|---|---:|---:|---:|
| fib | 28.8 | 21.1 | −27% |
| sumloop | 512 | 252 | −51% |
| mapops | 488 | 466 | −4.6% |
| integer | 161 | 161 | −0.2% |
| strings | 2,606 | 2,572 | −1.3% |
| ioref | 419 | 409 | −2.4% |
| nqueens | 282 | 277 | −1.7% |
| mvar | 169 | 170 | +0.7% |

Local wall time, median of 7: sumloop 30 → 15 ms (GHC -O1: 13 ms).

Hosted (AMD EPYC 7763, median of 3, [run](https://github.com/billyedmoore/shit-haskell/actions/runs/44173095898)):

| program | before ms | after ms | × GHC -O1 | × GHC -O0 |
|---|---:|---:|---:|---:|
| fib | 6.4 | 6.1 | 1.28 | 0.18 |
| sumloop | 46.8 | 20.7 | 1.28 | 0.06 |
| mapops | 110.7 | 103.6 | 1.90 | 1.02 |
| integer | 24.4 | 26.1 | 3.48 | 2.95 |
| strings | 593.3 | 598.1 | 4.21 | 5.37 |
| ioref | 86.0 | 82.6 | 9.01 | 1.01 |
| nqueens | 72.3 | 79.9 | 14.20 | 2.22 |
| mvar | 169.8 | 171.2 | 20.10 | 9.35 |

Geometric mean: 4.30× GHC -O1, 1.18× GHC -O0. nqueens and integer medians rose, but their best samples did not (noise).
