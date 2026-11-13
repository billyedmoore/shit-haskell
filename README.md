# hsc

A Haskell compiler written from scratch in Rust, aiming to be a drop-in replacement for GHC 9.12.2.

## Why

This project tests what AI systems can do with large rewrites that would otherwise be out of
reach, by hill-climbing: an AI agent (Claude) works in iteration loops, each time finding a way
hsc differs from GHC, fixing it, and checking the fix against GHC, so that the compiler climbs
steadily towards GHC parity. A compiler for a real language is a demanding target for this:
parser, type checker, optimiser, code generator and runtime all have to work together, and real
programs use a vast surface of the language and its libraries.

hsc was written almost entirely by Claude, working autonomously over long sessions. GHC is the
measure of progress and the test oracle: every behaviour is checked by compiling the same program
with both compilers and comparing the results. The question is how far the climb can go, from toy
programs to GHC's own boot libraries' test suites to large real-world projects such as ShellCheck.

## What it is

- One Rust crate with no dependencies outside the standard library (`crates/hsc`).
- GHC's command line, package databases and `cabal build -w <shim>/ghc` support, so cabal can drive it.
- Two back ends: an interpreter (used for `runghc`, `ghc -e` and Template Haskell splices), and a
  native x86-64 back end with its own Rust runtime (copying GC, threads, STM, FFI). Native code is
  assembled and linked with the system `gcc`.
- GHC's own `base`, `ghc-prim`, `ghc-internal` and the other boot libraries, compiled from their
  sources (`libs/`).

## Status

Work in progress. Programs of the test corpus give the same output as GHC on both back ends; most
boot packages pass their own test suites. ShellCheck 0.11 and its ~45 dependencies (aeson, vector,
text, ...) build, and its test suite nearly passes. Generated code is far slower than GHC's.
[PROGRESS.md](PROGRESS.md) has the details by milestone, and the
[issues](https://github.com/billyedmoore/shit-haskell/issues) track what is next.

## Building and using

```sh
cargo build --release -p hsc
target/release/hsc Main.hs -o main            # GHC-style command line
target/release/hsc --native Main.hs -o main   # native code
target/release/hsc --runghc Main.hs           # interpret
target/release/hsc --install-ghc-shims DIR    # ghc, ghc-pkg, hsc2hs shims for cabal -w DIR/ghc
```

## Testing

`tests/` holds programs that must compile and run (`tests/run`) or be rejected (`tests/fail`).
`difftest` compiles each with hsc and with GHC and compares the outputs:

```sh
cargo build -p difftest
./target/debug/difftest --hsc target/release/hsc --timeout 180 --work target/difftest tests/
```

`tools/pkgtest.sh <package>` runs a boot package's own test suites under cabal with hsc.
