# Progress

Reference: GHC 9.12.2 (oracle). Reference sources are extracted under `ref/src` (gitignored).

Run the differential suite:

    cargo build -p difftest -p hsc
    ./target/debug/difftest --hsc target/debug/hsc tests/

Parse-only dev mode: `target/debug/hsc [-X<ext>] [--dump-ast] file.hs`.

## Milestones

| # | Milestone | Status |
|---|-----------|--------|
| 1 | Differential harness and seed corpus | done: harness agrees with GHC 59/59 (`--hsc ghc` self-check) |
| 2 | Front end, Haskell 2010 (lexer, layout, parser, fixity, renamer) | in progress: lexer, layout, parser done; fixity and renamer pending |
| 3 | Types and Core | not started |
| 4 | Real `base` | not started |
| 5 | Native back end and RTS | not started |
| 6 | Driver and packages | not started |
| 7 | Boot libraries | not started |
| 8 | Remaining extension surface | not started |
| 9 | Large projects | not started |
| 10 | Diagnostics parity | not started |

## Front-end measurements

- Corpus `tests/run` (40 programs): all parse.
- Corpus `tests/fail` (19 programs): 5 are rejected by the parser (matching GHC); the other 14 are
  renamer or type errors, which later milestones must catch.
- GHC 9.12 testsuite (`should_run` and `should_compile`, excluding CPP files): 2742 of 4315 parse.
  Remaining failures are mostly outside milestone 2: TypeFamilies and associated types, MagicHash and
  unboxed syntax, TemplateHaskell and quasi-quotes, DataKinds promotion (`'`), implicit parameters,
  RecursiveDo, TransformListComp, view patterns, and standalone kind signatures' users.

## Corpus

- `tests/run/`: accepted programs, compared on exit code, stdout and stderr.
- `tests/fail/`: programs GHC rejects, compared on accept/reject.
- Sidecars: `NAME.flags` (compiler flags), `NAME.stdin` (program input).
