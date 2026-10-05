"""Compare exact prior/current XML routes with existing value-verifying probes."""

import argparse
import hashlib
import json
import statistics
import tempfile
from pathlib import Path

from editable_engines import HERE, measure
from shared_strings_checkpoint import generate as strings
from styled_checkpoint import generate as styled


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", required=True, type=Path)
    parser.add_argument("--after", required=True, type=Path)
    parser.add_argument("--rows", nargs="+", type=int, default=[10000, 100000])
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    report = {
        "method": "One warmup then alternating serial wait4 samples; builds/generation excluded; existing probes verify every value; temporary descriptor sampling every 10ms",
        "binary_sha256": {
            label: {
                name: hashlib.sha256((directory / name).read_bytes()).hexdigest()
                for name in ("shared_text", "styled_read")
            }
            for label, directory in (("before", args.before), ("after", args.after))
        },
        "cases": [],
    }
    for rows in args.rows:
        for kind, unique in (
            ("repeated-text", 128),
            ("unique-text", rows * 10),
            ("styled", 0),
        ):
            source = HERE / "data" / f"xml-scope-{kind}-{rows}.xlsx"
            if unique:
                strings(source, rows, unique)
            else:
                styled(source, rows)
            case = {
                "rows": rows,
                "kind": kind,
                "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
                "samples": {},
            }
            with tempfile.TemporaryDirectory(prefix="crabxl-xml-scope-") as temporary:
                directory = Path(temporary)
                policies = ("memory", "disk") if unique else ("styled",)
                variants = [
                    (label + ":" + policy, binary, policy)
                    for policy in policies
                    for label, binary in (
                        ("before", args.before),
                        ("after", args.after),
                    )
                ]
                for repeat in range(args.runs + 1):
                    offset = repeat % len(variants)
                    for label, binary, policy in variants[offset:] + variants[:offset]:
                        command = (
                            [binary / "shared_text", source, policy, unique, directory]
                            if unique
                            else [binary / "styled_read", source]
                        )
                        output, measured = measure(
                            command, directory, decode_output=False
                        )
                        expected = (
                            f"{rows * 10} {rows * 10 * 110}"
                            if unique
                            else str(rows * 10)
                        )
                        assert output == expected, (label, output, expected)
                        assert not list(directory.iterdir()), list(directory.iterdir())
                        if repeat:
                            case["samples"].setdefault(label, []).append(measured)
                        print(
                            json.dumps(
                                {
                                    "kind": kind,
                                    "rows": rows,
                                    "label": label,
                                    "repeat": repeat,
                                    **measured,
                                }
                            ),
                            flush=True,
                        )
            case["medians"] = {
                label: {
                    key: statistics.median(sample[key] for sample in samples)
                    for key in samples[0]
                }
                for label, samples in case["samples"].items()
            }
            report["cases"].append(case)
            args.output.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
