# IO loop fusion (PR #68)

Instructions (callgrind, millions), main c986a1f vs this change; outputs identical.

| program | before | after | change |
|---|---:|---:|---:|
| ioref | 409.1 | 260.7 | −36.3% |
| mvar | 158.2 | 152.0 | −3.9% |
| others | | | within 0.01% |

Hosted results: pending.
