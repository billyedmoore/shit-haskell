# Zip fusion (PR #66)

Instructions (callgrind, millions), after #65 vs this change; outputs identical.

| program | before | after | change |
|---|---:|---:|---:|
| nqueens | 145.1 | 66.6 | −54.1% |
| mvar | 169.9 | 158.2 | −6.9% |
| strings | 2,565.5 | 2,547.7 | −0.7% |
| others | | | within 0.01% |

Hosted results: pending.
