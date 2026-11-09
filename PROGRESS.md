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
| 2 | Front end, Haskell 2010 (lexer, layout, parser, fixity, renamer) | done: lexer, layout, parser, fixity, renamer, interfaces |
| 3 | Types and Core | in progress: HM with levels, classes via dictionaries, defaulting, deriving (Eq, Ord, Show, Enum, Bounded), Core, matcher, lazy interpreter, mini Base library |
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

## Differential results (`tests/`)

- Clone, current: 50 of 59 (run 32/40, fail 18/19). Oracle: GHC 9.12.2.
- Wider sweep over GHC's own `should_run` programs (stored `.stdout` files, 899 with output):
  143 pass, 652 fail to compile, 77 print different output. The stored outputs are old
  (for example `numeric/arith003` assumes a 32-bit `Int`), so prefer real GHC as the oracle
  for broad sweeps.
- Compile failures by cause (sweep): missing modules (Control.Concurrent, Foreign, Control.Exception,
  GHC.Exts, Data.Array, Data.Typeable, ...), MagicHash and unboxed syntax, Arrows (`proc`),
  TransformListComp, ImplicitParams, type families, higher-rank types.
- Known gaps, by cause:
  - Missing library modules: Data.Bits, Control.Exception, Control.Concurrent, Data.Function, Text.Printf.
  - Runtime error text: GHC 9.12 prints `Uncaught exception` blocks with `HasCallStack backtrace`
    lines naming `ghc-internal` source locations; exact parity needs the real base sources (milestone 4).
  - `Float` is an alias of `Double`; GHC shows single precision values.
  - Derived `Read` is not implemented.
  - GADT type refinement is not implemented (`gadt_implicit`).
  - No kind checking (`kind_error`).
  - Interpreter speed: variables are looked up by name in a linked environment; large loops are slow.

## Corpus

- `tests/run/`: accepted programs, compared on exit code, stdout and stderr.
- `tests/fail/`: programs GHC rejects, compared on accept/reject.
- Sidecars: `NAME.flags` (compiler flags), `NAME.stdin` (program input).
