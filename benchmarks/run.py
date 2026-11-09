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

# Size name: (rows, columns). The xl and xxl files (about 370 MB and 1 GB) are
# read only, by the readers that can hold them in memory.
SIZES = {
    "small": (10_000, 10),
    "medium": (100_000, 10),
    "large": (1_000_000, 10),
    "xl": (1_000_000, 100),
    "xxl": (1_000_000, 280),
    # The NYC 311 sample that the calamine README uses: 1,000,000 service requests
    # and a header row, 41 columns, mostly text. See benchmarks/README.md.
    "nyc311": (1_000_001, 41),
}
HUGE = {"xl", "xxl", "nyc311"}
HUGE_READERS = ["crabxl", "calamine-stream", "crabxl-model", "calamine"]
SCENARIOS = {
    # Streaming readers first, then readers that load the whole sheet.
    "read": ["crabxl", "calamine-stream", "oxlsx", "crabxl-model", "calamine", "umya"],
    "write": ["crabxl", "rust_xlsxwriter", "rust_xlsxwriter-constant-memory", "oxlsx", "umya"],
    "edit": ["crabxl", "oxlsx", "umya"],
}
TIMEOUT = 300
SLOW_CASE = 10  # seconds; slower cases get one measured run


def slow(previous, scenario, library):
    """Whether the case at the previous size predicts a timeout (sizes grow 10x)."""
    record = previous.get((scenario, library))
    return record is not None and ("timeout" in record or record.get("seconds", 0) * 10 > TIMEOUT)


def check(record):
    """Mark a case that did not handle every expected cell as incomplete."""
    rows, columns = record["rows"], record.get("columns", 10)
    expected = {
        "read": rows * columns,
        "write": rows * columns,
        "edit": -(-rows // 100) + 1000,
    }
    want = record.get("expected_cells") or expected[record["scenario"]]
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


PINNED_CPU = 2  # Not CPU 0, which handles most interrupts.


def cpu_time(process):
    """User plus kernel CPU time of a finished child on Windows, in seconds."""
    times = [ctypes.c_ulonglong() for _ in range(4)]
    if not ctypes.windll.kernel32.GetProcessTimes(
        ctypes.c_void_p(int(process._handle)), *map(ctypes.byref, times)
    ):
        raise ctypes.WinError()
    return (times[2].value + times[3].value) / 1e7


def run(command, cpus, env=None):
    """Run one case on one pinned CPU or on all CPUs.

    Return (seconds, peak bytes, CPU seconds, stdout), or None on a timeout. A
    pinned child starts suspended and gets its CPU before it runs any code, so
    it cannot size thread pools for more CPUs.
    """
    pinned = cpus == "1"
    options = {"stdout": subprocess.PIPE, "stderr": subprocess.PIPE, "env": env}
    if pinned and os.name == "nt":
        options["creationflags"] = 0x4  # CREATE_SUSPENDED
    elif pinned and hasattr(os, "sched_setaffinity"):
        options["preexec_fn"] = lambda: os.sched_setaffinity(0, {PINNED_CPU})
    elif pinned:
        raise SystemExit("This platform cannot pin a process to one CPU.")
    process = subprocess.Popen(command, **options)
    if pinned and os.name == "nt":
        handle = ctypes.c_void_p(int(process._handle))
        if not ctypes.windll.kernel32.SetProcessAffinityMask(handle, ctypes.c_size_t(1 << PINNED_CPU)):
            process.kill()
            raise ctypes.WinError()
        ctypes.windll.ntdll.NtResumeProcess(handle)
    start = time.perf_counter()
    if os.name == "nt":
        try:
            out, err = process.communicate(timeout=TIMEOUT)
        except subprocess.TimeoutExpired:
            process.kill()
            process.communicate()
            return None
        seconds = time.perf_counter() - start
        code = process.returncode
        used = cpu_time(process)
    else:
        # shortcut: no timeout on POSIX; wait4 is the only way to read child peak RSS.
        out, err = process.stdout.read(), process.stderr.read()
        _, status, usage = os.wait4(process.pid, 0)
        seconds = time.perf_counter() - start
        code = os.waitstatus_to_exitcode(status)
        process.returncode = code
        scale = 1 if sys.platform == "darwin" else 1024
        process.peak = usage.ru_maxrss * scale
        used = usage.ru_utime + usage.ru_stime
    if code != 0:
        raise RuntimeError(f"{command} failed: {err.decode(errors='replace')}")
    return seconds, peak_memory(process), used, out.decode().strip()


def cases(sizes):
    for size in sizes:
        rows, columns = SIZES[size]
        scenarios = {"read": HUGE_READERS} if size in HUGE else SCENARIOS
        for scenario, libraries in scenarios.items():
            for library in libraries:
                yield size, rows, columns, scenario, library


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--sizes", nargs="+", choices=SIZES, default=list(SIZES))
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument(
        "--cpus",
        nargs="+",
        choices=["all", "1"],
        default=["all", "1"],
        help="run each case on all CPUs, on one pinned CPU, or both",
    )
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
        if size == "nyc311" and not fixture.exists():
            raise SystemExit(
                f"{fixture} is missing. Build it as described in benchmarks/README.md."
            )
        if not fixture.exists():
            rows, columns = SIZES[size]
            subprocess.run([EXE, "fixture", str(rows), fixture, str(columns)], check=True)

    results = []
    previous = {}
    every = [(cpus, case) for cpus in args.cpus for case in cases(args.sizes)]
    for cpus, (size, rows, columns, scenario, library) in every:
        fixture = work / f"{size}.xlsx"
        output = work / f"out-{library}.xlsx"
        command = {
            "read": [EXE, library, "read", fixture],
            "write": [EXE, library, "write", str(rows), output],
            "edit": [EXE, library, "edit", fixture, output, str(rows)],
        }[scenario]
        runs = []
        record = {
            "size": size,
            "rows": rows,
            "columns": columns,
            "file_mib": fixture.stat().st_size / 2**20,
            "scenario": scenario,
            "library": library,
            "cpus": cpus,
        }
        if size == "nyc311":
            # The CSV has empty fields; readers count non-empty cells.
            record["expected_cells"] = int((work / "nyc311.cells").read_text())
        if slow(previous, (cpus, scenario), library):
            record["timeout"] = TIMEOUT
            record["estimated"] = True
        # One extra first run warms the file cache; it is not counted. A case
        # slower than SLOW_CASE seconds gets one measured run instead of several.
        wanted, attempt = args.runs, 0
        while "timeout" not in record and attempt <= wanted:
            try:
                measured = run(command, cpus)
            except RuntimeError as error:
                record["error"] = str(error).strip().splitlines()[-1]
                break
            if measured is None:
                record["timeout"] = TIMEOUT
                break
            if attempt:
                runs.append(measured)
            elif measured[0] > SLOW_CASE:
                wanted = 1
            attempt += 1
        if runs and len(runs) == wanted:
            record["runs"] = wanted
            record["seconds"] = statistics.median(r[0] for r in runs)
            record["peak_mib"] = statistics.median(r[1] for r in runs) / 2**20
            record["cpu_seconds"] = statistics.median(r[2] for r in runs)
            record["cells"] = int(runs[0][3])
        results.append(check(record))
        previous[((cpus, scenario), library)] = record
        print(json.dumps(record), flush=True)
        output.unlink(missing_ok=True)

    report = {
        "machine": f"{platform.system()} {platform.release()}, {platform.processor() or platform.machine()}",
        "cpus": os.cpu_count(),
        "runs": args.runs,
        "results": results,
    }
    RESULTS.mkdir(exist_ok=True)
    # Keep earlier results for the sizes and CPU modes that this run did not measure.
    saved = RESULTS / "results.json"
    if saved.exists():
        measured = {(size, cpus) for size in args.sizes for cpus in args.cpus}
        kept = [
            r
            for r in json.loads(saved.read_text())["results"]
            if (r["size"], r.get("cpus", "all")) not in measured
        ]
        order = list(SIZES)
        report["results"] = sorted(
            kept + results, key=lambda r: (r.get("cpus", "all") == "1", order.index(r["size"]))
        )
    saved.write_text(json.dumps(report, indent=1) + "\n")
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
        key = (record.get("cpus", "all"), record["scenario"], record["size"])
        groups.setdefault(key, []).append(record)
    for cpus in ("all", "1"):
        if not any(key[0] == cpus for key in groups):
            continue
        lines += [
            "# All CPUs" if cpus == "all" else "# One CPU",
            "",
            "Every library can use all logical CPUs."
            if cpus == "all"
            else "Every library is pinned to the same single logical CPU.",
            "",
        ]
        for (mode, scenario, size), records in groups.items():
            if mode != cpus:
                continue
            first = records[0]
            shape = f"{first['rows']:,} rows × {first.get('columns', 10)} columns"
            if scenario != "write" and "file_mib" in first:
                shape += f", {first['file_mib']:,.0f} MiB file"
            name = f"{scenario}-{size}" + ("" if cpus == "all" else "-1cpu")
            lines += [
                f"## {scenario.capitalize()}, {size} ({shape})",
                "",
                f"![{scenario} {size}]({name}.svg)",
                "",
                "| Library | Time (s) | CPU time (s) | Peak memory (MiB) |",
                "|---|---:|---:|---:|",
            ]
            for record in records:
                if "timeout" in record:
                    note = " (estimated from the smaller size)" if record.get("estimated") else ""
                    lines.append(f"| {record['library']} | > {record['timeout']}{note} | – | – |")
                elif "error" in record:
                    lines.append(f"| {record['library']} | failed: {record['error']} | – | – |")
                else:
                    used = record.get("cpu_seconds")
                    used = "–" if used is None else f"{used:.3f}"
                    lines.append(
                        f"| {record['library']} | {record['seconds']:.3f} | {used} "
                        f"| {record['peak_mib']:.1f} |"
                    )
            lines.append("")
            (RESULTS / f"{name}.svg").write_text(chart(records))
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
