#!/bin/sh
# Runs a package's own test suites with cabal and hsc (milestone 7).
#
#   tools/pkgtest.sh [--installed] <package-dir-under-ref/hackage> [cabal.project line ...]
#
# With --installed, the package's .cabal file keeps only its test suites (tools/testonly.py), so
# they build against the installed library (hsc's, from libs/): for packages whose tests cannot be
# solved in-tree, such as bytestring's (QuickCheck depends on it through random).
#
# The package is copied to target/pkgtest/<name> (afresh each time), with a cabal.project that
# enables its tests and lets it build against hsc's base; extra arguments are appended to the
# project file (for example 'constraints: tasty -unix'). The compiler is target/alt/release/hsc
# unless HSC_BIN names another; ghc, ghc-pkg and hsc2hs shims for it go in target/pkgtest/shims.
# Programs are built natively. The log is target/pkgtest/<name>.log; the summary is printed.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
installed=no
if [ "$1" = "--installed" ]; then installed=yes; shift; fi
pkg=$1
shift
hsc=${HSC_BIN:-$root/target/alt/release/hsc}
work=$root/target/pkgtest
mkdir -p "$work"
"$hsc" --install-ghc-shims "$work/shims"
rm -rf "$work/$pkg"
cp -r "$root/ref/hackage/$pkg" "$work/$pkg"
if [ $installed = yes ]; then
    for c in "$work/$pkg"/*.cabal; do python3 "$root/tools/testonly.py" "$c"; done
fi
{
    echo "packages: ."
    echo "tests: True"
    echo "allow-newer: base, ghc-prim"
    for line in "$@"; do echo "$line"; done
} > "$work/$pkg/cabal.project"
cd "$work/$pkg"
HSC_NATIVE=1 timeout "${PKGTEST_TIMEOUT:-2700}" cabal test -v1 -w "$work/shims/ghc" \
    --test-options="--timeout=${PKGTEST_CASE_TIMEOUT:-120s}" > "$work/$pkg.log" 2>&1 || true
grep -E "Test suite .*: (PASS|FAIL)|Failed to build|^Error" "$work/$pkg.log" || tail -20 "$work/$pkg.log"
