# IO loop fusion (PR #68)

Instructions (callgrind, millions), main c986a1f vs this change; outputs identical.

| program | before | after | change |
|---|---:|---:|---:|
| ioref | 409.1 | 260.7 | −36.3% |
| mvar | 158.2 | 152.0 | −3.9% |
| others | | | within 0.01% |

Hosted (median of 3, [run](https://github.com/billyedmoore/shit-haskell/actions/runs/44223899926)):

| program | before ms | after ms | × GHC -O1 | × GHC -O0 |
|---|---:|---:|---:|---:|
| fib | 4.6 | 4.9 | 1.15 | 0.27 |
| sumloop | 17.9 | 28.6 | 2.04 | 0.15 |
| mapops | 85.2 | 84.2 | 2.00 | 1.17 |
| integer | 19.4 | 19.9 | 3.11 | 2.73 |
| strings | 469.0 | 466.8 | 4.25 | 5.52 |
| ioref | 58.5 | 53.2 | 7.35 | 0.95 |
| nqueens | 17.9 | 18.6 | 3.77 | 0.63 |
| mvar | 39.9 | 33.0 | 4.73 | 2.35 |

Geometric mean: 3.08× GHC -O1, 1.00× GHC -O0.
