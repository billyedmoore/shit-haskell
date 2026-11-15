#!/usr/bin/env python3
"""Measure cold front-end instance lookup against an earlier hsc, current hsc, and GHC.

Example: python3 tools/compilebench.py --before target/before/release/hsc \
    --after target/release/hsc --runs 3
The generated source, raw measurements and Markdown charts stay in --work.
NoImplicitPrelude isolates instance lookup from boot-library startup.
--workload module-cache generates 200 modules with 128 bindings each and performs
one untimed warmup to create hsc's placeholder objects before measuring the front end.
--workload boot-cache measures warm dependency checking of 50 SOURCE cycles. Both
compilers get an untimed build; GHC keeps its native objects, hsc its checked modules.
--workload application-equalities checks nested applications under recursive
superclass equalities, as used by ghc-lib-parser's annotation constraints.
--workload type-substitution repeatedly instantiates monomorphic functions with
a large closed type, isolating copying of unchanged type application branches.
--workload typed-cache measures warm loading of 50 modules with 64 signatures
each at a closed 64-layer type; hsc retains shared types in its checked cache.
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


def application_equalities_source():
    lines = [
        "{-# LANGUAGE NoImplicitPrelude, AllowAmbiguousTypes, FlexibleInstances,",
        "    MultiParamTypeClasses, ScopedTypeVariables, TypeFamilies, UndecidableSuperClasses #-}",
        "module ApplicationEqualities where",
        "data Result = Result",
        "data Proxy a = Proxy",
        "data Parsed",
        "data Renamed",
        "type family Body b p where",
        "  Body b Parsed = b",
        "  Body b Renamed = [b]",
        "class (b ~ Body b p) => HasBody b p",
        "class Tag a where tag :: Proxy a -> Result",
    ]
    nested = "b"
    for _ in range(16):
        nested = f"Body ({nested}) p"
    for i in range(500):
        lines += [f"probe{i} :: forall b p. (HasBody b p, Tag b) => Proxy ({nested}) -> Result",
                  f"probe{i} = tag"]
    return "\n".join(lines) + "\n"


def type_substitution_source():
    closed = "Result"
    for _ in range(128):
        closed = f"Box ({closed})"
    lines = ["{-# LANGUAGE NoImplicitPrelude #-}", "module TypeSubstitution where",
             "data Result = Result", "data Box a = Box a", f"type Closed = {closed}",
             "callee :: Closed -> Closed", "callee x = x"]
    for i in range(500):
        lines += [f"probe{i} :: Closed -> Closed",
                  f"probe{i} x = callee (callee (callee (callee x)))"]
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


def boot_sources(work):
    """Each implementation completes a boot interface loaded through its consumer."""
    (work / "Seed.hs").write_text(
        "{-# LANGUAGE NoImplicitPrelude #-}\nmodule Seed where\ndata Result = Result\n")
    previous = None
    for i in range(50):
        name = f"Boot{i:03}"
        consumer = f"Consumer{i:03}"
        prefix = "{-# LANGUAGE NoImplicitPrelude #-}\n"
        (work / f"{name}.hs-boot").write_text(prefix + f"module {name} where\ndata T\n")
        lines = [prefix, f"module {consumer} where", f"import {{-# SOURCE #-}} {name}", "import Seed"]
        if previous:
            lines.append(f"import {previous} ()")
        lines += ["touch :: Result", "touch = Result"]
        (work / f"{consumer}.hs").write_text("\n".join(lines) + "\n")
        lines = [prefix, f"module {name} where", "import Seed", f"import {consumer}", "data T = T",
                 "fromConsumer :: Result", "fromConsumer = touch"]
        for j in range(128):
            lines += [f"value{j} :: Result", f"value{j} = Result"]
        (work / f"{name}.hs").write_text("\n".join(lines) + "\n")
        previous = name
    root = work / "CompileBoot.hs"
    root.write_text(prefix + f"module CompileBoot where\nimport {previous} ()\n")
    return root


def typed_sources(work):
    prefix = "{-# LANGUAGE NoImplicitPrelude #-}\n"
    closed = "Box (" * 64 + "Result" + ")" * 64
    (work / "Seed.hs").write_text(prefix + "module Seed where\ndata Result = Result\n"
                                   + "data Box a = Box a\n" + f"type Closed = {closed}\n")
    previous = None
    for i in range(50):
        name = f"Typed{i:03}"
        lines = [prefix, f"module {name} where", "import Seed"]
        if previous:
            lines.append(f"import {previous} ()")
        for j in range(64):
            lines += [f"value{j} :: Closed -> Closed", f"value{j} x = x"]
        (work / f"{name}.hs").write_text("\n".join(lines) + "\n")
        previous = name
    root = work / "CompileTyped.hs"
    root.write_text(prefix + f"module CompileTyped where\nimport {previous} ()\n")
    return root


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", required=True)
    parser.add_argument("--after", required=True)
    parser.add_argument("--ghc", default="ghc")
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--workload", choices=("instance-lookup", "module-cache", "boot-cache", "application-equalities", "type-substitution", "typed-cache"), default="instance-lookup")
    parser.add_argument("--work", type=Path, default=Path("target/compilebench"))
    args = parser.parse_args()
    if args.runs < 1:
        parser.error("--runs must be positive")
    work = args.work.resolve()
    work.mkdir(parents=True, exist_ok=True)
    warm = args.workload in ("boot-cache", "typed-cache")
    if args.workload == "typed-cache":
        hs = typed_sources(work)
        mode = ["--make", "-no-link"]
        title = "Warm shared type cache (seconds)"
    elif args.workload == "boot-cache":
        hs = boot_sources(work)
        mode = ["--make", "-no-link"]
        title = "Warm SOURCE dependency build (seconds)"
    elif args.workload == "module-cache":
        hs = module_sources(work)
        mode = ["--make", "-no-link"]
        title = "Module cache bookkeeping front end (seconds)"
    elif args.workload == "application-equalities":
        hs = work / "ApplicationEqualities.hs"
        hs.write_text(application_equalities_source())
        mode = ["-c"]
        title = "Nested application equalities front end (seconds)"
    elif args.workload == "type-substitution":
        hs = work / "TypeSubstitution.hs"
        hs.write_text(type_substitution_source())
        mode = ["-c"]
        title = "Closed type substitution front end (seconds)"
    else:
        hs = work / "InstanceLookup.hs"
        hs.write_text(source())
        mode = ["-c"]
        title = "Instance lookup front end (seconds)"
    env = dict(os.environ, HSC_NO_CACHE="1", XDG_CACHE_HOME=str(work / "cache"))
    if warm:
        env.pop("HSC_NO_CACHE", None)
    # Environment switches must not select a different backend or produce profiling noise.
    for key in ("HSC_NATIVE", "HSC_TIME", "HSC_TIME_TC", "HSC_PROGRESS", "HSC_DEBUG_CACHE"):
        env.pop(key, None)
    compilers = [
        ("Before", args.before, []), ("After", args.after, []),
        ("GHC -O0", args.ghc, ["-O0", *([] if warm else ["-fno-code"])]),
        ("GHC -O1", args.ghc, ["-O1", *([] if warm else ["-fno-code"])]),
    ]
    results = {name: [] for name, _, _ in compilers}
    warmups = 1 if args.workload in ("module-cache", "boot-cache", "typed-cache") else 0
    for run in range(-warmups, args.runs):
        # Alternate order to reduce systematic bias from warming or clock changes.
        for name, compiler, flags in compilers[::1 if run < 0 or run % 2 == 0 else -1]:
            out = work / name.replace(" ", "-")
            out.mkdir(exist_ok=True)
            cmd = [compiler, *flags, *([] if warm else ["-fforce-recomp"]), *mode, str(hs),
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
        (f"Warm dependency build, median of {args.runs} runs; caches enabled." if warm else
         f"Cold front end, median of {args.runs} runs; module cache disabled for hsc."),
        f"Untimed warmups per compiler: {warmups}.",
        ("hsc reuses checked Core and source interfaces; GHC reuses native objects and interfaces; no linking." if warm else
         "hsc emits Core, source interfaces and placeholder objects; GHC uses -fno-code (no object code)."),
        "", "```mermaid", "xychart-beta", f'    title "{title}"',
        '    x-axis ["Before", "After", "GHC -O0", "GHC -O1"]',
        f"    y-axis \"Seconds\" 0 --> {max(medians) * 1.1:.3f}",
        "    bar [" + ", ".join(f"{t:.3f}" for t in medians) + "]", "```", "",
    ])
    (work / "chart.md").write_text(chart)
    print(chart)


if __name__ == "__main__":
    main()
