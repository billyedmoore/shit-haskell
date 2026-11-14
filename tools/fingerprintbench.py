#!/usr/bin/env python3
"""Compare native fingerprintData throughput, including GHC and the runtime target.

Builds/warmups are untimed. Each process fills a byte buffer, mutates its first
byte before each hash, and prints every digest. Run without competing local jobs.
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
    return elapsed, proc.stdout


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", required=True)
    parser.add_argument("--after", required=True)
    parser.add_argument("--ghc", default="ghc")
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--mib", type=int, default=16)
    parser.add_argument("--hashes", type=int, default=8)
    parser.add_argument("--work", type=Path, default=Path("target/fingerprintbench"))
    args = parser.parse_args()
    if min(args.runs, args.mib, args.hashes) < 1:
        parser.error("runs/mib/hashes must be positive")
    work = args.work.resolve()
    work.mkdir(parents=True, exist_ok=True)
    source = work / "FingerprintBench.hs"
    source.write_text(f"""module Main where
import Data.Word
import Foreign.Marshal.Alloc
import Foreign.Marshal.Utils
import Foreign.Storable
import GHC.Fingerprint

main :: IO ()
main = allocaBytes {args.mib * 1024 * 1024} $ \\ptr -> do
  fillBytes ptr 97 {args.mib * 1024 * 1024}
  mapM_ (\\i -> do
    poke ptr (fromIntegral i :: Word8)
    fingerprintData ptr {args.mib * 1024 * 1024} >>= print) [1 .. {args.hashes} :: Int]
""")
    env = dict(os.environ, XDG_CACHE_HOME=str(work / "cache"))
    for key in ("HSC_NO_CACHE", "HSC_NATIVE", "HSC_TIME", "HSC_TIME_TC", "HSC_PROGRESS", "HSC_DEBUG_CACHE"):
        env.pop(key, None)
    compilers = [("Before", args.before, ["--native"]), ("After", args.after, ["--native"]),
                 ("GHC -O0", args.ghc, ["-O0"]), ("GHC -O1", args.ghc, ["-O1"])]
    commands = {}
    builds = {}
    expected = None
    for name, compiler, flags in compilers:
        out = work / name.replace(" ", "-")
        out.mkdir(exist_ok=True)
        executable = out / "fingerprint"
        build = [compiler, *flags, "-fforce-recomp", str(source), "-odir", str(out),
                 "-hidir", str(out), "-o", str(executable)]
        builds[name] = build
        run(build, env, out / "build.log")
        commands[name] = [str(executable)]
        elapsed, stdout = run(commands[name], env, out / "warmup.log")
        if expected is None:
            expected = stdout
        if stdout != expected:
            raise SystemExit(f"digest mismatch: see {out / 'warmup.log'}")
        print(f"Warmup {name}: {elapsed:.6f}s", flush=True)
    results = {name: [] for name, _, _ in compilers}
    for i in range(args.runs):
        for name, _, _ in compilers[::1 if i % 2 == 0 else -1]:
            out = work / name.replace(" ", "-")
            elapsed, stdout = run(commands[name], env, out / f"run-{i + 1}.log")
            if stdout != expected:
                raise SystemExit(f"digest mismatch in {name} run {i + 1}")
            results[name].append(elapsed)
            print(f"{name}: {elapsed:.6f}s", flush=True)
    medians = [statistics.median(results[name]) for name, _, _ in compilers]
    target = 0.9 * medians[-1]
    (work / "times.json").write_text(json.dumps(dict(results=results, commands=commands,
        builds=builds, target=target, mib=args.mib, hashes=args.hashes), indent=2) + "\n")
    chart = "\n".join([
        f"Native runtime, median of {args.runs} alternating runs; startup, fill and output included.",
        f"{args.hashes} fingerprintData calls on {args.mib} MiB, with a changed first byte before each hash.",
        "Every digest is printed and compared across all four builds; one untimed warmup each.",
        "", "```mermaid", "xychart-beta", '    title "Native byte fingerprinting (seconds)"',
        '    x-axis ["Before", "After", "GHC -O0", "GHC -O1", "0.9x O1 target"]',
        f'    y-axis "Seconds" 0 --> {max(medians) * 1.1:.3f}',
        "    bar [" + ", ".join(f"{t:.3f}" for t in [*medians, target]) + "]", "```", "",
    ])
    (work / "chart.md").write_text(chart)
    print(chart)


if __name__ == "__main__":
    main()
