# hsc

A Haskell compiler written in Rust, targeting GHC 9.12.2 compatibility.

## Build

```sh
cargo build --release -p hsc
```

## Usage

```sh
target/release/hsc Main.hs -o main
target/release/hsc --native Main.hs -o main
target/release/hsc --native -O2 Main.hs -o main   # -O0, -O1 (default), -O2 as GHC's
target/release/hsc --runghc Main.hs
target/release/hsc -fno-code Main.hs
target/release/hsc --install-ghc-shims DIR
```

## Test

```sh
cargo test -p hsc --lib
cargo build -p difftest
./target/debug/difftest --hsc target/release/hsc --timeout 180 --work target/difftest tests/
```

The differential tester compares acceptance with GHC. A test can also supply a
`.diag` sidecar with one required fragment of hsc's diagnostics per line.
Use `-v` to see both compilers' diagnostics when a case fails.
