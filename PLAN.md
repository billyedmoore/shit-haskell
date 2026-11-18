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
- **Corpus:** 249 differential tests, both back ends, all passing.
- **Boot libraries:** deepseq, directory, filepath, bytestring, exceptions, file-io pass their
  suites; unix 15/16 and os-string 14/16 (same failures as GHC); parsec 3/4 (a timing test);
  binary 141/142 (needs PolyKinds); time times out; text needs a GHC plugin.
- **Projects:** ShellCheck and its ~45 dependencies match the GHC build. hlint's dependencies
  build up to ghc-lib-parser.
- **Speed:** compile times are close to GHC's (ghc-lib-parser cold ≈ 28 s, warm ≈ 1.4 s).
  Generated code is ~3.3× slower than `ghc -O1` (geometric mean, PR #66's hosted run):

  | program | × GHC -O1 | × GHC -O0 |
  |---|---:|---:|
  | fib | 1.3 | 0.16 |
  | mapops | 1.9 | 1.04 |
  | sumloop | 2.0 | 0.10 |
  | integer | 3.0 | 2.69 |
  | nqueens | 3.6 | 0.55 |
  | strings | 4.1 | 5.48 |
  | mvar | 6.3 | 2.98 |
  | ioref | 11.0 | 0.99 |

## Working rules

- Std-only Rust, no crates. GHC is only the oracle, never used by hsc.
- Every fix has a corpus test compared with GHC: `tests/run` (exit code, stdout, stderr),
  `tests/fail` (accept or reject); `NAME.flags` and `NAME.stdin` sidecars.
- Gate: both corpora pass (`./target/debug/difftest --hsc <hsc> tests/`, and with a `--native`
  wrapper). CI runs both.
- Pull requests: CI green, a review, a rebase merge. Commit trailers name the model.
- PR descriptions are short and plain: what changed, why, the headline number, a chart.
- Speed PRs: before/after instruction counts and a chart; the hosted benchmark is checked
  before merging and recorded in `bench/results/`.
- This file stays short: current state and next steps. A finished item gets a line at most.
- Keep going until the roadmap is done; after an interruption, resume without waiting.

## Roadmap, in order

### 1. Speed

**Target:** generated programs at most 0.9× `ghc -O1` time on `bench/` and the large projects'
test suites, and never slower than `ghc -O0`. Compile times comparable to GHC's.

**Done so far (details in `bench/results/`):** direct calls, unboxed workers and raw returns,
join points, RULES and foldr/build fusion, oneShot, case-of-case, calls by value, INLINE
pragmas, eta-expansion, raw numbers in registers with constant division (PR #63), green
threads: coroutines on one OS thread, safe foreign calls on worker threads (PR #64); list
comprehensions desugared with foldr/build, coercions dropped, partial applications inlined
(PR #65); `zip` fuses with enumerations ("foldr2/left" built in, PR #66); `forM_`/`mapM_`
fuse, and loops made in phase 0 take their state token (PR #68).

**Next:**
- mvar: the MVar primitives and thread switches still go through the generic runtime.
- Call arity, so that fused `length`/`sum` loops take their accumulator (then rules may
  resolve every alias of their heads); `foldr2_left` still called, not inlined.
- ioref: the `(# s, () #)` pairs of each step are still built and taken apart.
- Leaner frame prologues (pointer locals are zeroed at every entry).
- Strings: `[Char]` processing and `show`.
- Compile time: real interface files; parallel module compilation.

### 2. Large projects

Each built with its own build system and compared with its GHC build: hlint (then its tests),
cabal-install, pandoc, GHC itself. Alongside: text (plugin), binary (PolyKinds), time (speed).

### 3. Language surface

As projects need them: Template Haskell `[d| |]`, type and pattern splices, `reify` of classes;
PolyKinds and kind inference; Arrows.

### 4. Diagnostics

Error messages match GHC's text (`difftest --diag`).

### 5. Independence

Nothing reads GHC's files or runs its tools; both corpora pass with `ghc` off `PATH`. gcc, as
and ld stay as assembler and linker.
