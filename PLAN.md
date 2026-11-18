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
- **Corpus:** 265 differential tests, both back ends, all passing.
- **Boot libraries:** deepseq, directory, filepath, bytestring, exceptions, file-io pass their
  suites; unix 15/16 and os-string 14/16 (same failures as GHC); parsec 3/4 (a timing test);
  binary 141/142 (needs PolyKinds); time times out; text needs a GHC plugin.
- **Projects:** ShellCheck and its ~45 dependencies match the GHC build. hlint builds and
  runs (not yet compared with its GHC build).
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

### 1. Large projects

More real programs built with their own build systems and compared with their GHC builds:
hlint (then its tests), cabal-install, pandoc, aeson, megaparsec, lens, and in the end GHC
itself. Alongside: text (plugin), binary (PolyKinds), time (speed). Language features come as
these need them: Template Haskell `[d| |]`, type and pattern splices, `reify` of classes;
PolyKinds and kind inference; Arrows.

### 2. Diagnostics

Error and warning messages match GHC's text (`difftest --diag`), `-W` flags included.

### 3. Usability

The command line behaves like GHC's: `--help`, `--version`/`--info`, clear errors for unknown
flags, and `-O0`/`-O1`/`-O2` meaning different things (`-O0` compiles fast, `-O2` optimises
hardest).

### 4. Independence

Nothing reads GHC's files or runs its tools; both corpora pass with `ghc` off `PATH`. gcc, as
and ld stay as assembler and linker.

### 5. Speed

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
