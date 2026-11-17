# Comprehension fusion (PR #65)

Instructions (callgrind, millions), main 2dd6c0e vs this change; outputs identical.

| program | before | after | change |
|---|---:|---:|---:|
| nqueens | 277.2 | 145.1 | −47.7% |
| mapops | 465.6 | 460.9 | −1.0% |
| strings | 2,571.6 | 2,565.5 | −0.2% |
| mvar | 170.1 | 169.9 | −0.2% |
| others | | | within 0.1% |

Hosted results: pending.
