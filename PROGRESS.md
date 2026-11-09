# Progress

Reference: GHC 9.12.2 (oracle). Reference sources are extracted under `ref/src` (gitignored).

Run the differential suite:

    cargo build -p difftest -p hsc
    ./target/debug/difftest --hsc target/debug/hsc tests/

Parse-only dev mode: `target/debug/hsc [-X<ext>] [--dump-ast] file.hs`.

## Milestones

| # | Milestone | Status |
|---|-----------|--------|
| 1 | Differential harness and seed corpus | done |
| 2 | Front end (lexer, layout, parser, fixity, renamer, interfaces) | done for Haskell 2010 + GHC2021 surface syntax |
| 3 | Types and Core (type checker, Core, matcher, interpreter, mini Base) | working; see "What works" |
| 4 | Real `base` (compile GHC's own library sources) | not started; blocked on CPP, MagicHash, unboxed types, type families, Typeable |
| 5 | Native back end and RTS | not started; the interpreter and the lock-based threads are the interim runtime |
| 6 | Driver and packages (cabal, package DBs, interface files) | partial: `-o`, `-i` via search path, `--run`; no package DB, no recompilation avoidance |
| 7 | Boot libraries (containers, bytestring, text, mtl, ...) | partial: own Data.Map/Set, State monad, Printf, Array |
| 8 | Remaining extension surface | partial: GADTs, existentials, newtype deriving, kind checking; missing type families, RankN, fundeps, TH |
| 9 | Large projects | not started |
| 10 | Diagnostics parity | partial: some GHC-format messages, call-site backtraces for error/undefined |

## What works

- Syntax: layout, operators and fixity, sections, records (construction, update, puns, wildcards),
  guards, where, let, case, do, comprehensions, arithmetic sequences, GADT and existential syntax,
  deriving clauses with strategies, typeclass declarations with defaults, standalone deriving.
- Types: Hindley-Milner with levels; multi-parameter classes; superclasses; defaulting; the
  monomorphism restriction; GADT refinement and existentials (scoped givens); kind inference.
- Deriving: Eq, Ord, Show (GHC format), Read, Enum, Bounded, Functor, Foldable, Traversable,
  newtype deriving (dictionary alias), DeriveAnyClass (empty instance).
- Runtime: lazy interpreter over resolved code with tail calls; exceptions (catch, throw, try,
  bracket, finally); IORef; arrays; MVar and forkIO under a global lock; uncaught error output
  in GHC 9.12 format for `error` and `undefined` in user code.
- Libraries (in `libs/`): Base (defining module), Prelude (GHC names), Data.List, Data.Char,
  Data.Maybe, Data.Either, Data.Ord, Data.Function, Data.Bits, Data.Word, Data.Int, Data.Ratio,
  Data.IORef, Data.STRef, Data.Array, Data.Map, Data.Set, Data.Monoid, Data.Functor, Data.Foldable,
  Data.Traversable, Data.Functor.Identity, Control.Monad, Control.Applicative, Control.Exception,
  Control.Concurrent(.MVar), Control.Monad.ST, Control.Monad.State, System.IO, System.Exit,
  System.Environment, System.IO.Unsafe, Text.Printf, Text.Read, Numeric, Debug.Trace.

## Differential results (`tests/`)

- Clone, current: 57 of 59 (run 38/40, fail 19/19). Oracle: GHC 9.12.2.
- The two failures compare stderr for uncaught `head []` and pattern-match failures. GHC prints
  library-internal call stacks (ghc-internal source locations) that need the real base sources.

## Known gaps (priority order)

1. Real base library: compile GHC's `ghc-prim`, `ghc-internal`, `base` (needs CPP, MagicHash,
   unboxed tuples/types, Typeable, type families, rank-N types).
2. Higher-rank types: nested `forall` is approximated by outer quantification; constrained rank-N
   arguments are rejected.
3. Type families, functional dependencies (no improvement), default signatures, associated types.
4. MagicHash, UnboxedTuples, Arrows, TransformListComp, ImplicitParams, TemplateHaskell.
5. Foreign function interface (needs the native back end).
6. Performance: the interpreter is a few hundred times slower than GHC -O0 on tight loops.
7. Wide sweep over GHC's testsuite (`should_run`, 899 programs with output): 171 pass; compile
   failures are dominated by missing modules (Foreign, GHC.Exts, Data.Typeable, GHC.Conc) and
   the unsupported extensions above.

## Corpus

- `tests/run/`: accepted programs, compared on exit code, stdout and stderr.
- `tests/fail/`: programs GHC rejects, compared on accept/reject.
- Sidecars: `NAME.flags` (compiler flags), `NAME.stdin` (program input).
