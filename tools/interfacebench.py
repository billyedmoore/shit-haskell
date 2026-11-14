#!/usr/bin/env python3
"""Compare warm package-interface loading in two hsc builds and GHC.

Builds a small package with each compiler, registers its own interface directory,
then compiles a client against those installed interfaces. Package builds and one
client warmup are untimed. Large comments isolate embedded-source hashing from
type-checking work. Run without competing local compiler or test jobs.
"""

import argparse
import json
import os
from pathlib import Path
import statistics
import subprocess
import time


def run(command, env, log):
    start = time.monotonic()
    proc = subprocess.run(command, env=env, capture_output=True, text=True, timeout=300)
    elapsed = time.monotonic() - start
    log.write_text(proc.stdout + proc.stderr)
    if proc.returncode:
        raise SystemExit(f"command failed: see {log}")
    return elapsed


def sources(work, count, padding_kib):
    source = work / "source"
    source.mkdir(exist_ok=True)
    prefix = "{-# LANGUAGE NoImplicitPrelude #-}\n"
    (source / "BenchSeed.hs").write_text(prefix + "module BenchSeed where\ndata Result = Result\n")
    modules = ["BenchSeed"]
    previous = None
    padding = "{-" + "x" * (padding_kib * 1024) + "-}\n"
    for i in range(count):
        name = f"BenchStep{i:03}"
        text = prefix + padding + f"module {name} where\nimport BenchSeed\n"
        if previous:
            text += f"import {previous} ()\n"
        for j in range(32):
            text += f"value{j} :: Result\nvalue{j} = Result\n"
        (source / f"{name}.hs").write_text(text)
        modules.append(name)
        previous = name
    client = work / "InterfaceClient.hs"
    client.write_text(prefix + "module InterfaceClient where\nimport BenchSeed\n"
                      + f"import {previous}\nvalue :: Result\nvalue = value0\n")
    return source, source / f"{previous}.hs", client, modules


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", required=True)
    parser.add_argument("--after", required=True)
    parser.add_argument("--ghc", default="ghc")
    parser.add_argument("--ghc-pkg", default="ghc-pkg")
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--modules", type=int, default=50)
    parser.add_argument("--padding-kib", type=int, default=256)
    parser.add_argument("--work", type=Path, default=Path("target/interfacebench"))
    args = parser.parse_args()
    if min(args.runs, args.modules) < 1 or args.padding_kib < 0:
        parser.error("runs/modules must be positive and padding must be nonnegative")
    work = args.work.resolve()
    work.mkdir(parents=True, exist_ok=True)
    source, last, client, modules = sources(work, args.modules, args.padding_kib)
    env = dict(os.environ, XDG_CACHE_HOME=str(work / "cache"))
    for key in ("HSC_NO_CACHE", "HSC_NATIVE", "HSC_TIME", "HSC_TIME_TC", "HSC_PROGRESS", "HSC_DEBUG_CACHE"):
        env.pop(key, None)
    compilers = [("Before", args.before, []), ("After", args.after, []),
                 ("GHC -O0", args.ghc, ["-O0"]), ("GHC -O1", args.ghc, ["-O1"])]
    commands = {}
    for name, compiler, flags in compilers:
        out = work / name.replace(" ", "-")
        package = out / "package"
        database = out / "package-db"
        obj = out / "client"
        for directory in (package, database, obj):
            directory.mkdir(parents=True, exist_ok=True)
        command = [compiler, *flags, "--make", "-no-link", "-fforce-recomp",
                   "-this-unit-id", "interfacebench-0.1", str(last), "-i" + str(source),
                   "-odir", str(package), "-hidir", str(package)]
        run(command, env, out / "package-build.log")
        config = ("name: interfacebench\nversion: 0.1\nid: interfacebench-0.1\n"
                  "key: interfacebench-0.1\nexposed: True\n"
                  + "exposed-modules: " + ", ".join(modules) + "\n"
                  + f"import-dirs: {package}\nlibrary-dirs: {package}\n")
        (database / "interfacebench.conf").write_text(config)
        run([args.ghc_pkg, "--package-db=" + str(database), "recache"], env, out / "register.log")
        # The client checks the installed package's interfaces; its source directory
        # is deliberately absent from the search path.
        command = [compiler, *flags, *(["-fno-code"] if name.startswith("GHC") else []),
                   "-c", "-fforce-recomp", "-hide-all-packages", "-package-db", str(database),
                   "-package", "interfacebench", str(client), "-i" + str(work),
                   "-odir", str(obj), "-hidir", str(obj)]
        commands[name] = command
        elapsed = run(command, env, out / "warmup.log")
        print(f"Warmup {name}: {elapsed:.6f}s", flush=True)
    results = {name: [] for name, _, _ in compilers}
    for i in range(args.runs):
        for name, _, _ in compilers[::1 if i % 2 == 0 else -1]:
            out = work / name.replace(" ", "-")
            elapsed = run(commands[name], env, out / f"run-{i + 1}.log")
            results[name].append(elapsed)
            print(f"{name}: {elapsed:.6f}s", flush=True)
    (work / "times.json").write_text(json.dumps(dict(results=results, commands=commands), indent=2) + "\n")
    medians = [statistics.median(results[name]) for name, _, _ in compilers]
    chart = "\n".join([
        f"Warm installed interfaces, median of {args.runs} alternating runs; caches enabled.",
        f"{args.modules} step modules plus a seed; each step has {args.padding_kib} KiB of comment padding and 32 values.",
        "Each compiler builds its package first; one untimed client warmup per compiler.",
        "hsc emits Core/source interfaces and a placeholder object; GHC uses -fno-code; no linking.",
        "Package size is not an interface-read count: GHC may load interfaces lazily.",
        "", "```mermaid", "xychart-beta", '    title "Warm installed package interfaces (seconds)"',
        '    x-axis ["Before", "After", "GHC -O0", "GHC -O1"]',
        f'    y-axis "Seconds" 0 --> {max(medians) * 1.1:.3f}',
        "    bar [" + ", ".join(f"{t:.3f}" for t in medians) + "]", "```", "",
    ])
    (work / "chart.md").write_text(chart)
    print(chart)


if __name__ == "__main__":
    main()
