# Heap spaces on huge pages (PR #70)

Local (WSL, transparent huge pages on request), best of 5, before → after:

| program | page faults | user + system CPU |
|---|---:|---:|
| ioref | 16,668 → 1,344 | 37 → 23 ms |
| mapops | 18,381 → 3,048 | 77 → 68 ms |
| strings | 120,135 → 3,951 | 343 → 327 ms |

Instructions are unchanged (the program does the same work; the kernel does less).

Hosted (median of 3, [run](https://github.com/billyedmoore/shit-haskell/actions/runs/44234174582)):

| program | before ms | after ms | × GHC -O1 | × GHC -O0 |
|---|---:|---:|---:|---:|
| fib | 7.7 | 6.2 | 1.24 | 0.19 |
| sumloop | 34.5 | 34.4 | 2.16 | 0.10 |
| mapops | 98.8 | 66.5 | 1.24 | 0.67 |
| integer | 28.6 | 19.7 | 2.65 | 2.07 |
| strings | 502.1 | 272.2 | 1.94 | 2.56 |
| ioref | 63.1 | 36.0 | 4.02 | 0.46 |
| nqueens | 18.2 | 12.8 | 2.17 | 0.32 |
| mvar | 48.5 | 46.3 | 6.22 | 2.71 |

Geometric mean: 2.35× GHC -O1, 0.64× GHC -O0.
