# Progress

Reference: GHC 9.12.2 (oracle). Reference sources are extracted under `ref/src` (gitignored).

Run the differential suite:

    cargo build -p difftest
    ./target/debug/difftest --hsc target/debug/hsc tests/

## Milestones

| # | Milestone | Status |
|---|-----------|--------|
| 1 | Differential harness and seed corpus | done: harness agrees with GHC 59/59 (`--hsc ghc` self-check) |
| 2 | Front end, Haskell 2010 (lexer, layout, parser, fixity, renamer) | in progress |
| 3 | Types and Core | not started |
| 4 | Real `base` | not started |
| 5 | Native back end and RTS | not started |
| 6 | Driver and packages | not started |
| 7 | Boot libraries | not started |
| 8 | Remaining extension surface | not started |
| 9 | Large projects | not started |
| 10 | Diagnostics parity | not started |

## Corpus

- `tests/run/`: accepted programs, compared on exit code, stdout and stderr.
- `tests/fail/`: programs GHC rejects, compared on accept/reject.
- Sidecars: `NAME.flags` (compiler flags), `NAME.stdin` (program input).
