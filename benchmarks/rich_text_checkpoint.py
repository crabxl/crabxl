"""Full rich-run verification and explicitly separate plain projections."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import sys
import tempfile
import zipfile
import openpyxl
from shared_strings_checkpoint import ROOT, HERE, MAIN, generate as generate_plain, run


def generate(path, rows, unique):
    with tempfile.TemporaryDirectory() as temp:
        plain = Path(temp) / "plain.xlsx"
        generate_plain(plain, rows, unique)
        with zipfile.ZipFile(plain) as source, zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as target:
            for member in source.infolist():
                with target.open(member.filename, "w") as output:
                    if member.filename == "xl/sharedStrings.xml":
                        output.write(f'<sst xmlns="{MAIN}" count="{rows*10}" uniqueCount="{unique}">'.encode())
                        for index in range(unique):
                            output.write((f'<si><r><rPr><b/><i val="0"/><color theme="3" tint="0.25"/></rPr>'
                                f'<t>item-{index:08}-</t></r><r><rPr><rFont val="Aptos"/><u val="double"/></rPr>'
                                f'<t>{"x"*96}</t></r></si>').encode())
                        output.write(b'</sst>')
                    else:
                        with source.open(member) as input_file:
                            while chunk := input_file.read(65536):
                                output.write(chunk)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rows", type=int, nargs="+", default=[10000, 100000])
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--output", type=Path, default=HERE / "rich-text.local.json")
    args = parser.parse_args()
    if openpyxl.__version__ != "3.1.5" or args.runs < 1 or any(not 0 < r <= 1048576 for r in args.rows):
        parser.error("Require pinned openpyxl, positive runs and valid row bounds")
    run(["cc", "-O2", "-Wall", "-Wextra", "-Werror", HERE / "measure.c", "-o", HERE / "measure"])
    run(["cargo", "build", "--release", "--locked", "-p", "crabxl", "--example", "shared_text", "--example", "rich_text"])
    run(["cargo", "build", "--release", "--locked", "--manifest-path", HERE / "calamine/Cargo.toml", "--bin", "shared_text"])
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    report = {"workload": "Two formatted runs per shared value: 14-byte prefix and 96-byte suffix; verify all displayed text and specified font/color overrides", "measurement": f"Linux fork/exec/wait4; one warmup and {args.runs} rotating serial runs; build/generation excluded", "versions": {"openpyxl": openpyxl.__version__, "calamine": "0.36.1", "rust": run(["rustc", "--version"]).stdout.strip()}, "platform": platform.platform(), "semantics": "Typed rich and flattened projections are separate comparison groups. Calamine only competes on plain text. Native rows stream; openpyxl read-only eagerly retains SST; calamine materializes Range. Memory=512MiB; Disk/Auto=16MiB component budget, cache=1MiB. No global RSS cap.", "cases": []}
    data = HERE / "data"
    data.mkdir(exist_ok=True)
    for rows in args.rows:
        for kind, unique in [("repeated", 128), ("unique", rows*10)]:
            path = data / f"rich-{kind}-{rows}.xlsx"
            generate(path, rows, unique)
            with tempfile.TemporaryDirectory(prefix="crabxl-rich-bench-") as temp:
                commands = {f"rich-crabxl-{p}": [target / "release/examples/rich_text", path, p, unique, temp] for p in ("memory", "disk", "auto")}
                commands["rich-openpyxl"] = [sys.executable, HERE / "read_rich_openpyxl.py", path, unique]
                commands.update({f"plain-crabxl-{p}": [target / "release/examples/shared_text", path, p, unique, temp] for p in ("memory", "disk", "auto")})
                commands["plain-openpyxl"] = [sys.executable, HERE / "read_shared_openpyxl.py", path, unique]
                commands["plain-calamine"] = [target / "release/shared_text" if os.environ.get("CARGO_TARGET_DIR") else HERE / "calamine/target/release/shared_text", path, unique]
                def measure(command):
                    result = run([HERE / "measure", *command])
                    if result.stdout.strip() != f"{rows*10} {rows*10*110}":
                        raise RuntimeError(result.stdout)
                    if list(Path(temp).iterdir()):
                        raise RuntimeError("Owned temporary file leak")
                    sample = json.loads(result.stderr.split("MEASURE ")[-1])
                    for line in result.stderr.splitlines():
                        if line.startswith("STRINGS "):
                            sample["strings"] = json.loads(line[8:])
                    return sample
                for command in commands.values():
                    measure(command)
                samples = {name: [] for name in commands}
                names = list(commands)
                for iteration in range(args.runs):
                    for name in names[iteration % len(names):] + names[:iteration % len(names)]:
                        sample = measure(commands[name])
                        samples[name].append(sample)
                        print(rows, kind, name, sample, flush=True)
                report["cases"].append({"rows": rows, "cells": rows*10, "unique_strings": unique, "kind": kind, "file_bytes": path.stat().st_size, "file_sha256": hashlib.sha256(path.read_bytes()).hexdigest(), "samples": samples, "medians": {name: {key: statistics.median(s[key] for s in values) for key in ("seconds", "cpu_seconds", "peak_rss_kib")} for name, values in samples.items()}})
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(json.dumps(report, indent=2) + "\n")

if __name__ == "__main__":
    main()
