"""Direct Python API costs for openpyxl and the optional Rust adapter."""
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
report = {"scope": "Identical supported Workbook/append and load_workbook/Cell.value/save calls; adapter is partially compatible, not a complete replacement", "measurement": "Native wait4 CPU/wall/peak RSS; one warmup and three alternating runs per engine; temp files polled at 10ms; checksums outside timing", "python": sys.version, "platform": platform.platform(), "cases": []}

def measure(command, directory):
    environment = dict(os.environ, TMPDIR=str(directory))
    process = subprocess.Popen([str(ROOT / "benchmarks/measure"), *command], env=environment, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    temporary = 0
    while process.poll() is None:
        size = 0
        for path in directory.iterdir():
            if path.suffix == ".xlsx":
                continue
            try:
                size += path.stat().st_size
            except FileNotFoundError:
                pass
        temporary = max(temporary, size)
        time.sleep(.01)
    stdout, stderr = process.communicate()
    if process.returncode:
        raise RuntimeError(stderr)
    assert stdout.strip() == "Saved"
    measured = json.loads(stderr.split("MEASURE ")[-1])
    measured["observed_temp_bytes"] = temporary
    assert all(path.suffix == ".xlsx" for path in directory.iterdir())
    return measured

for rows in (10000, 100000):
    source = ROOT / f"benchmarks/data/numbers-{rows}.xlsx"
    for workload in ("create", "edit"):
        runs = []
        with tempfile.TemporaryDirectory(prefix="crabxl-python-api-") as name:
            directory = Path(name)
            for engine in ("crabxl", "openpyxl"):
                output = directory / f"{engine}.xlsx"
                measure([sys.executable, str(ROOT / "benchmarks/python_adapter_run.py"), engine, workload, str(rows), str(source), str(output)], directory)
            for iteration in range(3):
                for engine in (("crabxl", "openpyxl") if iteration % 2 == 0 else ("openpyxl", "crabxl")):
                    output = directory / f"{engine}.xlsx"
                    measured = measure([sys.executable, str(ROOT / "benchmarks/python_adapter_run.py"), engine, workload, str(rows), str(source), str(output)], directory)
                    cells = rows * 10
                    checksum = subprocess.run([str(ROOT / "target/release/examples/sum"), str(output)], check=True, capture_output=True, text=True).stdout.strip()
                    assert checksum == f"{cells} {cells * (cells - 1) // 2 - int(workload == 'edit')}"
                    measured.update(engine=engine, output_bytes=output.stat().st_size, checksum=checksum, cleanup=True)
                    runs.append(measured)
        report["cases"].append({"rows": rows, "columns": 10, "workload": workload, "runs": runs})
        print(rows, workload, [(engine, statistics.median(run["seconds"] for run in runs if run["engine"] == engine)) for engine in ("crabxl", "openpyxl")], flush=True)
(ROOT / "benchmarks/results/python-adapter.json").write_text(json.dumps(report, indent=2) + "\n")
