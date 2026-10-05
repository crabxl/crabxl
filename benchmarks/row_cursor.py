"""Compare retained public-model row traversal; no XLSX or binding timer."""

import argparse
import hashlib
import json
import statistics
import tempfile
from pathlib import Path

from editable_engines import measure


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", type=Path, required=True)
    parser.add_argument("--after", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--rows", type=int, nargs="+", default=[10000, 100000])
    parser.add_argument("--runs", type=int, default=3)
    args = parser.parse_args()
    report = {
        "method": "Identical pure public-core probe, release Rust 1.99/flags/dependencies. One warmup and rotating serial wait4 samples, five traversals per worker. Full process includes canonical model creation/destruction; traverse_seconds separately includes every-value/sparse-coordinate verification. No XLSX, Python conversion, builds/tests/profile overlap or source clones.",
        "prior_core": "a1c11a9 (same canonical core as 9b5311b)",
        "binary_sha256": {
            label: hashlib.sha256(binary.read_bytes()).hexdigest()
            for label, binary in (("before", args.before), ("after", args.after))
        },
        "probe_sha256": hashlib.sha256(
            Path(__file__).with_suffix(".rs").read_bytes()
        ).hexdigest(),
        "resource_settings": "1 GiB model allowance and physical cell limit; managed ledger remains 256 bytes per cell plus owned payload/name, not an RSS cap.",
        "cases": [],
    }
    for rows in args.rows:
        for shape in ("dense", "sparse"):
            variants = [
                (label + ":" + mode, binary, mode)
                for mode in ("row", "cell")
                for label, binary in (("before", args.before), ("after", args.after))
            ]
            samples = {}
            with tempfile.TemporaryDirectory(prefix="crabxl-row-cursor-") as temporary:
                directory = Path(temporary)
                for repeat in range(args.runs + 1):
                    offset = repeat % len(variants)
                    for label, binary, mode in variants[offset:] + variants[:offset]:
                        output, measured = measure(
                            [binary, rows, shape, mode], directory
                        )
                        count = rows * 10
                        assert output["cells"] == count * 5
                        assert output["checksum"] == count * (count - 1) // 2 * 5
                        assert output["model_bytes"] == count * 256 + 4
                        assert not list(directory.iterdir())
                        measured["traverse_seconds"] = output["traverse_seconds"]
                        if repeat:
                            samples.setdefault(label, []).append(measured)
                        print(
                            json.dumps(
                                {
                                    "rows": rows,
                                    "shape": shape,
                                    "label": label,
                                    "repeat": repeat,
                                    **measured,
                                }
                            ),
                            flush=True,
                        )
            report["cases"].append(
                {
                    "rows": rows,
                    "shape": shape,
                    "samples": samples,
                    "medians": {
                        label: {
                            key: statistics.median(value[key] for value in values)
                            for key in values[0]
                        }
                        for label, values in samples.items()
                    },
                }
            )
    args.output.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
