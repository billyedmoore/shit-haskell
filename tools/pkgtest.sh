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
# Programs are built natively. The log is target/pkgtest/<name>.log; the summary is printed. The
# copy is deleted afterwards unless PKGTEST_KEEP is set.
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
# PKGTEST_TEST_OPTIONS passes options to the test programs (tasty's --timeout=120s, say); not
# every test driver accepts tasty's options.
HSC_NATIVE=1 timeout "${PKGTEST_TIMEOUT:-2700}" cabal test -v1 -w "$work/shims/ghc" \
    ${PKGTEST_TEST_OPTIONS:+--test-options="$PKGTEST_TEST_OPTIONS"} > "$work/$pkg.log" 2>&1 || true
grep -a -E "Test suite .*: (PASS|FAIL)|Failed to build|^Error" "$work/$pkg.log" || tail -20 "$work/$pkg.log"
# The copy (with its build products) is removed to save space; PKGTEST_KEEP=1 keeps it, to
# rebuild in it with `cabal build` while fixing a failure.
if [ -z "${PKGTEST_KEEP:-}" ]; then
    cd "$root"
    rm -rf "$work/$pkg"
fi
