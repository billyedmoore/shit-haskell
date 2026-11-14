# Plan

hsc is a Haskell compiler written in std-only Rust that aims to be a drop-in replacement for
GHC 9.12.2. It is built by hill-climbing: GHC is the oracle, every difference in behaviour is a
failing test, and each step makes one more program behave as it does with GHC. This file is the
plan and the record of where things stand, and is updated as work lands; open work is tracked in
the GitHub issues.

## Where it stands

- **Compiler:**
  - A front end for the Haskell 2010 and GHC2021 surface and most extensions real code uses.
  - A type checker with classes, GADTs, type and data families, rank-N types and quantified
    constraints, and Core.
  - An interpreter, used for Template Haskell and `runghc`, and a native x86-64 back end with a
    Rust runtime: a copying GC, GHC's scheduling of threads, STM, and FFI.
- **Driver:** GHC's command line and package databases, and `hsc-pkg` for the commands cabal
  uses; `cabal build/test -w <shims>/ghc` works.
- **Corpus:** 204 differential tests for both back ends, compared with GHC.
- **Boot libraries:** the vendored GHC sources (`libs/`). Their own test suites:
  - Pass: deepseq, directory, filepath, bytestring, exceptions, file-io.
  - unix passes 15 of 16 suites; the one failure fails the same way with GHC on this kernel.
  - parsec passes 3 of 4; the fourth is a timing test.
  - binary's qc fails 1 of 142 (a promoted list's TypeRep, which needs PolyKinds).
  - os-string fails the same 2 of 16 as GHC.
  - time's main suites build but time out.
  - text needs tasty-inspection-testing, a GHC plugin.
- **Projects:**
  - ShellCheck 0.11 and its ~45 dependencies build and match the GHC build: the whole test suite,
    and its output on real scripts.
  - hlint's dependencies build up to and including ghc-lib-parser and mono-traversable.
- **Weakness: speed.** Generated code is 10–500× slower than `ghc -O0` (baseline below), and
  large packages compile slowly, which limits how fast the rest of the work can go. A cold
  compile of all 425 exposed ghc-lib-parser modules takes a median 63.1 s, down from 72.8 s
  before class-indexed instance lookup (paired runs; measurements below).

## Working rules

- Std-only Rust, no crates. GHC is used only as the oracle (difftest) and never by hsc itself.
- Every fix comes with a corpus test whose behaviour is compared with GHC's:
  - `tests/run`: programs, compared on exit code, stdout and stderr.
  - `tests/fail`: programs GHC rejects, compared on accept or reject.
  - Sidecars: `NAME.flags` for compiler flags, `NAME.stdin` for input.
- The gate for every change is both corpora passing, interpreter and native:
  `./target/debug/difftest --hsc <hsc> tests/`, and the same with a `--native` wrapper. CI runs
  both on each pull request.
- Work goes through pull requests: CI green, a review, then a rebase merge.
- Commit trailers name the specific model and variant (`Co-Authored-By: GPT-6.1 Sol
  <noreply@openai.com>`). Pull request descriptions also claim authorship by that model.
- A package's own test suite is run under cabal against hsc's build (`tools/pkgtest.sh`), and
  a project counts as done when its tests and output match the GHC build's.
- Every speed pull request includes graphs of before, after, and GHC, with the measurement
  commands and conditions; run-time graphs show both `-O0` and `-O1` and the 0.9× target.
- This file is kept up to date as work lands.

## Roadmap, in order

### 1. Speed

**Targets:**
- Generated programs run in at most 0.9× the time of `ghc -O1` on `bench/` and on the large
  projects' test suites, and at least as fast as `ghc -O0`.
- Compile times are comparable to GHC's for the same packages.

**Compile time first**, since it limits every iteration:
- Types share their parts, so copying, zonking and normalising a large type are cheap (done:
  a derived `Generic` of 200 constructors checks in 2.2 s against GHC's 3.8 s).
- Instance lookup indexes by class and resolves each wanted's types once, skipping kind checks
  without annotations (done). With the module cache disabled, median of three alternating
  before/after runs: ghc-lib-parser's full import set compiles in 72.8 → 63.1 s (type checking
  47.5 → 39.1 s). The focused `tools/compilebench.py` front-end benchmark takes 0.371 → 0.129 s;
  GHC 9.12.2 with `-fno-code` takes 1.070 s at `-O0` and 1.067 s at `-O1`. These are compile-time
  measurements, including process startup; the native run-time baseline below is unchanged.
- Real interface files, so that base and a package's modules are not re-checked for every
  program and every downstream package.
- Profile the driver and the type checker on ghc-lib-parser and the corpus (`HSC_TIME`), and
  remove passes that grow faster than the code they check.
- Compile a package's modules in parallel.

**Then run time**, measured with `difftest --bench` against `ghc -O0` and `-O1`, and on the
build tools hsc builds (happy, alex). The baseline (native, best of 3, against `ghc -O0`;
`difftest --bench 3 bench/`):

| program | ghc -O0 ms | hsc ms | ratio |
|---------|-----------:|-------:|------:|
| fib (27) | 16.5 | 592 | 36× |
| integer (3000!) | 6.3 | 129 | 20× |
| ioref (10^6 modifyIORef') | 61 | 3031 | 50× |
| mapops (search tree, 5·10^4) | 145 | 1635 | 11× |
| mvar (2·10^4 round trips) | 12 | 1004 | 84× |
| nqueens (9) | 38 | 583 | 15× |
| strings (words, map) | 188 | 10077 | 54× |
| sumloop (5·10^6 strict Int loop) | 212 | 111100 | 524× |

The work:
- Space: thunk retention, selector thunks in the GC, blackholing.
- Code: Int literals without a run-time `fromInteger`, primops by index, unboxed Int and
  Double paths, a direct calling convention.
- Optimiser: inlining, specialisation at known dictionaries, case-of-case, strictness analysis
  with worker/wrapper, rewrite rules (for `-O1`'s list fusion).
- Runtime: a generational GC, cheaper thunk allocation.

### 2. Large projects

Each built with cabal and compared with its GHC build:
- hlint: its remaining dependencies, then its test suite and its output on real code.
- cabal-install.
- pandoc.

The remaining boot-library suites go alongside:
- text: needs tasty-inspection-testing, a GHC plugin.
- binary's one failure needs PolyKinds.
- time's slow suites wait on speed.

### 3. Language surface

Gaps are filled as projects hit them:
- Template Haskell: `[d| |]` quotations, type and pattern splices, `reify` of classes.
- PolyKinds and kind inference, which also fixes Typeable's kind arguments.
- Arrows.

### 4. Diagnostics

Error messages match GHC's text (`difftest --diag`), starting with those that print the
user's own type variable names.

### 5. Independence

Nothing reads GHC's files or runs GHC's tools; only difftest uses GHC, as the oracle. The check
is that both corpora pass with `ghc` removed from `PATH`. The system gcc, as and ld remain the
assembler and linker.
