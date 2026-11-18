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

Hosted results: pending.
