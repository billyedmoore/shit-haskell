# Collector copying with raw pointers (PR #69)

Instructions (callgrind, millions), main vs this change; outputs identical.

| program | before | after | change |
|---|---:|---:|---:|
| strings | 2,547.7 | 1,951.0 | −23.4% |
| mapops | 460.9 | 445.1 | −3.4% |
| others | | | within 0.05% |

Hosted results: pending.
