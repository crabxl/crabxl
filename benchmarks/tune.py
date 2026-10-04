"""Measure whether larger input buffers improve numeric parsing on this host."""
import argparse
import json
import statistics

from run import HERE, ROOT, measure, run


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rows", type=int, default=1000000)
    parser.add_argument("--runs", type=int, default=3)
    args = parser.parse_args()
    if args.runs < 1 or args.rows < 1:
        parser.error("Rows and runs must be positive")
    path = HERE / "data" / f"numbers-{args.rows}.xlsx"
    if not path.exists():
        parser.error("Generate this input first with benchmarks/run.py")
    run(["cc", "-O2", "-Wall", "-Wextra", "-Werror", HERE / "measure.c", "-o", HERE / "measure"])
    run(["cargo", "build", "--release", "--locked", "--examples"])
    buffers = [32 * 1024, 256 * 1024, 1024 * 1024, 8 * 1024 * 1024]
    samples = {size: [] for size in buffers}
    commands = {size: [ROOT / "target/release/examples/sum", path, "Sheet", size] for size in buffers}
    for size in buffers:
        measure(commands[size], args.rows * 10)
    for index in range(args.runs):
        for size in buffers[index % 4:] + buffers[:index % 4]:
            sample = measure(commands[size], args.rows * 10)
            samples[size].append(sample)
            print(f"buffer={size} {sample}", flush=True)
    result = {
        "rows": args.rows, "columns": 10, "cells": args.rows * 10,
        "checksum": args.rows * 10 * (args.rows * 10 - 1) // 2,
        "measurement": "Same environment/input as numeric-results.json; native Linux wait4; one warmup per size, rotating serial order, three measured runs by default; no baseline subtraction",
        "rust": run(["rustc", "--version"]).stdout.strip(),
        "samples": samples,
        "medians": {size: {key: statistics.median(sample[key] for sample in values)
                           for key in ("seconds", "cpu_seconds", "peak_rss_kib")}
                    for size, values in samples.items()},
    }
    (HERE / "buffer-results.json").write_text(json.dumps(result, indent=2) + "\n")


if __name__ == "__main__":
    main()
