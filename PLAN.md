# Plan

hsc is a Haskell compiler written in std-only Rust that aims to be a drop-in replacement for
GHC 9.12.2. It is built by hill-climbing: GHC is the oracle, every difference in behaviour is a
failing test, and each step makes one more program behave as it does with GHC. `PROGRESS.md`
records where things stand; open work is tracked in the GitHub issues.

## Where it stands

- A complete pipeline: front end, type checker, Core, an interpreter (used for Template Haskell
  and `runghc`) and a native x86-64 back end with a Rust runtime (copying GC, threads, STM, FFI).
- GHC's command line and package databases: `cabal build/test -w <shims>/ghc` works.
- The boot libraries are GHC's own sources, and most pass their own test suites.
- Real projects build and match their GHC builds: ShellCheck (its whole test suite and output on
  real scripts), and hlint's dependencies up to and including ghc-lib-parser.
- The weakness is speed. Generated code is 10–500× slower than `ghc -O0`, and large packages
  compile slowly, which now limits how fast the rest of the work can go.

## Working rules

- Std-only Rust, no crates. GHC is used only as the oracle (difftest) and never by hsc itself.
- Every fix comes with a corpus test (`tests/run`, `tests/fail`, a `.flags` file where needed)
  whose output is compared with GHC's.
- The gate for every change is both corpora passing, interpreter and native:
  `./target/debug/difftest --hsc <hsc> tests/`, and the same with a `--native` wrapper. CI runs
  both on each pull request.
- Work goes through pull requests: CI green, a review, then a rebase merge.
- A package's own test suite is run under cabal against hsc's build (`tools/pkgtest.sh`), and
  a project counts as done when its tests and output match the GHC build's.
- Speed work shows its results in each pull request as charts of before, after, and GHC.

## Roadmap, in order

### 1. Speed

**Targets:**
- Generated programs run in at most 0.9× the time of `ghc -O1` on `bench/` and on the large
  projects' test suites, and at least as fast as `ghc -O0`.
- Compile times are comparable to GHC's for the same packages.

**Compile time first**, since it limits every iteration:
- Real interface files, so that base and a package's modules are not re-checked for every
  program and every downstream package.
- Profile the driver and the type checker on ghc-lib-parser and the corpus (`HSC_TIME`), and
  remove passes that grow faster than the code they check.
- Compile a package's modules in parallel.

**Then run time**, measured with `difftest --bench` against `ghc -O0` and `-O1`, and on the
build tools hsc builds (happy, alex):
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
