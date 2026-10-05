"""Measure native zlib-rs/zlib ZIP levels with identical uncompressed payloads."""

import argparse
import hashlib
import json
import statistics
import subprocess
import tempfile
import zipfile
from pathlib import Path


def payloads(path):
    result = {}
    with zipfile.ZipFile(path) as archive:
        for name in archive.namelist():
            digest = hashlib.sha256()
            with archive.open(name) as part:
                while data := part.read(65536):
                    digest.update(data)
            result[name] = digest.hexdigest()
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary-prefix", type=Path, default=Path("/tmp/crabxl-alpha4"))
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--measure", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--rows", type=int, default=100000)
    parser.add_argument("--runs", type=int, default=3)
    args = parser.parse_args()
    report = {
        "scope": "One warmup and rotating serial native release samples; numeric/mixed "
        "creation and existing A1 replacement/save. Build/tests/readback excluded. "
        "Level 0 stores ZIP entries, 1..9 use Deflate. Full uncompressed-part hashes "
        "and CRC readback are verified across each workload's outputs.",
        "rows": args.rows, "columns": 10,
        "source_sha256": hashlib.sha256(args.source.read_bytes()).hexdigest(),
        "cases": [],
    }
    with tempfile.TemporaryDirectory(prefix="crabxl-compression-") as temp:
        path = Path(temp) / "output.xlsx"
        for workload in ["numeric", "mixed", "edit"]:
            baseline = None
            cases = [(backend, level) for backend in ["rs", "zlib"] for level in [0, 1, 3, 6, 9]]
            samples = {case: [] for case in cases}
            for trial in range(args.runs + 1):
                for backend, level in cases[::(-1 if trial % 2 else 1)]:
                    binary = Path(f"{args.binary_prefix}-{backend}-{'edit' if workload == 'edit' else 'write'}")
                    command = ([str(binary), str(args.source), str(path), "edit", str(level)]
                               if workload == "edit" else
                               [str(binary), str(path), str(args.rows), workload, "-", str(level)])
                    result = subprocess.run([str(args.measure), *command], check=True, capture_output=True, text=True)
                    sample = json.loads(result.stderr.split("MEASURE ")[-1])
                    sample.update(output_bytes=path.stat().st_size, stats=result.stdout.strip())
                    if trial:
                        samples[(backend, level)].append(sample)
                    if trial == args.runs:
                        hashes = payloads(path)
                        if baseline is None:
                            baseline = hashes
                        assert hashes == baseline, (workload, backend, level)
                print(workload, "trial", trial, "passed", flush=True)
            for (backend, level), values in samples.items():
                report["cases"].append({
                    "workload": workload, "backend": backend, "level": level,
                    "samples": values,
                    "median": {key: statistics.median(v[key] for v in values)
                               for key in ["seconds", "peak_rss_kib", "output_bytes"]},
                    "all_uncompressed_parts_verified": True,
                })
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
