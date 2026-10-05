"""Compare alpha.2 and buffered edit/save using serial native measurements."""

import argparse
import hashlib
import json
import statistics
import subprocess
import tempfile
import zipfile
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", type=Path, required=True)
    parser.add_argument("--after", type=Path, required=True)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--measure", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--runs", type=int, default=3)
    args = parser.parse_args()
    report = {
        "scope": "Existing A1 integer replacement and save; one warmup followed "
        "by rotating serial samples. All original package semantics, including "
        "global formula-cache invalidation, remain enabled. Validation is outside timing.",
        "source_sha256": hashlib.sha256(args.source.read_bytes()).hexdigest(),
        "binary_sha256": {
            name: hashlib.sha256(path.read_bytes()).hexdigest()
            for name, path in [("alpha2", args.before), ("candidate", args.after)]
        },
        "samples": {"alpha2": [], "candidate": []},
    }
    with tempfile.TemporaryDirectory(prefix="crabxl-edit-buffering-") as temp:
        outputs = {name: Path(temp) / f"{name}.xlsx" for name in report["samples"]}
        binaries = {"alpha2": args.before, "candidate": args.after}
        for trial in range(args.runs + 1):
            for name in list(binaries)[::(-1 if trial % 2 else 1)]:
                result = subprocess.run(
                    [str(args.measure.resolve()), str(binaries[name].resolve()),
                     str(args.source.resolve()), str(outputs[name]), "edit"],
                    check=True, capture_output=True, text=True,
                )
                sample = json.loads(result.stderr.split("MEASURE ")[-1])
                sample["save_stats"] = result.stdout.strip()
                sample["output_bytes"] = outputs[name].stat().st_size
                if trial:
                    report["samples"][name].append(sample)
        with zipfile.ZipFile(outputs["alpha2"]) as before, zipfile.ZipFile(outputs["candidate"]) as after:
            assert before.namelist() == after.namelist()
            for part in before.namelist():
                assert before.read(part) == after.read(part), part
        report["all_uncompressed_parts_identical"] = True
    report["median"] = {
        name: {key: statistics.median(sample[key] for sample in samples)
               for key in ["seconds", "cpu_seconds", "peak_rss_kib", "output_bytes"]}
        for name, samples in report["samples"].items()
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report["median"], indent=2))


if __name__ == "__main__":
    main()
