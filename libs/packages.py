#!/usr/bin/env python3
"""The packages of hsc's global database (libs/package.conf.d), and how they are vendored.

A development tool. `python3 libs/packages.py vendor` copies the sources of the boot packages
from ref/ (GHC's source tree and Hackage downloads) into libs/<name>; with no argument it writes
the package database and, in each import directory, `hsc-package.flags`: the flags its modules
are compiled with (relative paths in it are relative to that directory)."""
import hashlib, os, shutil, subprocess, sys

here = os.path.dirname(os.path.abspath(__file__))
ref = os.path.join(here, "..", "ref")

# name, version, origin (under ref/), copied entries, import dirs, flags, dependencies
PACKAGES = [
    ("rts", "1.0.2", None, [], [], [], []),
    ("ghc-prim", "0.13.0", None, [], ["ghc-prim"], [], ["rts"]),
    ("ghc-bignum", "1.3", None, [], ["ghc-bignum/src"], [], ["ghc-prim"]),
    ("ghc-internal", "9.1202.0", None, [], ["ghc-internal/src"], [], ["ghc-bignum", "ghc-prim", "rts"]),
    ("base", "4.21.0.0", None, [], ["base/src"], [], ["ghc-internal", "ghc-prim", "ghc-bignum"]),
    ("deepseq", "1.5.1.0", "hackage/deepseq-1.5.1.0", ["Control", "LICENSE"], ["deepseq"],
     ["-XHaskell2010"], ["base", "ghc-prim"]),
    ("array", "0.5.8.0", "hackage/array-0.5.8.0", ["Data", "LICENSE"], ["array"], ["-XHaskell2010"], ["base"]),
    ("pretty", "1.1.3.6", "hackage/pretty-1.1.3.6", ["src", "LICENSE"], ["pretty/src"], ["-XHaskell2010", "-XCPP", "-XBangPatterns", "-XDeriveGeneric"],
     ["base", "deepseq", "ghc-prim"]),
    ("ghc-boot-th", "9.12.2", "src/libraries/ghc-boot-th", ["GHC", "LICENSE"], ["ghc-boot-th"],
     ["-XHaskell2010", "-XNoImplicitPrelude"], ["base", "ghc-prim", "pretty", "ghc-internal"]),
    ("template-haskell", "2.23.0.0", "hackage/template-haskell-2.23.0.0", ["Language", "vendored-filepath", "LICENSE"],
     ["template-haskell/vendored-filepath", "template-haskell"], ["-XHaskell2010", "-XImplicitPrelude"],
     ["base", "ghc-boot-th", "ghc-internal"]),
    ("containers", "0.7", "hackage/containers-0.7", ["src", "include", "LICENSE"], ["containers/src"],
     ["-XHaskell2010", "-I../include"], ["base", "array", "deepseq", "template-haskell"]),
    ("transformers", "0.6.1.2", "hackage/transformers-0.6.1.2", ["Control", "Data", "LICENSE"], ["transformers"],
     ["-XHaskell2010"], ["base"]),
    ("mtl", "2.3.1", "hackage/mtl-2.3.1", ["Control", "LICENSE"], ["mtl"], ["-XHaskell2010"], ["base", "transformers"]),
    ("stm", "2.5.3.1", "hackage/stm-2.5.3.1", ["Control", "LICENSE"], ["stm"], ["-XHaskell2010"], ["base", "array"]),
    # The pure-haskell flag: the C code of bytestring is replaced by its Haskell version.
    ("bytestring", "0.12.2.0", "hackage/bytestring-0.12.2.0", ["Data", "include", "LICENSE"], ["bytestring"],
     ['-XHaskell2010', '-XBangPatterns', '-XDeriveDataTypeable', '-XDeriveGeneric', '-XDeriveLift', '-XFlexibleContexts', '-XFlexibleInstances', '-XLambdaCase', '-XMagicHash', '-XMultiWayIf', '-XNamedFieldPuns', '-XPatternSynonyms', '-XRankNTypes', '-XScopedTypeVariables', '-XStandaloneDeriving', '-XTupleSections', '-XTypeApplications', '-XTypeOperators', '-XUnboxedTuples', '-DPURE_HASKELL=1', '-Iinclude'], ["base", "deepseq", "ghc-prim", "template-haskell"]),
    ("binary", "0.8.9.3", "hackage/binary-0.8.9.3", ["src", "LICENSE"], ["binary/src"], ["-XHaskell2010"],
     ["base", "bytestring", "containers", "array"]),
    # Without simdutf (C++): the C routines of cbits/.
    ("text", "2.1.2", "hackage/text-2.1.2", ["src", "cbits", "LICENSE"], ["text/src"],
     ["-XHaskell2010", "-XNondecreasingIndentation", "-c-source=../cbits/is_ascii.c", "-c-source=../cbits/reverse.c",
      "-c-source=../cbits/utils.c", "-c-source=../cbits/measure_off.c"],
     ["array", "base", "binary", "bytestring", "deepseq", "ghc-prim", "template-haskell"]),
    ("exceptions", "0.10.9", "hackage/exceptions-0.10.9", ["src", "LICENSE"], ["exceptions/src"], ["-XHaskell2010"],
     ["base", "mtl", "stm", "template-haskell", "transformers"]),
    ("os-string", "2.0.7", "hackage/os-string-2.0.7", ["System", "LICENSE"], ["os-string"], ["-XHaskell2010"],
     ["base", "bytestring", "deepseq", "exceptions", "template-haskell"]),
    ("filepath", "1.5.4.0", "hackage/filepath-1.5.4.0", ["System", "LICENSE"], ["filepath"], ["-XHaskell2010"],
     ["base", "bytestring", "deepseq", "exceptions", "os-string", "template-haskell"]),
    ("parsec", "3.1.18.0", "hackage/parsec-3.1.18.0", ["src", "LICENSE"], ["parsec/src"], ["-XHaskell2010"],
     ["base", "bytestring", "mtl", "text"]),
    ("time", "1.14", "hackage/time-1.14", ["lib", "LICENSE"], ["time/lib"],
     ["-XHaskell2010", "-XRank2Types", "-XDeriveDataTypeable", "-XDeriveLift", "-XDeriveGeneric", "-XStandaloneDeriving",
      "-XPatternSynonyms", "-XViewPatterns", "-Iinclude", "-c-source=cbits/HsTime.c"],
     ["base", "deepseq", "template-haskell"]),
    ("unix", "2.8.6.0", "hackage/unix-2.8.6.0", ["System", "include", "cbits", "LICENSE"], ["unix"],
     ["-XHaskell2010", "-Iinclude", "-c-source=cbits/HsUnix.c", "-c-source=cbits/execvpe.c"],
     ["base", "bytestring", "filepath", "os-string", "time"]),
    ("file-io", "0.1.5", "hackage/file-io-0.1.5", ["System", "posix", "LICENSE"], ["file-io/posix", "file-io"],
     ["-XHaskell2010"], ["base", "bytestring", "deepseq", "filepath", "os-string", "unix"]),
    ("directory", "1.3.9.0", "hackage/directory-1.3.9.0", ["System", "LICENSE"], ["directory"],
     ["-XHaskell2010", "-I."], ["base", "filepath", "os-string", "time", "unix", "file-io"]),
    ("process", "1.6.25.0", "hackage/process-1.6.25.0", ["System", "include", "cbits", "LICENSE"], ["process"],
     ["-XHaskell2010", "-Iinclude", "-c-source=cbits/posix/runProcess.c", "-c-source=cbits/posix/fork_exec.c",
      "-c-source=cbits/posix/posix_spawn.c", "-c-source=cbits/posix/find_executable.c"],
     ["base", "deepseq", "directory", "filepath", "unix"]),
]

# Config headers made by tools/confprobe.py in place of the packages' configure scripts:
# (package, template under ref/, output under libs/<package>/).
CONFIGS = [
    ("time", "hackage/time-1.14/lib/include/HsTimeConfig.h.in", "lib/include/HsTimeConfig.h"),
    ("unix", "hackage/unix-2.8.6.0/include/HsUnixConfig.h.in", "include/HsUnixConfig.h"),
    ("directory", "hackage/directory-1.3.9.0/HsDirectoryConfig.h.in", "HsDirectoryConfig.h"),
    ("process", "hackage/process-1.6.25.0/include/HsProcessConfig.h.in", "include/HsProcessConfig.h"),
]

def vendor():
    for name, ver, origin, entries, *_ in PACKAGES:
        if not origin:
            continue
        dst = os.path.join(here, name)
        if os.path.isdir(dst):
            continue
        src = os.path.join(ref, origin)
        os.makedirs(dst)
        for e in entries:
            s = os.path.join(src, e)
            (shutil.copytree if os.path.isdir(s) else shutil.copy)(s, os.path.join(dst, e))
        print("vendored", name, ver)
    probe = os.path.join(here, "..", "tools", "confprobe.py")
    versions = {name: ver for name, ver, *_ in PACKAGES}
    for name, template, out in CONFIGS:
        target = os.path.join(here, name, out)
        if not os.path.exists(target):
            subprocess.run([sys.executable, probe, os.path.join(ref, template), target, name, versions[name]], check=True)
            print("configured", name)

def modules(d):
    out = []
    root = os.path.join(here, d)
    for dp, dns, fns in os.walk(root):
        dns[:] = sorted(x for x in dns if x[0].isupper())
        for f in fns:
            for ext in (".hs", ".hsc"):
                if f.endswith(ext) and f[0].isupper():
                    out.append(os.path.relpath(os.path.join(dp, f[:-len(ext)]), root).replace(os.sep, "."))
    return sorted(set(out))

def write_db():
    ids = {name: f"{name}-{ver}" for name, ver, *_ in PACKAGES}
    db = os.path.join(here, "package.conf.d")
    os.makedirs(db, exist_ok=True)
    for f in os.listdir(db):
        if f.endswith(".conf"):
            os.remove(os.path.join(db, f))
    for name, ver, origin, entries, dirs, flags, deps in PACKAGES:
        pid = ids[name]
        lines = [f"name: {name}", f"version: {ver}", "visibility: public", f"id: {pid}", f"key: {pid}",
                 "license: BSD-3-Clause", "abi: " + hashlib.md5(pid.encode()).hexdigest(), "exposed: True"]
        mods = sorted(set(m for d in dirs for m in modules(d)))
        if mods:
            lines.append("exposed-modules:")
            for i in range(0, len(mods), 4):
                lines.append("    " + ", ".join(mods[i:i + 4]) + ("," if i + 4 < len(mods) else ""))
        if dirs:
            lines.append("import-dirs: " + " ".join("${pkgroot}/" + d for d in dirs))
        lines.append("depends: " + " ".join(ids[d] for d in deps))
        with open(os.path.join(db, pid + ".conf"), "w") as fh:
            fh.write("\n".join(lines) + "\n")
        for d in dirs:
            path = os.path.join(here, d, "hsc-package.flags")
            if flags:
                with open(path, "w") as fh:
                    fh.write("\n".join(flags) + "\n")
            elif os.path.exists(path):
                os.remove(path)
    print("wrote", len(PACKAGES), "packages to", db)

if __name__ == "__main__":
    if sys.argv[1:] == ["vendor"]:
        vendor()
    write_db()
