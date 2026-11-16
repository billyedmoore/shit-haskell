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
- **Corpus:** 241 differential tests for both back ends, compared with GHC.
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
- **Weakness: speed.** Generated programs remain much slower than GHC (measurements below), and
  large packages have also been slow to compile. In the latest paired comparison on CPU 0,
  a cold compile of all 425 exposed ghc-lib-parser modules with a fresh module cache takes
  27.9 s versus 28.3 s; run variation exceeds this small difference. Warm builds take
  1.34 s versus 1.41 s after kind arrows began sharing their branches (seven alternating pairs).
  The earlier completed-boot cache change reduced warm builds from 11.16 s to 1.94 s.

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
- Keep PR descriptions concise, with key results, graphs and validation.
- Remove old generated corpus executables after preserving reports and raw measurements;
  reuse build directories and retain the compiler baselines needed for current comparisons.
- This file is kept up to date as work lands.
- Keep iterating until the roadmap is complete (asked for on 2026-11-16): each merged pull
  request leads straight into the next item. If execution or usage limits interrupt work,
  checkpoint the current state and resume after the reset without waiting for an instruction.

## Roadmap, in order

### 1. Speed

**Targets:**
- Generated programs run in at most 0.9× the time of `ghc -O1` on `bench/` and on the large
  projects' test suites, and at least as fast as `ghc -O0`.
- Compile times are comparable to GHC's for the same packages.

**Current focus: generated program run time**, following the shared-kind change below.
Remaining compile-time work waits unless it blocks runtime iteration.

**Compile-time work so far and remaining items:**
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
- Checked-module cache entries preserve shared type branches through completed-node references;
  ABI fingerprints retain structural encoding independent of sharing. Invalid references and
  cross-scheme sharing have regression coverage; old local entries use a different format tag.
  `tools/compilebench.py --workload typed-cache` checks warm dependencies of 50 modules,
  each with 64 signatures at a closed 64-layer type: 0.118 → 0.015 s, versus GHC's
  0.129/0.127 s at `-O0`/`-O1` (seven alternating runs, one untimed build each).
  hsc keeps checked Core while GHC keeps native build artifacts. The checked-cache payload
  falls from 6,528,438 to 813,592 bytes on this workload. The full parser's warm build takes
  1.931 → 1.772 s (seven alternating pairs); cold builds take 30.34 → 29.84 s
  (three pairs, within run variation). Its median cache payload falls from 108.6 to 106.6 MB.
- Exported view-pattern synonyms can use the checked-module cache. Their renamed ordinary
  expressions retain qualified names, binders, locations, tuple holes, signatures and records;
  unsupported declaration/statement/TH forms still leave the module uncached. Replay and private
  helper edits have regression coverage. `tools/compilebench.py --workload view-cache` builds
  50 modules with 64 values and one view-pattern synonym each: warm dependency checks take
  0.034 → 0.009 s, versus GHC's 0.107/0.128 s at `-O0`/`-O1` (seven alternating runs,
  one untimed build each, CPU 0, common hsc library directory). hsc retains checked Core while
  GHC retains native objects/interfaces. Full parser warm builds take 1.674 → 1.391 s
  (seven pairs); cold builds take 28.10 → 27.83 s (three pairs, within variation).
- Kind arrows share immutable branches. Pruning clones their parts shallowly, and zonking
  retains unchanged tails while observing new solutions and keeping generic instantiations
  independent. The structural cache encoding is unchanged. `tools/compilebench.py --workload
  wide-kinds` checks 500 functions using a 128-argument constructor: 0.080 → 0.050 s, versus
  GHC's 0.177/0.174 s at `-O0`/`-O1` (seven alternating runs, CPU 0, hsc cache disabled).
  Full parser warm builds take 1.407 → 1.341 s (seven pairs); cold builds take
  28.28 → 27.86 s (three pairs, within variation).
- Real interface files, so that base and a package's modules are not re-checked for every
  program and every downstream package.
- Profile the driver and the type checker on ghc-lib-parser and the corpus (`HSC_TIME`), and
  remove passes that grow faster than the code they check.
- Compile a package's modules in parallel.

**Run time now**, measured against GHC `-O0` and `-O1` with `tools/runtimebench.py`,
and on build tools hsc builds (happy, alex). `difftest --bench` also checks the native baseline.
Representative paired results use Rust 1.92.0 release hsc, GHC 9.12.2 and CPU 0,
one untimed correctness warmup and three alternating timed samples per build.
Builds are excluded; startup is included. Every stdout/stderr/exit result matches.
The table is the static zero-field constructor comparison on a hosted AMD EPYC 9V45 runner.
The target remains **0.9× GHC `-O1` on every program**.

| program | before ms | hsc ms | GHC -O0 ms | GHC -O1 ms | 0.9×O1 target ms |
|---------|----------:|-------:|-----------:|-----------:|----------------:|
| fib | 107.710 | 92.605 | 15.013 | 3.226 | 2.904 |
| integer | 61.658 | 60.242 | 5.325 | 5.274 | 4.747 |
| ioref | 1134.077 | 1096.582 | 36.940 | 5.398 | 4.858 |
| mapops | 302.871 | 305.348 | 53.384 | 30.552 | 27.497 |
| mvar | 168.389 | 167.519 | 10.480 | 6.185 | 5.567 |
| nqueens | 171.541 | 172.681 | 17.310 | 4.294 | 3.865 |
| strings | 3903.332 | 3860.743 | 63.345 | 85.318 | 76.786 |
| sumloop | 8124.657 | 7853.504 | 135.956 | 14.145 | 12.730 |

Canonical standard numeric literals fold through dictionary aliases, eliminating repeated
conversion workers. Custom instances, noncanonical `Base` names, overflowing `Int` literals
and failed parses keep their original calls. That change gave a 1.22× geometric-mean speedup;
[its measurements](bench/results/runtime-literals.md) retain the paired baseline and IO rechecks.

Common ordinary primitives are classified once at startup under the existing program-local
IDs. Their calls reuse static names and skip foreign-signature parsing, while general calls
retain their original dispatch. Scheduler points, argument roots, reservation, exception
handling and final forcing are shared. That paired comparison measured
fib 0.231 → 0.188 s, ioref 2.521 → 2.211 s, sumloop 23.507 → 20.941 s;
eight-program geometric-mean speedup 1.103×. MVar results are mixed (1.5% slower in three
pairs, 8.3% faster in a seven-pair same-binary recheck); no consistent MVar gain is claimed.
[Raw samples, conditions and graphs](bench/results/runtime-dispatch.md) retain both runs.
All runtime targets remain unmet.

The inline evaluated-argument experiment is held out: it removed only 0.4% of instructions
on a scaled sumloop and made fib 2.3% slower over seven alternating pairs.

Native diagnostic body names are decoded on demand from the executable's static table.
Address lookup, empty names and lossy Unicode behavior are preserved. This avoids eagerly
allocating thousands of Strings during startup: 1.130× geometric-mean speedup across eight
programs on a hosted Intel Xeon 6973P-C; integer 108.5 → 71.2 ms and fib 173.7 → 141.9 ms.
Strings measured 1.5% slower; no universal gain is claimed.
[Raw samples, runner metadata and graphs](bench/results/runtime-lazy-names.md) record this
independent paired comparison. Both backends passed 225 tests and all 44 unit tests.

Known global methods can be selected even beside lazy default-method applications.
Constructor identity is checked, while selected non-global fields retain their calls.
The `Ord Int` dictionary has global comparisons beside applied max/min defaults; the old
all-fields-global check excluded it. Custom defaults, unused bottom fields, superclass
offsets and aliases have regression coverage. Both backends passed 226 tests and all
47 unit tests. Its paired comparison measured a geometric-mean speedup of 1.009×,
within variation, so no consistent overall speedup is claimed.
[Samples, runner metadata and graphs](bench/results/runtime-partial-dictionaries.md).

Runtime PRs run paired Before/After/GHC benchmarks on hosted CI, saving raw samples,
compiler revisions and graphs without generated executables. Independent runs can use
different hosted CPU models; their absolute times are not chained together.

Pure local variable aliases no longer allocate forwarding thunks. Resolution preserves
sharing and strict Case nodes, and conservatively retains recursive or unresolved aliases
and bodies with shadowed/reused binders. Post-optimization Core lint checks scope when
requested. Both backends passed 227 tests and all 51 unit tests. That paired
comparison measured a geometric-mean speedup of 1.314×, with all eight medians improved;
ioref 2.865 → 1.804 s and sumloop 18.865 → 14.779 s.
[Raw samples, conditions and graphs](bench/results/runtime-alias-thunks.md).

Native INT cells for -16 through 255 are immutable static values, matching GHC's small
boxed-Int range. Uncached values retain normal allocation. Both backends passed 228 tests
and all 53 unit tests, including direct/indirect static GC roots and numeric boundaries.
Two independent hosted comparisons measured geometric-mean speedups of 1.037× and 1.039×;
sumloop took 12.656 → 11.030 s and 12.299 → 10.751 s. IORef was 4.2% slower initially and
0.6% faster on verification, so no consistent IORef change is claimed. [Both complete runs and graphs](bench/results/runtime-small-integers.md).

Native constructors without fields share per-program static cells. Their constructor tags,
two-word object layout and return convention are preserved; fieldful constructors retain
ordinary allocation. Both backends passed 229 tests and all 55 unit tests, including tag
separation, both return modes and mixed values retained across GC. The table above is this
paired comparison: geometric-mean speedup 1.031×; fib 107.7 → 92.6 ms and sumloop
8.125 → 7.854 s. Mapops and nqueens measured less than 1% slower.
[Raw samples, conditions and graphs](bench/results/runtime-nullary-constructors.md).

Direct native calls to known global functions with exact positive arity bypass generic
dispatch. Argument and tail-frame setup stay unchanged; other calls retain generic dispatch.
Both corpora pass 230/230 and all 58 unit tests pass. The paired comparison measures
a 1.026× geometric-mean speedup, but a second run measures 0.993× and Integer regresses.
No consistent overall speedup is established; fib improves in both runs. All runtime
targets remain unmet. [Raw samples, conditions and graphs](bench/results/runtime-direct-calls.md).

Common binary Int arithmetic and comparisons are classified once at startup, avoiding
repeated string dispatch. Forcing, scheduling, reservation and result handling retain their
existing paths. Both corpora pass 231/231 and all 59 unit tests pass. A three-pair hosted
comparison measures 1.034× geometric-mean speedup; all eight medians improve, though
small differences may be noise. All runtime targets remain unmet.
[Raw samples, conditions and graphs](bench/results/runtime-int-operations.md).

A guard's fall-through was let-bound as a thunk entered in tail position, so guarded loops
pushed an update frame per iteration and retained every thunk (sumloop reached 2 GB). Single-use
lets now move to their use, `case otherwise` folds, and tail-only lets compile as join points
in the enclosing frame (a jump, no allocation). Both corpora pass 232/232. The paired
comparison: sumloop 10.70 → 3.77 s; the other seven are within 10% (ioref 1.41 → 1.26 s).
[Raw samples, conditions and graphs](bench/results/runtime-join-points.md).

A Core simplifier (`core/simplify.rs`) runs before code generation. Binders are renamed apart
first. It inlines small non-recursive global functions; beta-reduces; folds `Int#` constants and
cases of known constructors, literals and dictionaries; flattens single-alternative
case-of-case; and turns `case tagToEnum# e` on Bool into a literal case. Banged let groups become
strict cases in dependency order. A strictness analysis (to a fixpoint, error paths forcing
nothing) evaluates strict arguments before the call instead of allocating thunks. Code generation
computes `Int#` arithmetic and comparisons inline on evaluated INT cells, and compares number and
character literal alternatives inline. Banged `where` bindings are now forced as GHC forces them.
All eight medians improve (geometric mean 2.36×): sumloop 2.451 → 0.160 s (GHC -O0 0.146 s),
fib 91.8 → 17.7 ms, mapops 274 → 112 ms, strings 3.72 → 2.38 s. Both corpora pass 234/234.
[Raw samples, conditions and graphs](bench/results/runtime-simplifier.md). All runtime targets
remain unmet; sumloop and fib are now within 1.15× of GHC -O0.
`HSC_TRACE_SIMPLIFY=<global>` prints a binding after each simplifier round.

Inline `Char#`, `Word#` and `tagToEnum#` code, shared static 0/1 and True/False results,
static small `Integer` literals and Int/Word/Integer conversion folds (PR #44): geometric mean
1.27×; strings 2.384 → 0.799 s, nqueens 110 → 81 ms, mapops 112 → 89 ms. fib (11.5 ms) and
sumloop (156 ms) now run faster than GHC -O0 (16.6 / 204 ms). ioref measured 4% slower, within
its run-to-run variation. [Raw samples, conditions and graphs](bench/results/runtime-char-word-integer.md).

`Integer` division uses Knuth's algorithm D instead of bit-at-a-time long division (`show` of a
large Integer divides repeatedly), and the scheduler's maps keyed by thread number or `MVar` key
use an integer hasher instead of SipHash (PR #45). Hosted, integer takes 81.7 → 31.6 ms; mvar's
instructions fall 328 → 257 M but its time barely moves (198.6 → 196.5 ms): its cost is OS
thread switches (a Haskell thread is an OS thread), which green threads would remove.
[Raw samples, conditions and graphs](bench/results/runtime-bigint-division.md).

A body's frame kept every slot alive until the body returned, so during `case m s of ..` the
slot holding `m` kept the chain of actions `forM_` builds reachable (ioref held ~19M words). A
call or evaluation in a scrutinee now clears the slots it mentions that nothing uses after it
(the alternatives, enclosing continuations and the code of join points they jump to). ioref
keeps 2,434 live words (PR #46). Hosted, ioref takes 633 → 166 ms; the others are unchanged
(a local strings gain did not reproduce). [Raw samples, conditions and graphs](bench/results/runtime-frame-liveness.md).

A local function generalised over class constraints but called with the same global
dictionaries everywhere (recursive calls may pass the parameter itself) is specialised: the
dictionaries are substituted and the parameters dropped, so its methods resolve and inline.
nqueens' `safe` compared through three dictionaries (PR #47): hosted, nqueens takes
133 → 100 ms (instructions 831 → 434 M). fib measured 19.1 → 23.9 ms on that noisy runner,
but its Core is identical and local runs show no change. [Raw samples, conditions and graphs](bench/results/runtime-local-specialisation.md). `HSC_DEBUG` builds give every generated body a sized symbol, so
cachegrind attributes time to Haskell bindings. The nqueens profile now shows ~32% in the
generic eval/apply/update runtime: unknown calls and laziness, the target of worker/wrapper and
wider specialisation.

A case of a variable that is already a value goes straight to its alternatives (no return
address or `hsc_eval`), and applying a partial application or over-applying a function stay in
the assembly apply path instead of calling into Rust. `HSC_DEBUG` builds also size the runtime's
own routines for profilers (PR #48). Instructions: nqueens 434 → 422 M, mapops 603 → 560 M,
others flat; the hosted run shows no wall-clock speedup (all within noise; integer and sumloop
measured 9% and 5% slower), so none is claimed. [Raw samples, conditions and graphs](bench/results/runtime-eval-apply-paths.md).
The nqueens profile is now `hsc_eval` 18%, `hsc_apply` 10%, updates 5%: lazy intermediate
structure that GHC's `-O1` fuses away, which fusion and worker/wrapper would address.

Strictness treats a pattern-match failure as bottom, as GHC does: the matcher's unreachable
`FAIL` default no longer erased every demand, so `length`'s accumulator (`lenAcc`) is strict and
`n + 1` is evaluated before the call instead of built as a thunk per element. A case scrutinee
with an inline fast path is computed in place, and a case's alternatives share frame slots.
Hosted (PR #49): strings 1.139 → 0.684 s, mapops 145 → 125 ms, the others within noise.
[Raw samples, conditions and graphs](bench/results/runtime-strictness-failures.md).

Frames can hold raw (unboxed) words: a body frame's size word is `words | raw << 32`, and the
collector, unwinder and `TOP` skip the raw words at the frame's low end. The result of an inline
`Int#`/`Word#`/`Char#` primitive whose every use is another inline primitive or a literal case
is kept raw: no cell is allocated and no kind is checked. Raw operands are boxed only for the
runtime's slow path (PR #50). Hosted: sumloop 184 → 122 ms, fib 20.1 → 14.5 ms, nqueens
79 → 67 ms, strings 584 → 536 ms. integer measured 23.1 → 30.1 ms, though its instructions fell
(181.1 → 179.9 M): taken as noise on a 23 ms run, to be rechecked by the next paired run.
[Raw samples, conditions and graphs](bench/results/runtime-raw-slots.md). Next: raw values
across calls (worker/wrapper), so loop arguments are not boxed.

Workers take unboxed arguments: a global function whose parameter's type is a monomorphic
`Int`/`Word`/`Char` (boxed or `#`; from its type scheme, dictionaries first), that is certainly
evaluated, used as a number in its own body (Core is untyped: neither an identity such as a
newtype's constructor nor a GADT-refined parameter may go raw), and only ever used raw (operand, literal case, a worker's raw argument, or its result,
boxed once) gets a second entry taking it raw. Every exact call uses the worker, evaluating a
raw parameter's argument first; partial applications and the function as a value use the boxed
entry. The worker's prologue moves raw arguments into raw slots before anything can collect.
Hosted (PR #51): sumloop 92.6 → 71.4 ms (GHC -O0 144 ms), fib 12.8 → 10.3 ms, strings
466 → 424 ms, the others within noise; integer measured 19.7 → 19.0 ms, so #50's apparent
integer regression was noise. [Raw samples, conditions and graphs](bench/results/runtime-workers.md).

Workers whose type gives a numeric result for a full application, with exactly that many
parameters and no constructor or function among their results, return the number raw in R1.
A raw result is taken as it is only by a raw case binder whose scrutinee is the call itself,
or by another such worker's tail call; elsewhere it is boxed after the return, and inside such
a worker every other result is evaluated and unboxed. Instructions: fib 39.7 → 30.8 M,
sumloop 1,023 → 962 M, others within 1%.

The work:
- Space: thunk retention, selector thunks in the GC, blackholing.
- Code: inline `Int#`, `Word#`, `Char#` and `tagToEnum#` operations, static small `Integer`
  literals, and folds of Int/Word/Integer conversions (done); next Double, then unboxed values in
  registers (worker/wrapper) and a direct calling convention.
- Optimiser: inlining, case-of-case, strictness and call by value, specialisation of local
  functions at constant dictionaries (done, first versions); next specialisation of global
  functions at known dictionaries, worker/wrapper, rewrite rules (for `-O1`'s list fusion).
- Strictness: the analysis's effect boundary (GHC's IO hack) recognises effectful primitives
  and applications matched against a pair; a helper answering only `State#` under a default
  alternative is not recognised, so a later demand could be evaluated before its effect. No
  program showing it is known; typed Core would let this test the `State#` type instead.
- Runtime: a generational GC, cheaper thunk allocation.

### 2. Large projects

Each built with its supported build system and compared with its GHC-built counterpart:
- hlint: its remaining dependencies, then its test suite and its output on real code.
- cabal-install.
- pandoc.
- GHC: build the compiler with hsc, then validate its test suite and the behaviour of
  programs it compiles against the reference GHC build.

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
