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
    """The package names the build-depends fields of a .cabal file's libraries name, with those of
    the `common` stanzas they import (test suites, benchmarks and executables left out: their test
    frameworks would make every package use every other)."""
    # Comments are whole lines in .cabal files.
    lines = [l for l in cabal_text.splitlines() if not l.lstrip().startswith("--")]
    sections = []  # (kind, name, deps, imports)
    i = 0
    while i < len(lines):
        line = lines[i]
        header = re.match(r"([A-Za-z-]+)\s*(\S*)", line)
        if header and line and not line[0].isspace():
            sections.append((header.group(1).lower(), header.group(2), set(), []))
            i += 1
            continue
        field = re.match(r"(\s*)(build-depends|import)\s*:(.*)", line, re.IGNORECASE)
        if not field or not sections:
            i += 1
            continue
        indent = len(field.group(1))
        text = [field.group(3)]
        i += 1
        # Continuation lines are indented more than the field name.
        while i < len(lines) and (not lines[i].strip() or len(lines[i]) - len(lines[i].lstrip()) > indent):
            text.append(lines[i])
            i += 1
        joined = " ".join(text)
        kind = field.group(2).lower()
        for item in joined.split(","):
            n = re.match(r"\s*([A-Za-z0-9][A-Za-z0-9-]*)", item)
            if n:
                if kind == "import":
                    sections[-1][3].append(n.group(1))
                else:
                    sections[-1][2].add(n.group(1))
    common = {name: (deps, imports) for k, name, deps, imports in sections if k == "common"}

    def with_imports(deps, imports, seen):
        out = set(deps)
        for name in imports:
            if name in common and name not in seen:
                seen.add(name)
                out |= with_imports(*common[name], seen)
        return out

    result = set()
    for k, _, deps, imports in sections:
        if k == "library":
            result |= with_imports(deps, imports, set())
    return result


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
