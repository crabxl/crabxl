"""Verify formula annotation retention and ordinary-mode native regression."""
import argparse
import hashlib
import json
from pathlib import Path
import statistics
import subprocess
import tempfile

from dynamic_formula_checkpoint import generate


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prior", type=Path, required=True)
    parser.add_argument("--current", type=Path, required=True)
    parser.add_argument("--retained", type=Path, required=True)
    parser.add_argument("--measure", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    commands = {"prior_projection": args.prior, "current_projection": args.current, "current_retention": args.retained}
    report = {
        "baseline_core": "ca40f674df0f97939821182c1e1f8c137b5ed3ae",
        "scope": "One warmup and three rotating serial Linux native release samples. "
        "Projection modes both verify two visible cells per row, including every "
        "array/cache/range. Retention explicitly selects only column A and verifies "
        "every cm reference; it does different work and is not a speed comparison. "
        "Builds/tests/generation excluded; no worksheet materialization or temporary store.",
        "binaries": {name: hashlib.sha256(path.read_bytes()).hexdigest() for name, path in commands.items()},
        "cases": [],
    }
    with tempfile.TemporaryDirectory(prefix="crabxl-formula-annotations-") as temporary:
        for count in [10000, 100000]:
            path = Path(temporary) / f"{count}.xlsx"
            generate(path, count)
            samples = {name: [] for name in commands}
            for trial in range(4):
                for name in list(commands)[::(-1 if trial % 2 else 1)]:
                    result = subprocess.run([str(args.measure), str(commands[name]), str(path)], check=True, capture_output=True, text=True)
                    assert int(result.stdout.strip()) == count * (1 if name == "current_retention" else 2)
                    sample = json.loads(result.stderr.split("MEASURE ")[-1])
                    if trial:
                        samples[name].append(sample)
            for name, values in samples.items():
                report["cases"].append({"rows": count, "mode": name, "source_sha256": hashlib.sha256(path.read_bytes()).hexdigest(), "samples": values, "median": {key: statistics.median(v[key] for v in values) for key in ["seconds", "peak_rss_kib"]}, "temporary_store_bytes": 0})
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps([{key: case[key] for key in ["rows", "mode", "median"]} for case in report["cases"]], indent=2))


if __name__ == "__main__":
    main()
