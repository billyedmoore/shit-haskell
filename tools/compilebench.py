#!/usr/bin/env python3
"""Measure cold front-end instance lookup against an earlier hsc, current hsc, and GHC.

Example: python3 tools/compilebench.py --before target/before/release/hsc \
    --after target/release/hsc --runs 3
The generated source, raw measurements and Markdown charts stay in --work.
NoImplicitPrelude isolates instance lookup from boot-library startup.
--workload module-cache generates 200 modules with 128 bindings each and performs
one untimed warmup to create hsc's placeholder objects before measuring the front end.
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


def module_sources(work):
    """A dependency chain makes the accumulated environment grow at each snapshot."""
    (work / "Seed.hs").write_text(
        "{-# LANGUAGE NoImplicitPrelude #-}\nmodule Seed where\ndata Result = Result\n")
    previous = "Seed"
    for i in range(200):
        name = f"Step{i:03}"
        lines = ["{-# LANGUAGE NoImplicitPrelude #-}", f"module {name} where",
                 "import Seed (Result(..))"]
        if previous != "Seed":
            lines.append(f"import {previous} ()")
        for j in range(128):
            lines += [f"value{j} :: Result", f"value{j} = Result"]
        (work / f"{name}.hs").write_text("\n".join(lines) + "\n")
        previous = name
    root = work / "CompileModules.hs"
    root.write_text("{-# LANGUAGE NoImplicitPrelude #-}\nmodule CompileModules where\n"
                    f"import {previous} ()\nimport Seed (Result(..))\nvalue :: Result\nvalue = Result\n")
    return root


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", required=True)
    parser.add_argument("--after", required=True)
    parser.add_argument("--ghc", default="ghc")
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--workload", choices=("instance-lookup", "module-cache"), default="instance-lookup")
    parser.add_argument("--work", type=Path, default=Path("target/compilebench"))
    args = parser.parse_args()
    if args.runs < 1:
        parser.error("--runs must be positive")
    work = args.work.resolve()
    work.mkdir(parents=True, exist_ok=True)
    if args.workload == "module-cache":
        hs = module_sources(work)
        mode = ["--make", "-no-link"]
        title = "Module cache bookkeeping front end (seconds)"
    else:
        hs = work / "InstanceLookup.hs"
        hs.write_text(source())
        mode = ["-c"]
        title = "Instance lookup front end (seconds)"
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
    warmups = 1 if args.workload == "module-cache" else 0
    for run in range(-warmups, args.runs):
        # Alternate order to reduce systematic bias from warming or clock changes.
        for name, compiler, flags in compilers[::1 if run < 0 or run % 2 == 0 else -1]:
            out = work / name.replace(" ", "-")
            out.mkdir(exist_ok=True)
            cmd = [compiler, *flags, "-fforce-recomp", *mode, str(hs),
                   "-i" + str(work), "-odir", str(out), "-hidir", str(out)]
            start = time.monotonic()
            proc = subprocess.run(cmd, env=env, capture_output=True, text=True, timeout=300)
            elapsed = time.monotonic() - start
            log = out / ("warmup.log" if run < 0 else f"run-{run + 1}.log")
            log.write_text(proc.stdout + proc.stderr)
            if proc.returncode:
                raise SystemExit(f"{name} failed: see {log}")
            if run < 0:
                print(f"Warmup {name}: {elapsed:.3f}s", flush=True)
                continue
            results[name].append(elapsed)
            print(f"{name}: {elapsed:.3f}s", flush=True)
    (work / "times.json").write_text(json.dumps(results, indent=2) + "\n")
    medians = [statistics.median(results[name]) for name, _, _ in compilers]
    chart = "\n".join([
        f"Cold front end, median of {args.runs} runs; module cache disabled for hsc.",
        f"Untimed warmups per compiler: {warmups}.",
        "hsc emits Core, source interfaces and placeholder objects; GHC uses -fno-code (no object code).",
        "", "```mermaid", "xychart-beta", f'    title "{title}"',
        '    x-axis ["Before", "After", "GHC -O0", "GHC -O1"]',
        f"    y-axis \"Seconds\" 0 --> {max(medians) * 1.1:.3f}",
        "    bar [" + ", ".join(f"{t:.3f}" for t in medians) + "]", "```", "",
    ])
    (work / "chart.md").write_text(chart)
    print(chart)


if __name__ == "__main__":
    main()
