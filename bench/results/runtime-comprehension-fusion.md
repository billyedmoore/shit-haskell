# Comprehension fusion (PR #65)

Instructions (callgrind, millions), main 2dd6c0e vs this change; outputs identical.

| program | before | after | change |
|---|---:|---:|---:|
| nqueens | 277.2 | 145.1 | −47.7% |
| mapops | 465.6 | 460.9 | −1.0% |
| strings | 2,571.6 | 2,565.5 | −0.2% |
| mvar | 170.1 | 169.9 | −0.2% |
| others | | | within 0.1% |

Hosted (AMD EPYC 7763, median of 3, [run](https://github.com/billyedmoore/shit-haskell/actions/runs/44195812034)):

| program | before ms | after ms | × GHC -O1 | × GHC -O0 |
|---|---:|---:|---:|---:|
| fib | 4.8 | 4.8 | 1.17 | 0.22 |
| sumloop | 35.7 | 20.8 | 1.13 | 0.09 |
| mapops | 92.7 | 87.4 | 1.43 | 1.06 |
| integer | 23.2 | 22.1 | 3.94 | 3.32 |
| strings | 446.6 | 450.1 | 3.93 | 4.73 |
| ioref | 78.3 | 59.8 | 8.93 | 0.86 |
| nqueens | 55.3 | 36.5 | 7.53 | 1.50 |
| mvar | 37.4 | 37.8 | 5.12 | 2.46 |

Geometric mean: 3.17× GHC -O1, 1.01× GHC -O0. sumloop and ioref moved with identical or near-identical instructions: runner noise.
