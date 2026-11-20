#!/usr/bin/env python3
"""The Hackage sweep: builds the most-used packages' libraries with hsc and with GHC, and ranks
hsc's failures by how many packages each blocks.

    tools/sweep.py run <rank file> <count> [--only ghc|hsc]   build the top <count> packages
    tools/sweep.py run <rank file> <count> --retry            build again those hsc failed
    tools/sweep.py report                                      print the ranked failures

The rank file is tools/sweep_rank.py's output. Each package is built from Hackage, with its
dependencies, as an extra package of a scratch project in target/sweep/work (cabal's solver picks
the versions from the local index; GHC's and hsc's stores are apart). Results go to
target/sweep/results.jsonl, one line per package and compiler, and a run resumes after the last
one recorded. hsc is target/release/hsc unless HSC_BIN names another; SWEEP_TIMEOUT is the
seconds a build may take (default 1800).
"""
import json
import os
import re
import shutil
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WORK = os.path.join(ROOT, "target", "sweep")
RESULTS = os.path.join(WORK, "results.jsonl")

# Packages the compiler provides, and packages for another system: not built.
SKIP = {
    "ghc-prim", "ghc-internal", "base", "ghc-bignum", "integer-gmp", "template-haskell",
    "ghc", "ghc-boot", "ghc-boot-th", "ghc-heap", "ghci", "rts", "Win32", "ghc-experimental",
    "ghc-toolchain", "ghc-platform", "integer-simple",
}


def shims():
    hsc = os.environ.get("HSC_BIN", os.path.join(ROOT, "target", "release", "hsc"))
    d = os.path.join(WORK, "shims")
    subprocess.run([hsc, "--install-ghc-shims", d], check=True)
    return os.path.join(d, "ghc")


def done(retry=False):
    """The (package, compiler) builds recorded; with `retry`, hsc's failures are not (the last
    record of a build counts)."""
    last = {}
    if os.path.exists(RESULTS):
        for line in open(RESULTS, encoding="utf-8"):
            r = json.loads(line)
            last[(r["package"], r["compiler"])] = r
    return {k for k, r in last.items() if not (retry and k[1] == "hsc" and not r["ok"])}


FAILED = re.compile(r"Failed to build ([A-Za-z0-9][A-Za-z0-9-]*-\d+(?:\.\d+)*)")


def first_error(log):
    """The package that failed and the first error's message (a few lines), from a cabal log."""
    failed = None
    m = FAILED.search(log)
    if m:
        failed = m.group(1)
    elif "Could not resolve dependencies" in log or "cannot satisfy" in log:
        return "(solver)", "could not resolve dependencies"
    lines = log.splitlines()
    for i, line in enumerate(lines):
        if re.search(r"(:\d+:\d+|:\(\d+,\d+\)-\(\d+,\d+\)).*: error:|: error: \[|^\S+: error:", line):
            msg = [lines[i].split("error:", 1)[1].strip()]
            for l in lines[i + 1:i + 4]:
                # The message ends where cabal's own lines (or the next error) begin.
                if not l.strip() or re.match(r"\s*(Error: \[|Failed to build|\S+:\d+:\d+)", l):
                    break
                msg.append(l.strip())
            return failed, " ".join(m for m in msg if m)[:300]
    for line in lines:
        if re.search(r"\berror\b|unknown primitive|panicked|Uncaught exception", line, re.IGNORECASE) and not re.match(
            r"\s*(Error: \[Cabal-|Compiling|Building|Preprocessing|Configuring)", line
        ):
            return failed, line.strip()[:300]
    return failed, (lines[-1].strip()[:300] if lines else "")


def building(log):
    """The package being built last in a log (for a build that ran out of time)."""
    names = re.findall(r"(?:Building|Configuring|Preprocessing) library (?:'[^']*' )?for ([A-Za-z0-9-]+-\d+(?:\.\d+)*)", log)
    return names[-1] if names else None


def build(pkg, compiler, ghc_shim, timeout):
    d = os.path.join(WORK, "work", f"{pkg}-{compiler}")
    shutil.rmtree(d, ignore_errors=True)
    os.makedirs(d)
    with open(os.path.join(d, "sweep-dummy.cabal"), "w") as f:
        f.write("cabal-version: 2.4\nname: sweep-dummy\nversion: 0\nlibrary\n  build-depends: base\n"
                "  default-language: Haskell2010\n")
    with open(os.path.join(d, "cabal.project"), "w") as f:
        f.write(f"packages: .\nextra-packages: {pkg}\nallow-newer: base, ghc-prim, template-haskell\n")
    env = dict(os.environ)
    cmd = ["cabal", "build", "-j1", pkg, "-w", ghc_shim if compiler == "hsc" else "ghc"]
    if compiler == "hsc":
        env["HSC_NATIVE"] = "1"
    start = time.time()
    # Its own process group: on a timeout the compilers cabal started stop with it.
    p = subprocess.Popen(cmd, cwd=d, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, start_new_session=True)
    try:
        out, _ = p.communicate(timeout=timeout)
        log, code = out.decode("utf-8", "replace"), p.returncode
    except subprocess.TimeoutExpired:
        os.killpg(p.pid, 9)
        out, _ = p.communicate()
        log, code = out.decode("utf-8", "replace"), "timeout"
    secs = round(time.time() - start)
    with open(os.path.join(WORK, "logs", f"{pkg}-{compiler}.log"), "w", encoding="utf-8") as f:
        f.write(log)
    shutil.rmtree(d, ignore_errors=True)
    r = {"package": pkg, "compiler": compiler, "ok": code == 0, "seconds": secs}
    if code == "timeout":
        r.update(failed=building(log) or pkg, error="timeout")
    elif code != 0:
        if "Unknown target" in log or "does not have any libraries" in log:
            r.update(failed=None, error="no library")
        else:
            failed, err = first_error(log)
            r.update(failed=failed, error=err)
    return r


def run(rank_file, count, only, retry=False):
    os.makedirs(os.path.join(WORK, "logs"), exist_ok=True)
    ghc_shim = shims()
    timeout = int(os.environ.get("SWEEP_TIMEOUT", "1800"))
    pkgs = []
    for line in open(rank_file, encoding="utf-8"):
        parts = line.split()
        if len(parts) >= 2 and parts[1] not in SKIP:
            pkgs.append(parts[1])
    seen = done(retry)
    for pkg in pkgs[:count]:
        for compiler in ["ghc", "hsc"]:
            if (only and compiler != only) or (pkg, compiler) in seen:
                continue
            r = build(pkg, compiler, ghc_shim, timeout)
            with open(RESULTS, "a", encoding="utf-8") as f:
                f.write(json.dumps(r) + "\n")
            print(json.dumps(r), flush=True)


def normalise(err):
    err = re.sub(r"[‘'`][^’'`]*[’'`]", "‘_’", err)
    err = re.sub(r"\b[a-z]\d+\b|\bt\d+\b|\bk\d+\b", "_", err)
    err = re.sub(r"\d+", "N", err)
    return err[:160]


def report():
    rs = {}
    for line in open(RESULTS, encoding="utf-8"):
        r = json.loads(line)
        rs[(r["package"], r["compiler"])] = r
    pkgs = sorted({p for p, _ in rs})
    ghc_ok = {p for p in pkgs if rs.get((p, "ghc"), {}).get("ok")}
    hsc_ok = {p for p in ghc_ok if rs.get((p, "hsc"), {}).get("ok")}
    tried = {p for p in ghc_ok if (p, "hsc") in rs}
    print(f"packages: {len(pkgs)}; GHC builds {len(ghc_ok)}; hsc builds {len(hsc_ok)} of the {len(tried)} GHC builds it tried\n")
    groups = {}
    for p in sorted(tried - hsc_ok):
        r = rs[(p, "hsc")]
        groups.setdefault(r.get("failed") or p, []).append(p)
    print("| blocks | failing package | first error | blocked |")
    print("|---:|---|---|---|")
    for failed, blocked in sorted(groups.items(), key=lambda kv: (-len(kv[1]), kv[0])):
        own = rs.get((blocked[0], "hsc"), {})
        err = own.get("error", "")[:140].replace("|", "\\|")
        print(f"| {len(blocked)} | {failed} | {err} | {', '.join(blocked[:6])}{' …' if len(blocked) > 6 else ''} |")


def main():
    if sys.argv[1] == "run":
        only = None
        if "--only" in sys.argv:
            only = sys.argv[sys.argv.index("--only") + 1]
        run(sys.argv[2], int(sys.argv[3]), only, "--retry" in sys.argv)
    else:
        report()


if __name__ == "__main__":
    main()
