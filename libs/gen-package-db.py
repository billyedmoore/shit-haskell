#!/usr/bin/env python3
"""Writes libs/package.conf.d: the global package database of hsc for the vendored packages.
A development tool: run it again when a package's modules change."""
import hashlib, os, sys
here = os.path.dirname(os.path.abspath(__file__))
pkgs = [
    # name, version, source dir (relative to libs/), dependencies
    ("rts", "1.0.2", None, []),
    ("ghc-prim", "0.13.0", "ghc-prim", ["rts-1.0.2"]),
    ("ghc-bignum", "1.3", "ghc-bignum/src", ["ghc-prim-0.13.0"]),
    ("ghc-internal", "9.1202.0", "ghc-internal/src", ["ghc-bignum-1.3", "ghc-prim-0.13.0", "rts-1.0.2"]),
    ("base", "4.21.0.0", "base/src", ["ghc-internal-9.1202.0", "ghc-prim-0.13.0", "ghc-bignum-1.3"]),
]
def modules(d):
    out = []
    root = os.path.join(here, d)
    for dp, dns, fns in os.walk(root):
        dns[:] = [x for x in dns if x[0].isupper()]
        for f in fns:
            if f.endswith(".hs") and f[0].isupper():
                rel = os.path.relpath(os.path.join(dp, f[:-3]), root)
                out.append(rel.replace(os.sep, "."))
    return sorted(out)
db = os.path.join(here, "package.conf.d")
os.makedirs(db, exist_ok=True)
for name, ver, src, deps in pkgs:
    pid = f"{name}-{ver}"
    lines = [f"name: {name}", f"version: {ver}", "visibility: public", f"id: {pid}", f"key: {pid}",
             "license: BSD-3-Clause", "abi: " + hashlib.md5(pid.encode()).hexdigest(), "exposed: True"]
    if src:
        mods = modules(src)
        lines.append("exposed-modules:")
        for i in range(0, len(mods), 4):
            lines.append("    " + ", ".join(mods[i:i + 4]) + ("," if i + 4 < len(mods) else ""))
        lines.append("import-dirs: ${pkgroot}/" + src)
    lines.append("depends: " + " ".join(deps))
    with open(os.path.join(db, pid + ".conf"), "w") as fh:
        fh.write("\n".join(lines) + "\n")
print("wrote", len(pkgs), "packages to", db)
