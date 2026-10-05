"""Four native Rust engines, explicit modes, verified outputs and Linux wait4."""

import argparse
import hashlib
import json
import os
import platform
import statistics
import subprocess
import tempfile
import time
import zipfile
from pathlib import Path

from lxml import etree

ROOT = Path(__file__).resolve().parents[1]
HERE = ROOT / "benchmarks"
BINARY = HERE / "umya/target/release/editable-core-comparison"


def run(command):
    return subprocess.run(
        list(map(str, command)), cwd=ROOT, text=True, capture_output=True, check=True
    )


def measure(command, directory, *, decode_output=True):
    environment = dict(os.environ, TMPDIR=str(directory), CRABXL_MEASURE_CHILD_PID="1")
    process = subprocess.Popen(
        list(map(str, [HERE / "measure", *command])),
        cwd=ROOT,
        env=environment,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    peak = 0
    child_line = process.stderr.readline()
    if not child_line.startswith("CHILD_PID "):
        raise RuntimeError(f"Launcher did not report child PID: {child_line}")
    child = int(child_line.split()[1])
    while process.poll() is None:
        try:
            files = {}
            for fd in Path(f"/proc/{child}/fd").iterdir():
                try:
                    if directory.name not in os.readlink(fd):
                        continue
                    stat = fd.stat()
                    files[stat.st_dev, stat.st_ino] = stat.st_size
                except OSError:
                    pass
            peak = max(peak, sum(files.values()))
        except OSError:
            pass
        time.sleep(0.01)
    stdout, stderr = process.communicate()
    if process.returncode:
        raise RuntimeError(f"{command}: {stdout}\n{stderr}")
    measured = json.loads(stderr.split("MEASURE ")[-1])
    output = json.loads(stdout) if decode_output else stdout.strip()
    return output, {**measured, "sampled_peak_working_file_bytes": peak}


def verify_output(path, rows, writing):
    """Independently check every numeric coordinate/value outside timing."""
    namespace = "{http://schemas.openxmlformats.org/spreadsheetml/2006/main}"
    with zipfile.ZipFile(path) as archive:
        parts = sorted(
            name
            for name in archive.namelist()
            if name.startswith("xl/worksheets/") and name.endswith(".xml")
        )
        assert len(parts) == (1 if writing else 2), parts
        for sheet_index, name in enumerate(parts):
            count, checksum = 0, 0
            with archive.open(name) as source:
                for _, cell in etree.iterparse(
                    source, events=("end",), tag=namespace + "c"
                ):
                    expected = count
                    row, column = divmod(count, 10)
                    assert cell.get("r") == f"{chr(65 + column)}{row + 1}"
                    assert cell.get("t") in (None, "n")
                    text = cell.findtext(namespace + "v")
                    value = int(text)
                    if not writing and sheet_index == 0 and count == 0:
                        expected = 42
                    assert value == expected, (name, cell.get("r"), value, expected)
                    count += 1
                    checksum += value
                    cell.clear()
                    while cell.getprevious() is not None:
                        del cell.getparent()[0]
            assert count == rows * 10, (name, count)
            assert checksum == count * (count - 1) // 2 + (
                42 if not writing and sheet_index == 0 else 0
            )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rows", nargs="+", type=int, default=[1000, 10000, 100000])
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--baseline-binary", type=Path)
    parser.add_argument("--modes", nargs="+")
    parser.add_argument(
        "--output", type=Path, default=HERE / "results/alpha7-four-engines.local.json"
    )
    args = parser.parse_args()
    read_modes = [
        "crabxl-lazy",
        "umya-lazy",
        "crabxl-stream",
        "calamine-stream",
        "umya-stream",
        "crabxl-model",
        "calamine-range",
        "calamine-model",
        "umya-model",
        "umya-lazy-model",
        "crabxl-edit",
        "umya-edit",
        "umya-lazy-edit",
    ]
    write_modes = [
        "write-crabxl-stream",
        "write-rust_xlsxwriter-constant",
        "write-crabxl-model",
        "write-rust_xlsxwriter-normal",
        "write-umya-model",
    ]
    report = {
        "core_revision": run(["git", "rev-parse", "HEAD"]).stdout.strip(),
        "versions": {
            "umya-spreadsheet": "3.1.0",
            "calamine": "0.36.1",
            "rust_xlsxwriter": "0.99.1",
            "rust": run(["rustc", "--version"]).stdout.strip(),
        },
        "platform": platform.platform(),
        "binary_sha256": hashlib.sha256(BINARY.read_bytes()).hexdigest(),
        "baseline_binary_sha256": hashlib.sha256(
            args.baseline_binary.read_bytes()
        ).hexdigest()
        if args.baseline_binary
        else None,
        "cpu_quota": Path("/sys/fs/cgroup/cpu.max").read_text().strip(),
        "memory_ceiling": Path("/sys/fs/cgroup/memory.max").read_text().strip(),
        "method": "one warmup per mode and scale; rotating serial release samples; native wait4; generation/build/output verification excluded; no baseline subtraction",
        "boundaries": "Read: two numeric sheets, sum all cells, complete model modes versus noneditable calamine ranges and streams separately. Lazy modes initialize only and do not read cells. Edit: both engines materialize and sum all sheets then replace first A1 with 42 and save. Write: one numeric sheet built from zero, ordinary owned model versus sequential writer modes separately. Different preservation, rich styles, formulas and unsupported features are not compared.",
        "temporary_sampling": "10ms lower-bound sum of unique process fds under isolated TMPDIR/output directory, including spools and adjacent/final ZIP during write; completed output bytes recorded separately; no exact managed-memory comparison",
        "cases": [],
    }
    for rows in args.rows:
        source = HERE / "data" / f"four-engines-{rows}.xlsx"
        source.parent.mkdir(exist_ok=True)
        run([ROOT / "target/release/examples/workbook_demo", rows, source])
        case = {
            "rows_per_sheet": rows,
            "columns": 10,
            "read_sheets": 2,
            "source_bytes": source.stat().st_size,
            "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
            "samples": {},
        }
        count = rows * 10
        with tempfile.TemporaryDirectory(
            prefix="crabxl-engine-benchmark-"
        ) as temporary:
            directory = Path(temporary)
            modes = args.modes or read_modes + write_modes
            if any(mode not in read_modes + write_modes for mode in modes):
                raise ValueError("Unknown benchmark mode")
            variants = [(mode, BINARY, mode) for mode in modes]
            if args.baseline_binary:
                variants += [
                    ("before:" + mode, args.baseline_binary, mode) for mode in modes
                ]
            for repeat in range(args.runs + 1):
                offset = repeat % len(variants)
                for label, binary, mode in variants[offset:] + variants[:offset]:
                    target = directory / "output.xlsx"
                    output, measured = measure(
                        [binary, source, mode, target, rows], directory
                    )
                    lazy = mode.endswith("-lazy")
                    writing = mode.startswith("write-")
                    expected_count = 0 if lazy else count if writing else count * 2
                    expected_sum = (
                        0
                        if lazy
                        else count * (count - 1) // 2
                        if writing
                        else count * (count - 1)
                    )
                    assert (
                        output["cells"] == expected_count
                        and output["checksum"] == expected_sum
                    ), output
                    assert output["sheets"] == (1 if writing else 2), output
                    if output["output_bytes"]:
                        verify_output(target, rows, writing)
                        assert output["output_bytes"] == target.stat().st_size
                        measured["output_bytes"] = target.stat().st_size
                        target.unlink()
                    assert not list(directory.iterdir()), list(directory.iterdir())
                    if repeat:
                        case["samples"].setdefault(label, []).append(measured)
                    print(
                        json.dumps(
                            {"rows": rows, "mode": label, "repeat": repeat, **measured}
                        ),
                        flush=True,
                    )
        case["medians"] = {
            mode: {
                key: statistics.median(s[key] for s in samples) for key in samples[0]
            }
            for mode, samples in case["samples"].items()
        }
        report["cases"].append(case)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
