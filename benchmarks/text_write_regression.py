"""Paired full-model and streaming Unicode XML text creation measurements."""

import argparse
import hashlib
import json
import statistics
import tempfile
import zipfile
from pathlib import Path

import openpyxl
from editable_engines import BINARY, measure

PATTERN = "plain \u6587\u5b57 caf\u00e9 &<> \"'\t\r\n " * 4


def verify(path, rows):
    book = openpyxl.load_workbook(path, read_only=True)
    seen = 0
    for index, row in enumerate(book.active.values):
        assert row == tuple(f"{index}:{column}:" + PATTERN for column in range(10))
        seen += 1
    book.close()
    assert seen == rows
    with zipfile.ZipFile(path) as archive:
        return {
            name: hashlib.sha256(archive.read(name)).hexdigest()
            for name in archive.namelist()
        }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", type=Path, required=True)
    parser.add_argument("--rows", nargs="+", type=int, default=[10000, 100000])
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    report = {
        "method": "One warmup and rotating serial release samples; complete "
        "generation/build/save measured. Independent openpyxl value/order and "
        "uncompressed package-part equality verification excluded from timing. "
        "No concurrent compilation, tests, profiling or other measurements.",
        "scope": "Same generated UTF-8 text, XML delimiters, quotes, tabs, "
        "carriage returns and newlines. Compare streaming and retained-model "
        "creation separately. 1 GiB owned-model allowance, writer defaults and "
        "compression backend identical. No reference parity claim.",
        "baseline_core_revision": "4f25c545684a03a879d717c27cf1bb91acd14676",
        "openpyxl": openpyxl.__version__,
        "binary_sha256": {
            label: hashlib.sha256(binary.read_bytes()).hexdigest()
            for label, binary in (("before", args.before), ("after", BINARY))
        },
        "cases": [],
    }
    variants = [
        (f"{label}:{mode}", binary, f"write-text-crabxl-{mode}")
        for mode in ("stream", "model")
        for label, binary in (("before", args.before), ("after", BINARY))
    ]
    for rows in args.rows:
        checksum = sum(
            len(f"{row}:{column}:".encode()) + len(PATTERN.encode())
            for row in range(rows)
            for column in range(10)
        )
        case = {"rows": rows, "cells": rows * 10, "samples": {}}
        with tempfile.TemporaryDirectory(prefix="crabxl-text-writes-") as temp:
            directory = Path(temp)
            outputs = {}
            for repeat in range(args.runs + 1):
                offset = repeat % len(variants)
                for label, binary, mode in variants[offset:] + variants[:offset]:
                    path = directory / f"{label.replace(':', '-')}.xlsx"
                    output, measured = measure(
                        [binary, "-", mode, path, rows], directory
                    )
                    assert output["cells"] == rows * 10
                    assert output["checksum"] == checksum
                    assert output["sheets"] == 1
                    measured["output_bytes"] = output["output_bytes"]
                    if repeat:
                        case["samples"].setdefault(label, []).append(measured)
                    outputs[label] = path
                    print(rows, label, repeat, measured, flush=True)
            hashes = {label: verify(path, rows) for label, path in outputs.items()}
            for mode in ("stream", "model"):
                assert hashes[f"before:{mode}"] == hashes[f"after:{mode}"]
            assert set(directory.iterdir()) == set(outputs.values())
            case["verified_package_part_sha256"] = {
                mode: hashes[f"after:{mode}"] for mode in ("stream", "model")
            }
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
