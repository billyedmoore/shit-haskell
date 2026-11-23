#!/usr/bin/env python3
"""Compare native program runtime: before/after hsc against GHC, level for level.

Every program is built by each compiler at each optimisation level (`-O0`, `-O1`, `-O2` by
default): hsc with `--native -On`, GHC with `-On`. Builds and correctness warmups are untimed.
Timed samples alternate order and verify that stdout, stderr and exit status agree across every
build. Run without competing local jobs.

Writes `times.json` (samples, commands, metadata) and `chart.md` (a report) to the work
directory; `--summary` appends the report to a file too (a CI step summary).
"""

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
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


def output_of(command, cwd=None):
    try:
        return subprocess.check_output(command, cwd=cwd, text=True, stderr=subprocess.DEVNULL).strip()
    except (OSError, subprocess.CalledProcessError):
        return None


def metadata(args, before, after, ghc):
    cpuinfo = Path("/proc/cpuinfo")
    model = next((line.split(":", 1)[1].strip() for line in cpuinfo.read_text().splitlines()
                  if line.startswith("model name")), "unknown") if cpuinfo.exists() else "unknown"
    meta = dict(
        # A compiler built in a checkout (`<repo>/target/release/hsc`) names its commit.
        before_commit=output_of(["git", "rev-parse", "HEAD"], Path(before).parent),
        after_commit=output_of(["git", "rev-parse", "HEAD"], Path(after).parent),
        platform=platform.platform(), cpu_model=model, cpu=args.cpu,
        ghc=output_of([ghc, "--numeric-version"]), rustc=output_of(["rustc", "--version"]),
        hsc_libdir=os.environ.get("HSC_LIBDIR"))
    if os.environ.get("GITHUB_RUN_ID"):
        meta["run_url"] = (f"{os.environ.get('GITHUB_SERVER_URL', 'https://github.com')}/"
                           f"{os.environ['GITHUB_REPOSITORY']}/actions/runs/{os.environ['GITHUB_RUN_ID']}")
    return meta


def geomean(values):
    return math.exp(statistics.fmean(math.log(v) for v in values))


def report(evidence, levels):
    meta = evidence["metadata"]
    lines = [
        f"Native runtime; median of {evidence['runs']} alternating samples, "
        f"{evidence['warmups']} untimed correctness warmups per build.",
        "Builds excluded; process startup included. Every stdout/stderr/exit result agrees across all builds.",
        f"hsc builds with `--native -On`, GHC with `-On` (default RTS settings). "
        f"CPU: {meta['cpu_model']}, affinity {meta['cpu'] if meta['cpu'] is not None else 'inherited'}. "
        f"GHC {meta['ghc']}.",
        "Ratios below 1 favour After.", ""]
    programs = evidence["programs"]
    for level in levels:
        before, after, ghc = (f"{who} -O{level}" for who in ("Before", "After", "GHC"))
        lines += [f"### -O{level}", "",
                  "| Program | Before ms | After ms | GHC ms | After/Before | After/GHC |",
                  "|---|---:|---:|---:|---:|---:|"]
        vs_before, vs_ghc = [], []
        for name, p in programs.items():
            m = p["medians"]
            vs_before.append(m[after] / m[before])
            vs_ghc.append(m[after] / m[ghc])
            lines.append(f"| {name} | {m[before] * 1000:.3f} | {m[after] * 1000:.3f} | {m[ghc] * 1000:.3f} "
                         f"| {vs_before[-1]:.3f} | {vs_ghc[-1]:.3f} |")
        lines += [f"| **geomean** | | | | **{geomean(vs_before):.3f}** | **{geomean(vs_ghc):.3f}** |", "",
                  "```mermaid", "xychart-beta",
                  f'    title "-O{level}: After runtime / GHC runtime (bar), parity (line)"',
                  "    x-axis [" + ", ".join(f'"{name}"' for name in programs) + "]",
                  f'    y-axis "Ratio" 0 --> {max(vs_ghc + [1.0]) * 1.1:.3f}',
                  "    bar [" + ", ".join(f"{r:.3f}" for r in vs_ghc) + "]",
                  "    line [" + ", ".join("1" for _ in programs) + "]", "```", ""]
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--before", required=True)
    parser.add_argument("--after", required=True)
    parser.add_argument("--ghc", default="ghc")
    parser.add_argument("--levels", default="0,1,2", help="comma-separated optimisation levels")
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--warmups", type=int, default=1)
    parser.add_argument("--cpu", help="CPU to pin to, or 'auto' for the lowest available")
    parser.add_argument("--program", action="append", help="program stem; repeat to select several")
    parser.add_argument("--bench", type=Path, default=Path("bench/progs"))
    parser.add_argument("--work", type=Path, default=Path("target/runtimebench"))
    parser.add_argument("--summary", type=Path, help="also append the report to this file")
    parser.add_argument("--timeout", type=float, default=600)
    args = parser.parse_args()
    if args.runs < 1 or args.warmups < 0 or args.timeout <= 0:
        parser.error("runs/timeout must be positive and warmups nonnegative")
    try:
        levels = sorted({int(level) for level in args.levels.split(",")})
    except ValueError:
        parser.error("--levels takes comma-separated numbers")
    if not levels or not all(0 <= level <= 2 for level in levels):
        parser.error("--levels must name levels 0 to 2")
    if args.cpu is not None:
        if not hasattr(os, "sched_setaffinity"):
            parser.error("--cpu needs affinity support")
        available = os.sched_getaffinity(0)
        args.cpu = min(available) if args.cpu == "auto" else int(args.cpu)
        if args.cpu not in available:
            parser.error("--cpu must name an available CPU")
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
    before, after, ghc = compiler_path(args.before), compiler_path(args.after), compiler_path(args.ghc)
    compilers = []
    for level in levels:
        compilers += [(f"Before -O{level}", before, ["--native", f"-O{level}"]),
                      (f"After -O{level}", after, ["--native", f"-O{level}"]),
                      (f"GHC -O{level}", ghc, [f"-O{level}"])]
    labels = [name for name, _, _ in compilers]
    evidence = dict(runs=args.runs, warmups=args.warmups, levels=levels,
                    metadata=metadata(args, before, after, ghc), programs={})
    for source in sources:
        program = work / source.stem
        program.mkdir(exist_ok=True)
        flags_file, input_file = source.with_suffix(".flags"), source.with_suffix(".stdin")
        flags = shlex.split(flags_file.read_text()) if flags_file.exists() else []
        stdin = input_file.read_bytes() if input_file.exists() else b""
        builds, commands = {}, {}
        for name, compiler, options in compilers:
            out = program / name.replace(" ", "")
            out.mkdir(exist_ok=True)
            executable = out / "program"
            command = [compiler, "-v0", "-fforce-recomp", *flags, *options,
                       str(source), "-outputdir", str(out), "-o", str(executable)]
            run(command, env, source.parent, b"", out / "build", args.timeout)
            builds[name], commands[name] = command, [str(executable)]
        expected = None

        def sample(name, tag):
            nonlocal expected
            out = program / name.replace(" ", "")
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
        evidence["programs"][source.stem] = dict(
            source=str(source), source_sha256=hashlib.sha256(source.read_bytes()).hexdigest(),
            builds=builds, commands=commands, results=results,
            medians={name: statistics.median(values) for name, values in results.items()},
            best={name: min(values) for name, values in results.items()})
        # Retain completed programs if a later workload fails or exceeds its timeout.
        (work / "times.json").write_text(json.dumps(evidence, indent=2) + "\n")
    text = report(evidence, levels)
    (work / "chart.md").write_text(text)
    if args.summary:
        with args.summary.open("a") as summary:
            summary.write(text + "\n")
    print(text)


if __name__ == "__main__":
    main()
