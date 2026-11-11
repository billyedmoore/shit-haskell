#!/bin/sh
# Builds every cabal project in projects/ twice, with GHC and with hsc (through `ghc`/`ghc-pkg`
# links made by `hsc --install-ghc-shims`), runs each executable and compares their outputs.
# Usage: projects/check.sh [path/to/hsc]
set -u
here=$(cd "$(dirname "$0")" && pwd)
hsc=${1:-$here/../target/release/hsc}
work=${TMPDIR:-/tmp}/hsc-projects-$$
mkdir -p "$work"
"$hsc" --install-ghc-shims "$work/shim" || exit 2
pass=0; total=0
for p in "$here"/*/; do
    name=$(basename "$p")
    for side in ghc hsc; do
        rm -rf "$work/$side/$name"; mkdir -p "$work/$side"; cp -r "$p" "$work/$side/$name"
        w=ghc; [ "$side" = hsc ] && w="$work/shim/ghc"
        (cd "$work/$side/$name" && cabal build --offline -v0 -w "$w" all >"$work/$side.$name.build" 2>&1 &&
         for exe in $(cabal list-bin --offline -v0 -w "$w" all 2>/dev/null); do "$exe"; echo "exit $?"; done) >"$work/$side.$name.out" 2>&1
    done
    total=$((total + 1))
    if cmp -s "$work/ghc.$name.out" "$work/hsc.$name.out"; then
        pass=$((pass + 1)); echo "PASS    $name"
    else
        echo "FAIL    $name"; diff "$work/ghc.$name.out" "$work/hsc.$name.out" | head -20
        tail -5 "$work/hsc.$name.build"
    fi
done
echo "TOTAL   $pass/$total projects"
rm -rf "$work"
[ "$pass" = "$total" ]
