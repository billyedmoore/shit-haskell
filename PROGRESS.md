# Progress

Reference: GHC 9.12.2 (oracle). Reference sources are extracted under `ref/src` (gitignored).
The plan of record is `/home/billy/.claude/plans/humming-spinning-zebra.md` (13 milestones).

Run the differential suite:

    cargo build -p difftest -p hsc
    ./target/debug/difftest --hsc target/release/hsc tests/

Parse-only dev mode: `target/release/hsc [-X<ext>] [--dump-ast] file.hs`.

## Milestones

| # | Milestone | Status |
|---|-----------|--------|
| 1 | Differential harness and seed corpus | done (`difftest`, `tests/`: 59 of 59); GHC testsuite sweep is a scratch script |
| 2 | Front end: lexer, layout, parser, fixity, renamer, modules | done for the Haskell 2010 and GHC2021 surface, plus CPP, DataKinds promotion, view patterns, pattern synonyms, RecursiveDo, MagicHash literals, TypeApplications |
| 3 | Types and Core | done: HM with classes, defaulting, deriving (incl. standalone), GADTs, existentials, fundeps, kinds, type families; Core lint (`-dcore-lint`) passes on the corpus and the testsuite's compiling programs |
| 4 | Real `base` from GHC sources | done. The libraries are the vendored GHC packages (`libs/base`, `libs/ghc-prim`, `libs/ghc-internal`, `libs/ghc-bignum`); the mini library is removed. Corpus: 59 of 59 (19 of 19 rejections, 40 of 40 runs). Programs run through `GHC.Internal.TopHandler` (uncaught exceptions as GHC reports them, `<<loop>>` as NonTermination, stdout flushed at exit), pattern-match failures carry GHC's source spans, and `HasCallStack` frames are recorded at every call site. Stand-ins for hsc2hs or RTS-only modules are documented in each file (`Stack/CCS`, `ExecutionStack/Internal`, `InfoProv/Types`, `Event/EPoll`, `Event/Poll`, `GHC.Debug`, `RTS/Flags/Test`, hand-preprocessed `Clock`, `rts/Signals.h`). Runtime: the primops the corpus needs (numbers, arrays, mutable variables, masking, exceptions, threads, stable pointers, byte arrays, `Addr#`), with the libc and RTS entry points base imports on Linux. Not yet: most of the remaining primops (about 500 of 656 are unimplemented: they answer "unknown primitive"), and the GHC testsuite and nofib beyond the seed corpus. A hello program takes about 1.3 s, mostly compiling base |
| 5 | Native back end and RTS | not started; the interpreter and the lock-based threads are the runtime |
| 6 | Driver and packages | partial: `-o`, `--run`, `-i` search path, output executables carry the program |
| 7 | Boot libraries | partial: Map, Set, Array, mutable arrays (ST/IO/MArray), State, Printf, Semigroup, Bifunctor, Monad.Fix |
| 8 | Speed parity with GHC | not started. Measured: deep recursion costs seconds of system time (evaluator stack frames); `length` and list walks now run in constant stack |
| 9 | Remaining extension surface | partial: TypeApplications, RecursiveDo, pattern synonyms, view patterns, DataKinds (promotion, promoted lists), constraint variables; missing rank-N, data families, Typeable, TH, arrows |
| 10 | Large projects | not started |
| 11 | Diagnostics parity | partial: GHC-format `error`/`undefined` output with call sites |
| 12 | Runtime and build independence | partial: the compiler uses only Rust std; the generator for GHC.Prim is a development tool whose output is vendored |
| 13 | No system toolchain | not started (native x86-64 encoder, ELF writer); depends on milestone 5 |

## What works

- Syntax: layout (with parse-error(t)), operators and fixity, sections, records (construction,
  update, puns, wildcards, OverloadedRecordDot), guards, where, let, case, do, comprehensions,
  arithmetic sequences, multi-way if (column-delimited), GADT and existential syntax, deriving
  clauses with strategies, type families, standalone deriving, MagicHash identifiers and literals.
- Types: Hindley-Milner with levels; multi-parameter classes; fundep improvement; superclasses;
  defaulting; the monomorphism restriction; GADT refinement; existentials; kind inference;
  scoped type variables; type-family reduction with deferred equalities.
- Deriving: Eq, Ord, Show (GHC format, infix constructors incl. backticks and GADT fixity rule,
  operator fields), Read (records, infix and prefix constructors, operator names), Enum, Bounded,
  Functor, Foldable, Traversable, newtype deriving, DeriveAnyClass.
- Strictness: strict constructor fields are forced on construction.
- Runtime: lazy interpreter with tail calls; thunks memoise values and synchronous exceptions;
  asynchronous exceptions (`throwTo`, `killThread`) are delivered at safe points and freeze the
  interrupted thunk; `<<loop>>` detection that distinguishes threads; preemptive switching every
  4096 evaluation steps once more than one thread exists; stack overflow raised as an exception
  at 4 GiB of thread stack; MVar, forkIO, threadDelay under a global lock.
- Libraries (in `libs/`): Base, Prelude, Data.{List,Char,Maybe,Either,Ord,Function,Bits,Word,Int,
  Ratio,IORef,STRef,Array,Ix,Map(+Strict),Set,Monoid,Semigroup,Functor(+Identity),Foldable,
  Traversable,Kind,Coerce,Proxy,Void,Fixed,Complex,String,Bifunctor,List.NonEmpty,
  Array.{Base,MArray,ST,IO,Unboxed}}, Control.{Monad,Applicative,Exception,Concurrent(+MVar),
  Monad.ST,Monad.State,DeepSeq}, System.{IO,IO.Unsafe,IO.Error,Exit,Environment,Mem}, Text.{Printf,
  Read}, Numeric, Debug.Trace, GHC.Exts, GHC.Stack, Unsafe.Coerce.

## Differential results (`tests/`)

- Clone: 57 of 59 (run passing, 2 known stderr gaps). Oracle: GHC 9.12.2.
- The two failures compare stderr for uncaught `head []` and pattern-match failures: GHC prints
  ghc-internal call-stack frames that need the real base sources.

## GHC testsuite sweep (scratch, not in the repo)

`should_run` programs that have expected stdout: 227 of 939 pass (last sweep). Failures:

- ~600 compile failures: missing modules (Foreign*, GHC.Conc/IO/Word/Int/Base/Float/ST/Types,
  Data.Typeable, Data.Tree, Data.Dynamic, Data.Data, GHC.Generics, GHC.TypeLits, Type.Reflection),
  and syntax outside the supported surface (ImplicitParams, Arrows, mdo, TransformListComp,
  unboxed sums and tuples, BinaryLiterals, LambdaCase, Rank2Types, PatternSynonyms).
- ~100 output mismatches, mostly stderr-free but with platform or flag differences the sweep does
  not model (`.flags` sidecars are not read by the sweep script).
- The sweep script honours `.stdout-ws-64`; `T9407`-style files without a trailing newline still
  differ.

## Known gaps (priority order)

1. Real base library: compile GHC's own `ghc-prim`, `ghc-internal`, `base` (needs CPP, unboxed
   types, Typeable, rank-N types).
2. Higher-rank types: nested `forall` is approximated by outer quantification; constrained
   rank-N arguments are rejected.
3. Syntax: ImplicitParams, Arrows, mdo/RecursiveDo, TransformListComp, unboxed sums, BinaryLiterals,
   LambdaCase `\case` coverage, PatternSynonyms, TemplateHaskell.
4. Missing modules: Data.Typeable, GHC.Generics, GHC.Records, Data.Tree, Data.Dynamic, Foreign.*
   (needs the native back end), GHC.Conc (throwTo currently lives in Control.Concurrent).
5. Performance (milestone 8). Measured on this machine:
   - N-queens and tight loops run about 30x slower than GHC -O0.
   - Deep non-tail recursion (`foldr (+) 0 [1..300000]`) costs several seconds, mostly system
     time from touching the large Rust stack frames of `eval`; reducing frame size and the
     `force`/`eval` pair is the first item.
   - Dictionary-parameterised local functions are not specialised.
6. Typeable, data families, default signatures, rewrite rules.
7. Native code generation and the ELF writer (milestones 5 and 13).

## Corpus

- `tests/run/`: accepted programs, compared on exit code, stdout and stderr.
- `tests/fail/`: programs GHC rejects, compared on accept/reject.
- Sidecars: `NAME.flags` (compiler flags), `NAME.stdin` (program input).
