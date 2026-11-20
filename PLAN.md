# Plan

hsc is a Haskell compiler in std-only Rust, aiming to be a drop-in replacement for GHC 9.12.2.
GHC is the oracle: every difference in behaviour is a failing test, and each step fixes one.
This file says where things stand and what comes next. History lives in git and
`bench/results/`.

## Where it stands

- **Compiler:** Haskell 2010 / GHC2021 and most extensions real code uses; classes, GADTs,
  type and data families, rank-N types. An interpreter (Template Haskell, `runghc`) and a
  native x86-64 back end with a Rust runtime (copying GC, threads, STM, FFI).
- **Driver:** GHC's command line and package databases; `hsc-pkg`; `cabal build/test -w` works.
- **Corpus:** 299 differential tests, both back ends, all passing.
- **Boot libraries:** deepseq, directory, filepath, bytestring, exceptions, file-io pass their
  suites; unix 15/16 and os-string 14/16 (same failures as GHC); parsec 3/4 (a timing test);
  binary 141/142 (needs PolyKinds); time times out; text needs a GHC plugin.
- **Projects:** ShellCheck and its ~45 dependencies match the GHC build. hlint builds and
  matches its GHC build on its own and ShellCheck's sources and passes its 956 tests (~11× slower).
  cabal-install builds and matches its GHC build on `--help`, `check` and the solver (~9× slower).
- **Speed:** compile times are close to GHC's (ghc-lib-parser cold ≈ 28 s, warm ≈ 1.4 s).
  Generated code is ~2.2× slower than `ghc -O1` (geometric mean, PR #74's hosted run);
  speed work resumes after the projects, diagnostics and usability items below:

  | program | × GHC -O1 | × GHC -O0 |
  |---|---:|---:|
  | fib | 1.1 | 0.20 |
  | mapops | 1.3 | 0.70 |
  | strings | 2.0 | 2.60 |
  | integer | 2.1 | 2.07 |
  | sumloop | 2.2 | 0.10 |
  | nqueens | 2.9 | 0.43 |
  | mvar | 3.6 | 1.69 |
  | ioref | 4.2 | 0.46 |

## Working rules

- Std-only Rust, no crates. GHC is only the oracle, never used by hsc.
- Every fix has a corpus test compared with GHC: `tests/run` (exit code, stdout, stderr),
  `tests/fail` (accept or reject); `NAME.flags` and `NAME.stdin` sidecars.
- Gate: both corpora pass (`./target/debug/difftest --hsc <hsc> tests/`, and with a `--native`
  wrapper). CI runs both.
- Pull requests: CI green, a review, a rebase merge. Commit trailers name the model.
- PR descriptions are short and plain: what changed, why, the headline number, a chart.
- Speed PRs: before/after instruction counts and a chart; the hosted benchmark (7 runs per
  build: single runs there vary by up to 1.6×) is checked before merging and recorded in
  `bench/results/`.
- This file stays short: current state and next steps. A finished item gets a line at most.
- Keep going until the roadmap is done; after an interruption, resume without waiting.

## Roadmap, in order

Big leaps first: work that finds or removes many problems at once comes before one-project,
one-fix iterations.

### 1. Coverage at scale

- A Hackage sweep: build the most-used packages (a Stackage snapshot's top few hundred) with
  hsc next to their GHC builds, unattended, and rank the failures by how many packages each
  blocks; fix from the top. Results recorded in `bench/sweep/`.
- GHC's own testsuite through difftest, for a conformance score.
- Sweep of the top 300 Stackage packages: hsc builds 245 of the 300 GHC builds (was 218). This
  PR adds ghc-bignum's API, integer-gmp, RebindableSyntax and CallStack implicit parameters
  (basement, memory, crypton, cborg build). Next, by packages blocked: reflection (lens),
  optics-core, constraints, monad-control, some, sop-core. RebindableSyntax still to rebind:
  do-notation, arithmetic sequences, string and list literals.
- Known gaps it will rank: PolyKinds and kind inference (binary), Template Haskell `[d| |]`,
  type and pattern splices, `foreign import "dynamic"`, collector-run
  finalizers, Arrows, text's plugin.

### 2. Native code per module

Each module's native code compiled once and cached as an object; a link only links. Today a
link regenerates the whole program (cabal-install: 760 MB of assembly, a 228 MB binary against
GHC's 58 MB) and needs memory a small machine lacks. Smaller generated code alongside.

### 3. Speed of real programs

Real programs run 9–17× slower than their GHC builds (hlint, cabal-install), against ~2× on
`bench/`. Their time goes to primitives called through the runtime with boxed results,
dictionaries passed at run time, and `Integer` conversions. Next: primitives compiled inline
on unboxed values, specialisation of overloaded functions across modules, `fromIntegral` at
known types without `Integer`. Measured on hlint's and cabal-install's own workloads.

### 4. Large projects

cabal-install's test suite, then pandoc, aeson, megaparsec, lens, and in the end GHC itself;
text, binary and time alongside (most gaps found by the sweep first).

### 5. Diagnostics

Error and warning messages match GHC's text (`difftest --diag`), `-W` flags included.

### 6. Usability

The command line behaves like GHC's: `--help`, `--version`/`--info`, clear errors for unknown
flags, and `-O0`/`-O1`/`-O2` meaning different things (`-O0` compiles fast, `-O2` optimises
hardest).

### 7. Independence

Nothing reads GHC's files or runs its tools; both corpora pass with `ghc` off `PATH`. gcc, as
and ld stay as assembler and linker.

### 8. Speed target

**Target:** generated programs at most 0.75× `ghc -O2` time on `bench/` and the large
projects' test suites. Compile times comparable to GHC's. (Today: ~2.2× `ghc -O1`.)

**Done so far (details in `bench/results/`):** direct calls, unboxed workers and raw returns,
join points, RULES and foldr/build fusion, oneShot, case-of-case, calls by value, INLINE
pragmas, eta-expansion, raw numbers in registers (#63), green threads (#64), fused
comprehensions, `zip` and `forM_` (#65, #66, #68), a faster collector and huge pages (#69,
#70), faster Integer (#71), inline indirections (#74).

**Next:**
- ioref: `seq` as a case and case-of-case past failing alternatives remove the per-step
  `(# s, () #)` pairs (branch `mvar-fast`, untested).
- mvar: the MVar primitives and thread switches still go through the generic runtime.
- Call arity, so that fused `length`/`sum` loops take their accumulator.
- Leaner frame clearing (clearing only before a collection made binaries 7× larger).
- Strings: a generational collector; `[Char]` processing and `show`.
- Compile time: real interface files; parallel module compilation.
