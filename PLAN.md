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
- **Corpus:** 219 differential tests for both back ends, compared with GHC.
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
  compile of all 425 exposed ghc-lib-parser modules with a fresh module cache takes a median
  28.5 s, down from 29.7 s before type substitution retained unchanged shared
  application branches (paired runs; measurements below). Warm builds take 1.75 s versus 1.84 s
  before reusing the resolved source for boot-file lookup (seven alternating pairs).

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
- Use goal mode and keep iterating until the roadmap is complete. If execution or usage limits interrupt work,
  checkpoint the current state and resume work when execution becomes available
  after the reset, without waiting for another instruction to continue.

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
- Cache bookkeeping journals first insertions into the global type and kind environments.
  Key snapshots keep nine lengths, and deltas visit just the new keys; open-family equation
  tails retain their separate count snapshot. Uncacheable modules skip snapshots (done).
  With caching enabled and a fresh module cache for each run, ghc-lib-parser's full import
  set compiles in 51.8 → 33.3 s (median of three alternating pairs, source extraction shared).
  `tools/compilebench.py --workload module-cache` measures a 200-module dependency chain
  against GHC; it disables the checked-module cache and prepares placeholder objects in an
  untimed warmup. `HSC_TIME` now reports interface reads and cache snapshots separately.
- Completed boot interfaces can use the checked-module cache. Entries replace boot
  declarations and dictionary metadata, and validate the actual boot/full SOURCE context
  before loading. Boot-file dependencies, dictionary names, inferred newtype contexts and
  import-order changes have regression coverage. Nested SOURCE dependency keys follow
  boot/full transitions; unknown keys and contextual open-family equations stay uncached.
  The warm comparison against GHC is reproducible with `tools/compilebench.py --workload boot-cache`: the 50-cycle
  workload takes 0.066 → 0.026 s (median of three alternating warm runs), versus GHC's
  0.254 s at `-O0` and 0.275 s at `-O1`; both compilers keep their cached build artifacts.
  The full parser import set warmed from 11.16 → 1.94 s (three alternating pairs).
- Application-equality contraction substitutes solved type variables into each pattern once
  per walk, preserving matching order and observing new substitutions on the next call.
  `tools/compilebench.py --workload application-equalities` checks 500 bindings with 16 nested
  applications each: 12.38 → 2.83 s (median of three alternating runs; GHC `-fno-code` takes
  0.467 s at `-O0` and 0.497 s at `-O1`). All hsc checked-module caching is disabled in this
  focused front-end measurement. The full parser import set, caching enabled with a fresh
  checked-module cache per run, takes 36.13 → 32.73 s; type checking falls
  from 24.19 → 20.66 s (three alternating pairs).
- Saved application equalities now follow the scope of their givens, retaining outer forms
  through nested signatures, rank-N arguments and GADT alternatives. Ending a branch restores
  forms even on solver errors; replacing an existing form counts as a local change. This also
  rejects a later GADT clause that previously borrowed an equality from its sibling. The same
  focused workload takes 2.843 → 0.384 s, versus GHC's 0.466/0.490 s at `-O0`/`-O1`.
  With a fresh checked-module cache for each run, the full parser import set takes
  31.32 → 29.64 s; type checking takes 19.27 → 18.23 s (three alternating pairs).
- Type substitution preserves unchanged shared application branches, including closed parts
  of polymorphic types, while each instantiation still receives fresh type variables.
  Qualified/forall predicates retain constraint-variable canonicalization even for empty
  substitutions. `tools/compilebench.py --workload type-substitution` checks 500 functions at
  a closed 128-layer type: 0.159 → 0.047 s, versus GHC's 0.200/0.202 s at `-O0`/`-O1`.
  The full parser import set with a fresh checked-module cache per run takes
  29.67 → 28.54 s; type checking takes 18.09 → 17.36 s (three alternating pairs).
- Source and boot lookup reuse the already resolved source path and package flags. This
  avoids rereading and hashing a source-carrying interface just to locate its boot file;
  adjacent-file priority, fallback search and missing-boot validation are retained.
  `tools/interfacebench.py` builds a package with each compiler and measures a warm client
  against its installed interfaces: 0.087 → 0.065 s; GHC takes 0.091/0.093 s at `-O0`/`-O1`.
  The package has 50 padded step modules plus a seed; GHC may load interfaces lazily. Warm
  full parser imports take 1.843 → 1.755 s (seven alternating pairs, both compilers using
  the same vendored library tree through `HSC_LIBDIR`).
- Fingerprinting uses fixed MD5 constants and constant compression steps, retaining the RTS
  context layout and streaming behavior. Native `tools/fingerprintbench.py` hashes 128 MiB
  across eight calls: 0.273 → 0.236 s, versus GHC's 0.169/0.168 s at `-O0`/`-O1`
  (median of three alternating runs; the 0.9×`-O1` target is 0.151 s and remains unmet).
  Warm `tools/interfacebench.py` package clients take 0.063 → 0.055 s, versus GHC's
  0.061/0.061 s (seven alternating runs, common hsc library directory). Full parser warm
  timings were inconclusive: 1.914 → 2.065 s over seven pairs, then 1.831 → 1.812 s
  over 21 pairs; no full-parser improvement is claimed for this change.
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
