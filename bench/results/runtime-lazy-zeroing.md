# Indirections followed inline; frame locals cleared 16 bytes at a time (PR #74)

Instructions (callgrind, millions), main vs this change; outputs identical.

| program | before | after | change |
|---|---:|---:|---:|
| nqueens | 66.7 | 59.2 | −11.2% |
| ioref | 260.6 | 240.0 | −7.9% |
| mapops | 445.0 | 418.0 | −6.1% |
| strings | 1,956.1 | 1,888.0 | −3.5% |
| mvar | 143.4 | 142.1 | −1.0% |
| others | | | within 0.5% |

A first version cleared frame locals only before a collection could happen: fewer
instructions still (sumloop −4%), but the stores were repeated at every slow path, binaries
grew from 11.8 to 80 MB, and the hosted run was slower (integer 13.0 → 16.8 ms, fib 5.5 →
6.8 ms). This version keeps the clearing at entry, with 16-byte stores.

Hosted results: pending.
