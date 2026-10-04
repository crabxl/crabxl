"""Release formula translation and same-call writer regression evidence."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--before-python-path", type=Path, required=True, help="Snapshot containing the 1ad552d crabxl Python package")
args = parser.parse_args()
report = {"scope": "M5 A1 scanner/translated sparse moves checkpoint, not complete tokenizer or common-feature milestone", "measurement": "Native wait4 wall/CPU/RSS; one warmup and three alternating runs; temp polling at 10ms for writer cases", "baseline": "openpyxl 3.1.5; writer regression versus adapter 1ad552d", "cases": []}

def measured(command, environment=None, directory=None):
    process = subprocess.Popen([str(ROOT / "benchmarks/measure"), *command], stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=environment, text=True)
    temporary = 0
    while process.poll() is None:
        if directory:
            size = 0
            for path in directory.iterdir():
                if path.suffix == ".xlsx": continue
                try: size += path.stat().st_size
                except FileNotFoundError: pass
            temporary = max(temporary, size)
        time.sleep(.01)
    output, errors = process.communicate()
    if process.returncode: raise RuntimeError(errors)
    run = json.loads(errors.split("MEASURE ")[-1])
    run.update(result=output.strip(), observed_temp_bytes=temporary)
    return run

expected = '=SUM(B2:C3)+\'A1\'!$C4+T1[A1]+LOG10(E5)+"A1"'
for count in (10000, 100000):
    for mode in ("construct", "reuse"):
        commands = {engine: [sys.executable, str(ROOT / "benchmarks/formula_adapter_run.py"), engine, str(count), mode] for engine in ("crabxl", "openpyxl")}
        runs = []
        for command in commands.values(): measured(command)
        for iteration in range(3):
            for engine in (list(commands) if iteration % 2 == 0 else list(reversed(commands))):
                run = measured(commands[engine])
                assert run["result"] == f"{count} {count * len(expected)} {hashlib.sha256(expected.encode()).hexdigest()}"
                run["engine"] = engine
                runs.append(run)
        report["cases"].append({"workload": "translate", "mode": mode, "calls": count, "runs": runs, "temporary_bytes": 0})
        print(count, mode, [(engine, statistics.median(run["seconds"] for run in runs if run["engine"] == engine)) for engine in commands], flush=True)
for rows in (10000, 100000):
    runs = []
    with tempfile.TemporaryDirectory(prefix="crabxl-formula-writer-") as name:
        directory = Path(name)
        output = directory / "output.xlsx"
        command = [sys.executable, str(ROOT / "benchmarks/python_adapter_run.py"), "crabxl", "create", str(rows), "unused", str(output)]
        environments = {"before": dict(os.environ, PYTHONPATH=str(args.before_python_path), TMPDIR=str(directory)), "current": dict(os.environ, TMPDIR=str(directory))}
        for environment in environments.values(): measured(command, environment, directory)
        for iteration in range(3):
            for version in (list(environments) if iteration % 2 == 0 else list(reversed(environments))):
                run = measured(command, environments[version], directory)
                assert run.pop("result") == "Saved"
                cells = rows * 10
                checksum = subprocess.run([str(ROOT / "target/release/examples/sum"), str(output)], check=True, capture_output=True, text=True).stdout.strip()
                assert checksum == f"{cells} {cells * (cells - 1) // 2}"
                run.update(version=version, output_bytes=output.stat().st_size, checksum=checksum, cleanup=all(path.suffix == ".xlsx" for path in directory.iterdir()))
                assert run["cleanup"]
                runs.append(run)
    report["cases"].append({"workload": "writer-regression", "rows": rows, "columns": 10, "runs": runs})
    print(rows, "writer regression checksum and cleanup passed", flush=True)
(ROOT / "benchmarks/results/m5-formula.json").write_text(json.dumps(report, indent=2) + "\n")
