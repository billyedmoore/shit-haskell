# Heap spaces on huge pages (PR #70)

Local (WSL, transparent huge pages on request), best of 5, before → after:

| program | page faults | user + system CPU |
|---|---:|---:|
| ioref | 16,668 → 1,344 | 37 → 23 ms |
| mapops | 18,381 → 3,048 | 77 → 68 ms |
| strings | 120,135 → 3,951 | 343 → 327 ms |

Instructions are unchanged (the program does the same work; the kernel does less).

Hosted results: pending.
