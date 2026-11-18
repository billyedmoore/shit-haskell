# Lazy clearing of frame locals; indirections followed inline (PR #74)

Instructions (callgrind, millions), main vs this change; outputs identical.

| program | before | after | change |
|---|---:|---:|---:|
| nqueens | 66.7 | 57.6 | −13.6% |
| ioref | 260.6 | 243.1 | −6.7% |
| mapops | 445.0 | 419.5 | −5.7% |
| sumloop | 252.1 | 242.1 | −4.0% |
| strings | 1,956.1 | 1,881.8 | −3.8% |
| fib | 21.1 | 20.8 | −1.5% |
| mvar | 143.4 | 142.4 | −0.7% |
| integer | 117.6 | 117.2 | −0.4% |

Hosted results: pending.
