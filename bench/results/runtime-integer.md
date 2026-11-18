# Integer dispatch and bignum loops (PR #71)

Instructions (callgrind, millions), main e7c32be vs this change; outputs identical.

| program | before | after | change |
|---|---:|---:|---:|
| integer | 160.5 | 117.6 | −26.7% |
| mvar | 152.0 | 143.4 | −5.6% |
| others | | | within 0.05% |

Hosted results: pending.
