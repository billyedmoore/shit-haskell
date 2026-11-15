#!/usr/bin/env python3
"""Compare native program runtime with before/after hsc and GHC -O0/-O1.

Builds and correctness warmups are untimed. Timed samples alternate order and
verify stdout, stderr and exit status. Run without competing local jobs.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shlex
import shutil
import statistics
import subprocess
import time


def run(command, env, cwd, stdin, log, timeout):
    start = time.monotonic()
    try:
        proc = subprocess.run(command, env=env, cwd=cwd, input=stdin,
                              capture_output=True, timeout=timeout)
    except subprocess.TimeoutExpired as error:
        log.with_suffix(".stdout").write_bytes(error.stdout or b"")
        log.with_suffix(".stderr").write_bytes(error.stderr or b"")
        raise SystemExit(f"timed out: see {log}") from error
    elapsed = time.monotonic() - start
    log.with_suffix(".stdout").write_bytes(proc.stdout)
    log.with_suffix(".stderr").write_bytes(proc.stderr)
    if proc.returncode:
        raise SystemExit(f"exit {proc.returncode}: see {log}")
    return elapsed, (proc.stdout, proc.stderr, proc.returncode)


def compiler_path(command):
    return str(Path(shutil.which(command) or command).resolve())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", required=True)
    parser.add_argument("--after", required=True)
    parser.add_argument("--ghc", default="ghc")
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--warmups", type=int, default=1)
    parser.add_argument("--cpu", type=int)
    parser.add_argument("--program", action="append", help="program stem; repeat to select several")
    parser.add_argument("--bench", type=Path, default=Path("bench/progs"))
    parser.add_argument("--work", type=Path, default=Path("target/runtimebench"))
    parser.add_argument("--timeout", type=float, default=600)
    args = parser.parse_args()
    if args.runs < 1 or args.warmups < 0 or args.timeout <= 0:
        parser.error("runs/timeout must be positive and warmups nonnegative")
    if args.cpu is not None:
        if not hasattr(os, "sched_setaffinity") or args.cpu not in os.sched_getaffinity(0):
            parser.error("--cpu must name an available CPU with affinity support")
        os.sched_setaffinity(0, {args.cpu})
    sources = sorted(args.bench.resolve().glob("*.hs"))
    if args.program:
        missing = set(args.program) - {source.stem for source in sources}
        if missing:
            parser.error("unknown programs: " + ", ".join(sorted(missing)))
        sources = [source for source in sources if source.stem in args.program]
    if not sources:
        parser.error("no benchmark sources found")
    work = args.work.resolve()
    work.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, XDG_CACHE_HOME=str(work / "cache"))
    for key in ("HSC_NO_CACHE", "HSC_NATIVE", "HSC_TIME", "HSC_TIME_TC", "HSC_PROGRESS",
                "HSC_DEBUG_CACHE", "HSC_DEBUG_SCHEME", "HSC_DEBUG_BINDS", "GHCRTS"):
        env.pop(key, None)
    compilers = [("Before", compiler_path(args.before), ["--native"]),
                 ("After", compiler_path(args.after), ["--native"]),
                 ("GHC -O0", compiler_path(args.ghc), ["-O0"]),
                 ("GHC -O1", compiler_path(args.ghc), ["-O1"])]
    labels = [name for name, _, _ in compilers]
    evidence = dict(runs=args.runs, warmups=args.warmups, cpu=args.cpu, programs={})
    table = ["| Program | Before ms | After ms | GHC -O0 ms | GHC -O1 ms | 0.9×O1 ms |",
             "|---|---:|---:|---:|---:|---:|"]
    charts = []
    for source in sources:
        program = work / source.stem
        program.mkdir(exist_ok=True)
        flags_file, input_file = source.with_suffix(".flags"), source.with_suffix(".stdin")
        flags = shlex.split(flags_file.read_text()) if flags_file.exists() else []
        stdin = input_file.read_bytes() if input_file.exists() else b""
        builds, commands = {}, {}
        for name, compiler, options in compilers:
            out = program / name.replace(" ", "-")
            out.mkdir(exist_ok=True)
            executable = out / "program"
            command = [compiler, "-v0", "-fforce-recomp", *flags, *options,
                       str(source), "-outputdir", str(out), "-o", str(executable)]
            run(command, env, source.parent, b"", out / "build", args.timeout)
            builds[name], commands[name] = command, [str(executable)]
        expected = None

        def sample(name, tag):
            nonlocal expected
            out = program / name.replace(" ", "-")
            elapsed, output = run(commands[name], env, out, stdin, out / tag, args.timeout)
            if expected is None:
                expected = output
            if output != expected:
                raise SystemExit(f"output mismatch: {source.stem}, {name}, {tag}")
            return elapsed

        for i in range(args.warmups):
            for name in labels:
                sample(name, f"warmup-{i + 1}")
        results = {name: [] for name in labels}
        for i in range(args.runs):
            for name in labels[::1 if i % 2 == 0 else -1]:
                elapsed = sample(name, f"run-{i + 1}")
                results[name].append(elapsed)
                print(f"{source.stem} {name}: {elapsed:.6f}s", flush=True)
        medians = {name: statistics.median(results[name]) for name in labels}
        target = 0.9 * medians["GHC -O1"]
        evidence["programs"][source.stem] = dict(
            source=str(source), source_sha256=hashlib.sha256(source.read_bytes()).hexdigest(),
            builds=builds, commands=commands, results=results, medians=medians,
            best={name: min(values) for name, values in results.items()}, target=target)
        # Retain completed programs if a later workload fails or exceeds its timeout.
        (work / "times.json").write_text(json.dumps(evidence, indent=2) + "\n")
        values = [medians[name] for name in labels] + [target]
        table.append("| " + source.stem + " | " + " | ".join(f"{value * 1000:.3f}" for value in values) + " |")
        charts += ["```mermaid", "xychart-beta", f'    title "{source.stem}: native runtime (seconds)"',
                   '    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]',
                   f'    y-axis "Seconds" 0 --> {max(values) * 1.1:.6f}',
                   "    bar [" + ", ".join(f"{value:.6f}" for value in values) + "]", "```", ""]
    report = "\n".join([
        f"Native runtime; median of {args.runs} alternating samples, {args.warmups} untimed correctness warmups per build.",
        "Builds excluded; process startup included. Every stdout/stderr/exit result agrees.",
        f"CPU affinity: {args.cpu if args.cpu is not None else 'inherited'}. GHC uses default RTS settings.",
        "", *table, "", *charts])
    (work / "chart.md").write_text(report)
    print(report)


if __name__ == "__main__":
    main()
