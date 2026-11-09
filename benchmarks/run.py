"""Run the CrabXL benchmark suite against other Rust XLSX libraries.

Each case runs in its own process. The parent measures wall time and the peak
resident memory of that process. Results go to benchmarks/results/:
results.json, results.md and one SVG chart per scenario and size.

    python benchmarks/run.py                 # all sizes, 3 runs per case
    python benchmarks/run.py --sizes small --runs 1
"""

import argparse
import ctypes
import json
import os
import platform
import statistics
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent
DRIVER = ROOT / "driver"
EXE = DRIVER / "target/release" / ("driver.exe" if os.name == "nt" else "driver")
RESULTS = ROOT / "results"

SIZES = {"small": 10_000, "medium": 100_000, "large": 1_000_000}
SCENARIOS = {
    # Streaming readers first, then readers that load the whole sheet.
    "read": ["crabxl", "calamine-stream", "oxlsx", "crabxl-model", "calamine", "umya"],
    "write": ["crabxl", "rust_xlsxwriter", "rust_xlsxwriter-constant-memory", "oxlsx", "umya"],
    "edit": ["crabxl", "oxlsx", "umya"],
}
TIMEOUT = 900


def slow(previous, scenario, library):
    """Whether the case at the previous size predicts a timeout (sizes grow 10x)."""
    record = previous.get((scenario, library))
    return record is not None and ("timeout" in record or record.get("seconds", 0) * 10 > TIMEOUT)


def check(record):
    """Mark a case that did not handle every expected cell as incomplete."""
    rows = record["rows"]
    expected = {"read": rows * 10, "write": rows * 10, "edit": -(-rows // 100) + 1000}
    want = expected[record["scenario"]]
    if "cells" in record and record["cells"] != want:
        record["error"] = f"incomplete: handled {record['cells']:,} of {want:,} cells"
        for key in ("seconds", "peak_mib"):
            record.pop(key, None)
    return record


def peak_memory(process):
    """Peak resident memory of a finished child, in bytes."""
    if os.name == "nt":

        class Counters(ctypes.Structure):
            _fields_ = [("cb", ctypes.c_ulong), ("faults", ctypes.c_ulong)] + [
                (name, ctypes.c_size_t)
                for name in ("peak", "rss", "a", "b", "c", "d", "e", "f")
            ]

        counters = Counters(cb=ctypes.sizeof(Counters))
        # The Popen handle stays open after exit, so the counters are still readable.
        if not ctypes.windll.psapi.GetProcessMemoryInfo(
            ctypes.c_void_p(int(process._handle)), ctypes.byref(counters), counters.cb
        ):
            raise ctypes.WinError()
        return counters.peak
    return process.peak  # set by run() from wait4


def run(command):
    """Run one case. Return (seconds, peak bytes, stdout) or None on timeout."""
    start = time.perf_counter()
    process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if os.name == "nt":
        try:
            out, err = process.communicate(timeout=TIMEOUT)
        except subprocess.TimeoutExpired:
            process.kill()
            process.communicate()
            return None
        seconds = time.perf_counter() - start
        code = process.returncode
    else:
        # shortcut: no timeout on POSIX; wait4 is the only way to read child peak RSS.
        out, err = process.stdout.read(), process.stderr.read()
        _, status, usage = os.wait4(process.pid, 0)
        seconds = time.perf_counter() - start
        code = os.waitstatus_to_exitcode(status)
        process.returncode = code
        scale = 1 if sys.platform == "darwin" else 1024
        process.peak = usage.ru_maxrss * scale
    if code != 0:
        raise RuntimeError(f"{command} failed: {err.decode(errors='replace')}")
    return seconds, peak_memory(process), out.decode().strip()


def cases(sizes):
    for size in sizes:
        rows = SIZES[size]
        for scenario, libraries in SCENARIOS.items():
            for library in libraries:
                yield size, rows, scenario, library


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--sizes", nargs="+", choices=SIZES, default=list(SIZES))
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument(
        "--report-only", action="store_true", help="rewrite results.md from results.json"
    )
    args = parser.parse_args()
    if args.report_only:
        report = json.loads((RESULTS / "results.json").read_text())
        report["results"] = [check(record) for record in report["results"]]
        (RESULTS / "results.json").write_text(json.dumps(report, indent=1) + "\n")
        write_report(report, "Rust")
        return

    subprocess.run(
        ["cargo", "build", "--release", "--quiet", "--manifest-path", DRIVER / "Cargo.toml"],
        check=True,
    )
    work = ROOT / "data"
    work.mkdir(exist_ok=True)
    for size in args.sizes:
        fixture = work / f"{size}.xlsx"
        if not fixture.exists():
            subprocess.run([EXE, "fixture", str(SIZES[size]), fixture], check=True)

    results = []
    previous = {}
    for size, rows, scenario, library in cases(args.sizes):
        fixture = work / f"{size}.xlsx"
        output = work / f"out-{library}.xlsx"
        command = {
            "read": [EXE, library, "read", fixture],
            "write": [EXE, library, "write", str(rows), output],
            "edit": [EXE, library, "edit", fixture, output, str(rows)],
        }[scenario]
        runs = []
        record = {"size": size, "rows": rows, "scenario": scenario, "library": library}
        if slow(previous, scenario, library):
            record["timeout"] = TIMEOUT
            record["estimated"] = True
        # One extra first run warms the file cache; it is not counted.
        for attempt in range(0 if "timeout" in record else args.runs + 1):
            try:
                measured = run(command)
            except RuntimeError as error:
                record["error"] = str(error).strip().splitlines()[-1]
                break
            if measured is None:
                record["timeout"] = TIMEOUT
                break
            if attempt:
                runs.append(measured)
        if len(runs) == args.runs:
            record["seconds"] = statistics.median(r[0] for r in runs)
            record["peak_mib"] = statistics.median(r[1] for r in runs) / 2**20
            record["cells"] = int(runs[0][2])
        results.append(check(record))
        previous[(scenario, library)] = record
        print(json.dumps(record), flush=True)
        output.unlink(missing_ok=True)

    report = {
        "machine": f"{platform.system()} {platform.release()}, {platform.processor() or platform.machine()}",
        "cpus": os.cpu_count(),
        "runs": args.runs,
        "results": results,
    }
    RESULTS.mkdir(exist_ok=True)
    (RESULTS / "results.json").write_text(json.dumps(report, indent=1) + "\n")
    write_report(report, "Rust")


# ---------------------------------------------------------------- report
# The same report code is used by the Python suite in crabxl-python.


def write_report(report, title):
    lines = [
        f"# {title} benchmark results",
        "",
        f"Machine: {report['machine']}, {report['cpus']} logical CPUs. "
        f"Median of {report['runs']} runs. Time is the wall time of the whole "
        "process. Memory is the peak resident memory of the process.",
        "",
    ]
    groups = {}
    for record in report["results"]:
        groups.setdefault((record["scenario"], record["size"]), []).append(record)
    for (scenario, size), records in groups.items():
        rows = records[0]["rows"]
        lines += [
            f"## {scenario.capitalize()}, {size} ({rows:,} rows × 10 columns)",
            "",
            f"![{scenario} {size}]({scenario}-{size}.svg)",
            "",
            "| Library | Time (s) | Peak memory (MiB) |",
            "|---|---:|---:|",
        ]
        for record in records:
            if "timeout" in record:
                note = " (estimated from the smaller size)" if record.get("estimated") else ""
                lines.append(f"| {record['library']} | > {record['timeout']}{note} | – |")
            elif "error" in record:
                lines.append(f"| {record['library']} | failed: {record['error']} | – |")
            else:
                lines.append(
                    f"| {record['library']} | {record['seconds']:.3f} | {record['peak_mib']:.1f} |"
                )
        lines.append("")
        (RESULTS / f"{scenario}-{size}.svg").write_text(chart(records))
    (RESULTS / "results.md").write_text("\n".join(lines))


def chart(records):
    """Two horizontal bar panels (time and memory) for one scenario and size."""
    done = [r for r in records if "seconds" in r]
    label_width, panel, bar, gap = 230, 300, 22, 8
    height = 40 + len(records) * (bar + gap)
    width = label_width + 2 * panel + 60
    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" '
        f'viewBox="0 0 {width} {height}" font-family="system-ui, sans-serif" font-size="13">',
        "<style>text{fill:#334}.t{font-weight:600}.crabxl{fill:#e8590c}.other{fill:#8a94a6}"
        "@media (prefers-color-scheme: dark){text{fill:#dde}.other{fill:#6b7385}}</style>",
    ]
    for index, (key, name, unit) in enumerate(
        [("seconds", "Time", "s"), ("peak_mib", "Peak memory", "MiB")]
    ):
        x0 = label_width + index * (panel + 30)
        top = max((r[key] for r in done), default=1) or 1
        parts.append(f'<text class="t" x="{x0}" y="20">{name} ({unit}, lower is better)</text>')
        for row, record in enumerate(records):
            y = 34 + row * (bar + gap)
            if index == 0:
                parts.append(
                    f'<text x="{label_width - 10}" y="{y + 16}" text-anchor="end">{record["library"]}</text>'
                )
            if "seconds" not in record:
                parts.append(f'<text x="{x0}" y="{y + 16}">{"timeout" if "timeout" in record else "failed"}</text>')
                continue
            length = max(2, record[key] / top * (panel - 70))
            css = "crabxl" if record["library"].startswith("crabxl") else "other"
            value = f"{record[key]:.2f}" if key == "seconds" else f"{record[key]:.0f}"
            parts.append(f'<rect class="{css}" x="{x0}" y="{y}" width="{length:.1f}" height="{bar}" rx="3"/>')
            parts.append(f'<text x="{x0 + length + 6:.1f}" y="{y + 16}">{value}</text>')
    parts.append("</svg>")
    return "\n".join(parts) + "\n"


if __name__ == "__main__":
    main()
