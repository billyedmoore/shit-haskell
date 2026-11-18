# Collector copying with raw pointers (PR #69)

Instructions (callgrind, millions), main vs this change; outputs identical.

| program | before | after | change |
|---|---:|---:|---:|
| strings | 2,547.7 | 1,951.0 | −23.4% |
| mapops | 460.9 | 445.1 | −3.4% |
| others | | | within 0.05% |

Hosted (median of 3, [run](https://github.com/billyedmoore/shit-haskell/actions/runs/44229773858)):

| program | before ms | after ms | × GHC -O1 | × GHC -O0 |
|---|---:|---:|---:|---:|
| fib | 5.8 | 5.9 | 1.53 | 0.26 |
| sumloop | 17.2 | 17.0 | 1.24 | 0.08 |
| mapops | 80.4 | 79.5 | 1.98 | 1.12 |
| integer | 26.3 | 25.6 | 4.20 | 3.23 |
| strings | 461.1 | 406.4 | 3.82 | 5.10 |
| ioref | 47.2 | 46.0 | 7.03 | 0.82 |
| nqueens | 13.4 | 17.3 | 3.52 | 0.60 |
| mvar | 35.8 | 31.7 | 4.72 | 2.34 |

Geometric mean: 3.03× GHC -O1, 0.91× GHC -O0.
