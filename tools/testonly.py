#!/usr/bin/env python3
"""Rewrites a package's .cabal file to hold only its test suites, under the name <name>-tests, so
that they build against the installed library (a development tool for tools/pkgtest.sh).

    testonly.py <file.cabal>

The library and benchmark stanzas are dropped (a package's tests that depend on a package
depending on it, such as bytestring's on QuickCheck through random, cannot be solved in-tree);
everything else (flags, common stanzas, test suites) is kept."""
import re, sys

path = sys.argv[1]
lines = open(path).read().split("\n")
out = []
skipping = False
for line in lines:
    top = line and not line[0].isspace()
    if top:
        word = line.split()[0].lower()
        skipping = word in ("library", "benchmark", "foreign-library") or (word == "executable")
        m = re.match(r"(?i)^name\s*:\s*(\S+)", line)
        if m:
            line = "name: " + m.group(1) + "-tests"
    if not skipping:
        out.append(line)
open(path, "w").write("\n".join(out))
