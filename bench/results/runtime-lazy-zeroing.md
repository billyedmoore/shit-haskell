# Indirections followed inline (PR #74)

Instructions (callgrind, millions), main vs this change; outputs identical.

| program | before | after | change |
|---|---:|---:|---:|
| nqueens | 66.7 | 60.3 | −9.5% |
| ioref | 260.6 | 248.0 | −4.8% |
| mapops | 445.0 | 425.0 | −4.5% |
| strings | 1,956.1 | 1,900.3 | −2.9% |
| others | | | within 1% |

Two things tried and dropped (both cut instructions, both slower on the hosted runner):
- clearing frame locals only before a collection could happen: the stores were repeated at
  every slow path, binaries grew from 11.8 to 80 MB (integer 13.0 → 16.8 ms, fib 5.5 → 6.8 ms);
- clearing locals with 16-byte stores at entry: `rbp` is 8-byte aligned, so some stores split
  cache lines (ioref fastest run 30.1 → 32.1 ms).

Hosted (7 runs, [run](https://github.com/billyedmoore/shit-haskell/actions/runs/44282792448)):

| program | fastest before ms | fastest after ms | median before | median after | × GHC -O1 | × GHC -O0 |
|---|---:|---:|---:|---:|---:|---:|
| fib | 5.4 | 5.3 | 5.7 | 5.8 | 1.14 | 0.20 |
| sumloop | 20.8 | 20.9 | 22.0 | 34.8 | 2.18 | 0.10 |
| mapops | 63.7 | 63.7 | 64.4 | 65.6 | 1.29 | 0.70 |
| integer | 13.7 | 14.5 | 19.1 | 15.8 | 2.07 | 2.07 |
| strings | 267.7 | 268.2 | 269.7 | 270.1 | 1.96 | 2.60 |
| ioref | 34.7 | 34.6 | 35.7 | 35.3 | 4.24 | 0.46 |
| nqueens | 12.0 | 11.9 | 13.3 | 14.2 | 2.87 | 0.43 |
| mvar | 30.3 | 30.3 | 40.7 | 31.9 | 3.55 | 1.69 |

Geometric mean (medians): 2.21× GHC -O1, 0.63× GHC -O0. The fastest runs are within 1% (integer +5.7% switches between two speeds in both builds): the instructions saved do not show in time on this runner.
