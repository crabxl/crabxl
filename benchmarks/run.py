"""Reproducible warm-cache numeric experiment; Linux, cc, openpyxl 3.1.5."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import statistics
import subprocess
import sys

import openpyxl

ROOT = Path(__file__).resolve().parents[1]
HERE = ROOT / "benchmarks"


def run(command):
    return subprocess.run(command, cwd=ROOT, check=True, text=True, capture_output=True)


def measure(command, count):
    result = run([str(HERE / "measure"), *map(str, command)])
    expected = f"{count} {count * (count - 1) // 2}"
    if result.stdout.strip() != expected:
        raise RuntimeError(f"Checksum mismatch: {result.stdout!r}, expected {expected}")
    return json.loads(result.stderr.split("MEASURE ")[-1])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rows", type=int, nargs="+", default=[10000, 100000, 1000000])
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--output", type=Path, default=HERE / "results.local.json")
    args = parser.parse_args()
    if openpyxl.__version__ != "3.1.5":
        raise RuntimeError("Install openpyxl==3.1.5 for the pinned baseline")
    if args.runs < 1 or any(not 0 < rows <= 1048576 for rows in args.rows):
        parser.error("Require positive runs and rows within Excel limits")
    run(["cc", "-O2", "-Wall", "-Wextra", "-Werror", HERE / "measure.c", "-o", HERE / "measure"])
    run(["cargo", "build", "--release", "--locked", "--examples"])
    run(["cargo", "build", "--release", "--locked", "--manifest-path", HERE / "calamine/Cargo.toml"])
    report = {
        "workload": "10 columns, consecutive integers from zero, one worksheet",
        "versions": {"rust": run(["rustc", "--version"]).stdout.strip(),
                     "python": platform.python_version(), "openpyxl": openpyxl.__version__,
                     "calamine": "0.36.1"},
        "platform": platform.platform(),
        "cpu": next((line.split(":", 1)[1].strip() for line in Path("/proc/cpuinfo").read_text().splitlines() if line.startswith("model name")), "unknown"),
        "measurement": "native Linux fork/exec/wait4; wall time and kernel process peak RSS; one warmup; rotating serial order; no baseline subtraction; generation/build excluded",
        "temporary_storage_bytes": 0,
        "cases": [],
    }
    data = HERE / "data"
    data.mkdir(exist_ok=True)
    for rows in args.rows:
        path = data / f"numbers-{rows}.xlsx"
        if not path.exists():
            print(f"Generating {rows} rows", flush=True)
            workbook = openpyxl.Workbook(write_only=True)
            sheet = workbook.create_sheet("Sheet")
            for row in range(rows):
                sheet.append([row * 10 + column for column in range(10)])
            workbook.save(path)
        commands = {
            "crabxl": [ROOT / "target/release/examples/sum", path],
            "openpyxl": [sys.executable, HERE / "read_openpyxl.py", path],
            "calamine": [HERE / "calamine/target/release/calamine-baseline", path],
        }
        samples = {name: [] for name in commands}
        for name, command in commands.items():
            print(f"Warmup {rows} {name}", flush=True)
            measure(command, rows * 10)
        names = list(commands)
        for index in range(args.runs):
            for name in names[index % 3:] + names[:index % 3]:
                sample = measure(commands[name], rows * 10)
                samples[name].append(sample)
                print(f"{rows} {name} {sample}", flush=True)
        with path.open("rb") as source:
            digest = hashlib.file_digest(source, "sha256").hexdigest()
        case = {"rows": rows, "columns": 10, "cells": rows * 10,
                "checksum": rows * 10 * (rows * 10 - 1) // 2,
                "file_bytes": path.stat().st_size,
                "file_sha256": digest,
                "samples": samples,
                "medians": {name: {"seconds": statistics.median(s["seconds"] for s in values),
                                  "peak_rss_kib": statistics.median(s["peak_rss_kib"] for s in values)}
                            for name, values in samples.items()}}
        report["cases"].append(case)
        args.output.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
