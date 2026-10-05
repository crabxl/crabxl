"""Unconfigured ten-column numeric writer regression against an exact prior binary."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import statistics
import tempfile
import zipfile
from iso_checkpoint import measure

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--baseline-core", default="e2e5185453f4e4a3437a29872c38ce2dac39e8bf")
    parser.add_argument("--output", type=Path, default=HERE / "results/m5-worksheet-views-regression.json")
    parser.add_argument("--rows", type=int, nargs="+", default=[5000, 50000])
    parser.add_argument("--runs", type=int, default=5)
    args = parser.parse_args()
    current = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "release/examples/write_demo"
    report = {"baseline_core": args.baseline_core, "baseline_sha256": hashlib.sha256(args.baseline.read_bytes()).hexdigest(), "semantics": f"Same unconfigured ten-column numeric writer and scalar generation in both versions; one warmup plus {args.runs} rotating serial samples. Exact worksheet XML SHA/bytes match in every sample. Linux process wall/CPU/RSS, separate TMPDIR, no builds/tests during timing. 25ms temporary sampling is a lower bound; exact spool includes footer and excludes final ZIP.", "cases": []}
    for rows in args.rows:
        paths = {name: HERE / "data" / f"views-regression-{name}-{rows}.xlsx" for name in ("current", "prior")}
        commands = {"current": [current, paths["current"], rows, "numeric"], "prior": [args.baseline, paths["prior"], rows, "numeric"]}
        samples = {name: [] for name in commands}
        expected = None
        with tempfile.TemporaryDirectory(prefix="views-regression-") as directory:
            for command in commands.values(): measure(command, Path(directory))
            for iteration in range(args.runs):
                names = list(commands)
                for name in names[iteration % 2:] + names[:iteration % 2]:
                    output, sample = measure(commands[name], Path(directory))
                    row_count, cells, spool = map(int, output.split())
                    assert (row_count, cells) == (rows, rows * 10)
                    sample["logical_temp_peak_bytes"] = spool
                    with zipfile.ZipFile(paths[name]) as archive:
                        body = archive.read("xl/worksheets/sheet1.xml")
                        digest = hashlib.sha256(body).hexdigest()
                        assert spool == len(body)
                        if expected is None: expected = digest
                        assert digest == expected
                    samples[name].append(sample)
        case = {"rows": rows, "cells": rows * 10, "worksheet_sha256": expected, "samples": samples, "medians": {name: {key: statistics.median(sample[key] for sample in values) for key in ("seconds", "cpu_seconds", "peak_rss_kib", "sampled_temp_peak_bytes")} for name, values in samples.items()}}
        report["cases"].append(case)
        print(rows, case["medians"], flush=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")

if __name__ == "__main__": main()
