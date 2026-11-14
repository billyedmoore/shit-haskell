#!/usr/bin/env python3
"""Measure cold front-end instance lookup against an earlier hsc, current hsc, and GHC.

Example: python3 tools/compilebench.py --before target/before/release/hsc \
    --after target/release/hsc --runs 3
The generated source, raw measurements and Markdown charts stay in --work.
NoImplicitPrelude isolates instance lookup from boot-library startup.
Run without other compiler/test jobs competing for CPU.
"""

import argparse
import json
import os
from pathlib import Path
import statistics
import subprocess
import time


def source():
    lines = [
        "{-# LANGUAGE FlexibleInstances, NoImplicitPrelude #-}",
        "module InstanceLookup where",
        "data Result = Result",
        "data Proxy a = Proxy",
        "class Tag a where tag :: Proxy a -> Result",
    ]
    # Many candidates in one class expose repeated zonking of a large wanted.
    for i in range(100):
        lines += [f"data D{i} a = D{i}", f"instance Tag (D{i} a) where tag _ = Result"]
    # Unrelated classes expose scans through the whole instance environment.
    for i in range(100):
        lines += [f"class C{i} a where c{i} :: a -> Result"]
        for j in range(10):
            lines += [f"instance C{i} (D{j} a) where c{i} _ = Result"]
    lines += ["type Deep = " + "D0 (" * 128 + "Result" + ")" * 128]
    for i in range(1000):
        lines += [f"f{i} :: Result", f"f{i} = tag (Proxy :: Proxy (D99 Deep))"]
    return "\n".join(lines) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", required=True)
    parser.add_argument("--after", required=True)
    parser.add_argument("--ghc", default="ghc")
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--work", type=Path, default=Path("target/compilebench"))
    args = parser.parse_args()
    if args.runs < 1:
        parser.error("--runs must be positive")
    work = args.work.resolve()
    work.mkdir(parents=True, exist_ok=True)
    hs = work / "InstanceLookup.hs"
    hs.write_text(source())
    env = dict(os.environ, HSC_NO_CACHE="1", XDG_CACHE_HOME=str(work / "cache"))
    # Environment switches must not select a different backend or produce profiling noise.
    for key in ("HSC_NATIVE", "HSC_TIME", "HSC_TIME_TC", "HSC_PROGRESS", "HSC_DEBUG_CACHE"):
        env.pop(key, None)
    compilers = [
        ("Before", args.before, []), ("After", args.after, []),
        ("GHC -O0", args.ghc, ["-O0", "-fno-code"]),
        ("GHC -O1", args.ghc, ["-O1", "-fno-code"]),
    ]
    results = {name: [] for name, _, _ in compilers}
    for run in range(args.runs):
        # Alternate order to reduce systematic bias from warming or clock changes.
        for name, compiler, flags in compilers[::1 if run % 2 == 0 else -1]:
            out = work / name.replace(" ", "-")
            out.mkdir(exist_ok=True)
            cmd = [compiler, *flags, "-fforce-recomp", "-c", str(hs),
                   "-i" + str(work), "-odir", str(out), "-hidir", str(out)]
            start = time.monotonic()
            proc = subprocess.run(cmd, env=env, capture_output=True, text=True, timeout=300)
            elapsed = time.monotonic() - start
            (out / f"run-{run + 1}.log").write_text(proc.stdout + proc.stderr)
            if proc.returncode:
                raise SystemExit(f"{name} failed: see {out / f'run-{run + 1}.log'}")
            results[name].append(elapsed)
            print(f"{name}: {elapsed:.3f}s", flush=True)
    (work / "times.json").write_text(json.dumps(results, indent=2) + "\n")
    medians = [statistics.median(results[name]) for name, _, _ in compilers]
    chart = "\n".join([
        f"Cold front end, median of {args.runs} runs; module cache disabled for hsc.",
        "hsc uses -c (Core and source interfaces); GHC uses -fno-code (no object code).",
        "", "```mermaid", "xychart-beta", '    title "Instance lookup front end (seconds)"',
        '    x-axis ["Before", "After", "GHC -O0", "GHC -O1"]',
        f"    y-axis \"Seconds\" 0 --> {max(medians) * 1.1:.3f}",
        "    bar [" + ", ".join(f"{t:.3f}" for t in medians) + "]", "```", "",
    ])
    (work / "chart.md").write_text(chart)
    print(chart)


if __name__ == "__main__":
    main()
