# Zip fusion (PR #66)

Instructions (callgrind, millions), after #65 vs this change; outputs identical.

| program | before | after | change |
|---|---:|---:|---:|
| nqueens | 145.1 | 66.6 | −54.1% |
| mvar | 169.9 | 158.2 | −6.9% |
| strings | 2,565.5 | 2,547.7 | −0.7% |
| others | | | within 0.01% |

Hosted (median of 3, [run](https://github.com/billyedmoore/shit-haskell/actions/runs/44207562288)):

| program | before ms | after ms | × GHC -O1 | × GHC -O0 |
|---|---:|---:|---:|---:|
| fib | 6.2 | 5.6 | 1.26 | 0.16 |
| sumloop | 22.0 | 32.3 | 2.03 | 0.10 |
| mapops | 117.2 | 109.4 | 1.92 | 1.04 |
| integer | 23.9 | 22.6 | 3.03 | 2.69 |
| strings | 574.1 | 560.5 | 4.09 | 5.48 |
| ioref | 82.0 | 80.7 | 11.00 | 0.99 |
| nqueens | 45.7 | 17.6 | 3.57 | 0.55 |
| mvar | 56.5 | 51.0 | 6.29 | 2.98 |

Geometric mean: 3.33× GHC -O1, 0.89× GHC -O0.
