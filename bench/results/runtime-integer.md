# Integer dispatch and bignum loops (PR #71)

Instructions (callgrind, millions), main e7c32be vs this change; outputs identical.

| program | before | after | change |
|---|---:|---:|---:|
| integer | 160.5 | 117.6 | −26.7% |
| mvar | 152.0 | 143.4 | −5.6% |
| others | | | within 0.05% |

Hosted (median of 3, [run](https://github.com/billyedmoore/shit-haskell/actions/runs/44243281442)):

| program | before ms | after ms | × GHC -O1 | × GHC -O0 |
|---|---:|---:|---:|---:|
| fib | 7.1 | 7.2 | 1.49 | 0.27 |
| sumloop | 21.1 | 32.5 | 2.08 | 0.10 |
| mapops | 61.9 | 64.5 | 1.31 | 0.69 |
| integer | 25.7 | 21.9 | 2.95 | 2.40 |
| strings | 270.3 | 263.2 | 1.97 | 2.59 |
| ioref | 35.9 | 35.7 | 4.50 | 0.46 |
| nqueens | 12.6 | 12.0 | 2.23 | 0.37 |
| mvar | 37.6 | 33.3 | 4.01 | 1.75 |

Geometric mean: 2.36× GHC -O1, 0.65× GHC -O0.
