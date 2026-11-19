#!/usr/bin/env python3
"""Ranks a Stackage snapshot's packages by use: how many of the snapshot's packages depend on each
(directly or not), read from the local Hackage index. Prints `count name` lines, most used first.

    tools/sweep_rank.py <snapshot cabal.config> [index tar]

The snapshot is Stackage's `cabal.config` (https://www.stackage.org/nightly/cabal.config); the
index defaults to ~/.cabal/packages/hackage.haskell.org/01-index.tar. Each package's latest
version in the index (its last revision) gives its dependencies; a package missing from the
index is left out.
"""
import os
import re
import sys
import tarfile


def snapshot_names(path):
    names = []
    for line in open(path, encoding="utf-8"):
        m = re.match(r"\s*(?:constraints:)?\s*([A-Za-z0-9][A-Za-z0-9-]*)\s*(?:==|installed)", line)
        if m:
            names.append(m.group(1))
    return names


def version_key(v):
    return [int(x) if x.isdigit() else 0 for x in v.split(".")]


def latest_cabal_files(index, wanted):
    """The text of each wanted package's latest .cabal file (later revisions replace earlier)."""
    best = {}
    with tarfile.open(index) as tar:
        for member in tar:
            parts = member.name.split("/")
            if len(parts) != 3 or not parts[2].endswith(".cabal") or parts[0] not in wanted:
                continue
            name, version = parts[0], parts[1]
            prev = best.get(name)
            if prev is None or version_key(version) >= version_key(prev[0]):
                f = tar.extractfile(member)
                if f is not None:
                    best[name] = (version, f.read().decode("utf-8", "replace"))
    return best


def dependencies(cabal_text):
    """The package names the build-depends fields of a .cabal file's libraries name (test suites,
    benchmarks and executables left out: their test frameworks would make every package use
    every other)."""
    deps = set()
    lines = cabal_text.splitlines()
    in_library = False
    i = 0
    while i < len(lines):
        section = re.match(r"([A-Za-z-]+)(\s|$)", lines[i])
        if section and not lines[i][0].isspace():
            in_library = section.group(1).lower() == "library"
        m = re.match(r"(\s*)build-depends\s*:(.*)", lines[i], re.IGNORECASE)
        if not m or not in_library:
            i += 1
            continue
        indent = len(m.group(1))
        field = [m.group(2)]
        i += 1
        # Continuation lines are indented more than the field name.
        while i < len(lines) and (not lines[i].strip() or len(lines[i]) - len(lines[i].lstrip()) > indent):
            field.append(lines[i])
            i += 1
        text = re.sub(r"--.*", "", " ".join(field))
        for item in text.split(","):
            n = re.match(r"\s*([A-Za-z0-9][A-Za-z0-9-]*)", item)
            if n:
                deps.add(n.group(1))
    return deps


def main():
    snapshot = sys.argv[1]
    index = sys.argv[2] if len(sys.argv) > 2 else os.path.expanduser("~/.cabal/packages/hackage.haskell.org/01-index.tar")
    names = snapshot_names(snapshot)
    wanted = set(names)
    files = latest_cabal_files(index, wanted)
    direct = {n: dependencies(t) & wanted for n, (_, t) in files.items()}
    # Users of each package: the packages that reach it through their dependencies.
    users = {n: 0 for n in files}
    for n in files:
        seen = set()
        stack = list(direct.get(n, ()))
        while stack:
            d = stack.pop()
            if d in seen or d == n:
                continue
            seen.add(d)
            stack.extend(direct.get(d, ()))
        for d in seen:
            if d in users:
                users[d] += 1
    for n in sorted(users, key=lambda n: (-users[n], n)):
        print(users[n], n, files[n][0])


if __name__ == "__main__":
    main()
