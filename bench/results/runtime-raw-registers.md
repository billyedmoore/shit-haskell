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

Hosted results: pending.
