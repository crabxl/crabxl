"""ISO creation/reading with equivalent public value/type checks and owned temp accounting."""
import argparse
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys
import tempfile
import time
import zipfile
import openpyxl
from shared_strings_checkpoint import ROOT, HERE, run


def measure(command, directory):
    environment = dict(os.environ, TMPDIR=str(directory))
    process = subprocess.Popen(list(map(str, [HERE / "measure", *command])), cwd=ROOT, env=environment, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    peak = 0
    while process.poll() is None:
        current = 0
        for path in directory.iterdir():
            try:
                current += path.stat().st_size
            except FileNotFoundError:
                pass
        peak = max(peak, current)
        time.sleep(.025)
    output, errors = process.communicate()
    if process.returncode:
        raise RuntimeError((command, output, errors))
    sample = json.loads(errors.split("MEASURE ")[-1])
    sample["sampled_temp_peak_bytes"] = peak
    for line in errors.splitlines():
        if line.startswith("TEMP_BYTES "):
            sample["logical_temp_peak_bytes"] = int(line.split()[1])
    if list(directory.iterdir()):
        raise RuntimeError("Owned worksheet temporary file leak")
    return output.strip(), sample


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rows", type=int, nargs="+", default=[10000, 100000])
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--output", type=Path, default=HERE / "results/m2-iso-dates.json")
    args = parser.parse_args()
    if openpyxl.__version__ != "3.1.5" or args.runs < 1 or any(not 0 < n <= 1048576 for n in args.rows):
        parser.error("Require pinned reference, positive sample count and valid rows")
    run(["cc", "-O2", "-Wall", "-Wextra", "-Werror", HERE / "measure.c", "-o", HERE / "measure"])
    run(["cargo", "build", "--release", "--locked", "-p", "crabxl", "--example", "iso_fixture", "--example", "iso_read"])
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    data = HERE / "data"
    data.mkdir(exist_ok=True)
    report = {"workload": "Four columns: early literal date, microsecond datetime and clock truncated to ISO milliseconds, numeric elapsed duration; every value and kind checked", "reference": openpyxl.__version__, "platform": platform.platform(), "measurement": f"One warmup and {args.runs} rotating serial samples; Linux wall/CPU/RSS include process baseline; generation/build and verification excluded from creation samples", "semantics": "Both creation engines use sequential worksheet spools; native and reference readers stream. Native exact logical temp includes closed worksheet footer; 25ms disk sampling is a lower bound and excludes target ZIP. Separate owned TMPDIR per process; no residual files. calamine/rust_xlsxwriter do not participate here because their overlapping typed ISO API behavior has not yet been validated.", "cases": []}
    for rows in args.rows:
        for epoch in ("win", "mac"):
            paths = {name: data / f"iso-{epoch}-{name}-{rows}.xlsx" for name in ("crabxl", "openpyxl")}
            create = {"crabxl": [target / "release/examples/iso_fixture", paths["crabxl"], rows, epoch], "openpyxl": [sys.executable, HERE / "write_iso_openpyxl.py", paths["openpyxl"], rows, epoch]}
            with tempfile.TemporaryDirectory(prefix="crabxl-iso-bench-") as directory_name:
                directory = Path(directory_name)
                for command in create.values():
                    output, _ = measure(command, directory)
                    assert output == str(rows * 4)
                samples = {"create-crabxl": [], "create-openpyxl": [], "read-crabxl": [], "read-openpyxl": []}
                read = {"crabxl": [target / "release/examples/iso_read", paths["crabxl"]], "openpyxl": [sys.executable, HERE / "verify_iso_fixture.py", paths["crabxl"]]}
                for command in read.values():
                    output, _ = measure(command, directory)
                    assert output == str(rows * 4)
                names = list(create)
                for iteration in range(args.runs):
                    for name in names[iteration % 2:] + names[:iteration % 2]:
                        output, sample = measure(create[name], directory)
                        assert output == str(rows * 4)
                        assert run([sys.executable, HERE / "verify_iso_fixture.py", paths[name]]).stdout.strip() == str(rows * 4)
                        with zipfile.ZipFile(paths[name]) as archive:
                            sample["worksheet_xml_bytes"] = archive.getinfo("xl/worksheets/sheet1.xml").file_size
                        sample["output_bytes"] = paths[name].stat().st_size
                        if "logical_temp_peak_bytes" in sample:
                            assert sample["logical_temp_peak_bytes"] == sample["worksheet_xml_bytes"]
                        samples[f"create-{name}"].append(sample)
                        print(rows, epoch, "create", name, sample, flush=True)
                    for name in names[iteration % 2:] + names[:iteration % 2]:
                        output, sample = measure(read[name], directory)
                        assert output == str(rows * 4)
                        samples[f"read-{name}"].append(sample)
                        print(rows, epoch, "read", name, sample, flush=True)
                report["cases"].append({"rows": rows, "cells": rows*4, "epoch": epoch, "samples": samples, "medians": {name: {key: statistics.median(s[key] for s in values) for key in ("seconds", "cpu_seconds", "peak_rss_kib", "sampled_temp_peak_bytes")} for name, values in samples.items()}})
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(json.dumps(report, indent=2) + "\n")

if __name__ == "__main__":
    main()
