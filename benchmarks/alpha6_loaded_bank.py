"""Measure explicit lazy-bank and legacy standalone model loading; Linux wait4."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import statistics
import subprocess

ROOT = Path(__file__).resolve().parents[1]
HERE = ROOT / "benchmarks"
EXAMPLES = ROOT / "target/release/examples"


def run(command):
    return subprocess.run(list(map(str, command)), cwd=ROOT, text=True, capture_output=True, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=HERE / "results/alpha6-loaded-current.local.json")
    parser.add_argument("--checkpoint", default="A6 lazy canonical bank ownership; not M4 completion")
    parser.add_argument("--edit", action="store_true", help="Also verify full model/read/edit/save/reload")
    parser.add_argument("--active", action="store_true", help="Also verify lazy active selection/save/reload")
    parser.add_argument("--visibility", action="store_true", help="Also verify lazy visibility/active normalization/save/reload")
    parser.add_argument("--deferred", action="store_true", help="Also verify a signed relative view/save/reload")
    args = parser.parse_args()
    report = {
        "checkpoint": args.checkpoint,
        "platform": platform.platform(),
        "binary_sha256": hashlib.sha256((EXAMPLES / "loaded_rows").read_bytes()).hexdigest(),
        "cpu_quota": Path("/sys/fs/cgroup/cpu.max").read_text().strip(),
        "memory_ceiling": Path("/sys/fs/cgroup/memory.max").read_text().strip(),
        "rust": run(["rustc", "--version"]).stdout.strip(),
        "method": "one warmup, three rotating serial release runs; native wait4 wall/CPU/peak RSS; build and generation excluded",
        "comparison": "same current decoder and owned sparse cell type; bank transfers source styles and enforces joint resource accounting; standalone reproduces prior per-sheet native materialization; not Python or streaming performance",
        "cases": [],
    }
    for rows in [10_000, 100_000]:
        path = HERE / "data" / f"alpha6-loaded-{rows}.xlsx"
        run([EXAMPLES / "workbook_demo", rows, path])
        count = rows * 10
        expected = {"cells": 2 * count, "checksum": count * (count - 1)}
        modes = ["bank", "standalone", "bank-edit"] if args.edit else ["bank", "standalone"]
        if args.active:
            modes.append("bank-active")
        if args.visibility:
            modes.append("bank-visibility")
        if args.deferred:
            modes.append("bank-deferred")
        samples = {mode: [] for mode in modes}
        for repeat in range(4):
            order = modes[repeat % len(modes):] + modes[:repeat % len(modes)]
            for mode in order:
                target = path.with_suffix(".edited.xlsx")
                command = [HERE / "measure", EXAMPLES / "loaded_rows", path, mode]
                if mode in ["bank-edit", "bank-active", "bank-visibility", "bank-deferred"]:
                    command.append(target)
                result = run(command)
                output = json.loads(result.stdout)
                assert all(output[key] == value for key, value in expected.items()), output
                assert output["sst_temp_bytes"] == 0
                assert output["materialized_cells"] == (0 if mode in ["bank-active", "bank-visibility", "bank-deferred"] else expected["cells"])
                if mode in ["bank-edit", "bank-active", "bank-visibility", "bank-deferred"]:
                    assert output["verified_edit"] and output["output_bytes"] == target.stat().st_size
                    assert not list(path.parent.glob("crabxl-save-*"))
                    target.unlink()
                measure = json.loads(result.stderr.split("MEASURE ")[-1])
                if repeat:
                    samples[mode].append({**measure, "managed_bytes": output["managed_bytes"], "sst_temp_bytes": output["sst_temp_bytes"], "completed_adjacent_zip_bytes": output["output_bytes"]})
        case = {"rows_per_sheet": rows, "columns": 10, "sheets": 2, **expected,
                "file_bytes": path.stat().st_size, "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                "samples": samples,
                "medians": {mode: {key: statistics.median(sample[key] for sample in values)
                                   for key in values[0]} for mode, values in samples.items()}}
        report["cases"].append(case)
        print(json.dumps({"rows": rows, "medians": case["medians"]}), flush=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
