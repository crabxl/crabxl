"""Measure two independent-sheet scans with concurrency-aware Auto budgets."""
import argparse
import hashlib
import json
from pathlib import Path
import statistics
import subprocess
import tempfile
import zipfile

from shared_strings_checkpoint import generate


def multisheet(path, rows, unique):
    seed = path.with_suffix(".seed.xlsx")
    generate(seed, rows, unique)
    with zipfile.ZipFile(seed) as original, zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as output:
        for name in original.namelist():
            if name == "[Content_Types].xml":
                text = original.read(name).decode().replace("</Types>", '<Override PartName="/xl/worksheets/sheet2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>')
                output.writestr(name, text)
            elif name == "xl/workbook.xml":
                text = original.read(name).decode().replace("</sheets>", '<sheet name="Other" sheetId="2" r:id="other"/></sheets>')
                output.writestr(name, text)
            elif name == "xl/_rels/workbook.xml.rels":
                text = original.read(name).decode().replace("</Relationships>", '<Relationship Id="other" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet2.xml"/></Relationships>')
                output.writestr(name, text)
            else:
                with original.open(name) as source, output.open(name, "w") as target:
                    while data := source.read(65536):
                        target.write(data)
        with original.open("xl/worksheets/sheet1.xml") as source, output.open("xl/worksheets/sheet2.xml", "w") as target:
            while data := source.read(65536):
                target.write(data)
    seed.unlink()
    with zipfile.ZipFile(path) as archive:
        assert archive.testzip() is None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--measure", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    report = {
        "scope": "Two sheets sharing one SST, each read through an independent native "
        "reader. One warmup and three rotating serial samples. Serial executes two "
        "scans sequentially with concurrent_operations=1; parallel uses two threads "
        "and concurrent_operations=2. Same complete values/order are verified. "
        "Availability is a controlled caller override, not live host probing. "
        "Per-reader catalogs/SSTs are duplicated; no global reservation or hard RSS cap.",
        "binary_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest(),
        "cpu_quota": Path("/sys/fs/cgroup/cpu.max").read_text().strip(),
        "memory_ceiling": Path("/sys/fs/cgroup/memory.max").read_text().strip(),
        "cases": [],
    }
    with tempfile.TemporaryDirectory(prefix="crabxl-concurrent-auto-") as temporary:
        directory = Path(temporary)
        store = directory / "store"
        store.mkdir()
        for rows in [10000, 100000]:
            for cardinality, unique in [("repeated", 128), ("unique", rows * 10)]:
                path = directory / f"{rows}-{cardinality}.xlsx"
                multisheet(path, rows, unique)
                digest = hashlib.sha256(path.read_bytes()).hexdigest()
                for available in [64, 1024]:
                    samples = {"serial": [], "parallel": []}
                    for trial in range(4):
                        for mode in list(samples)[::(-1 if trial % 2 else 1)]:
                            result = subprocess.run([str(args.measure), str(args.binary), str(path), mode, str(unique), str(available), str(store)], check=True, capture_output=True, text=True)
                            readers = [json.loads(line) for line in result.stdout.splitlines()]
                            assert len(readers) == 2 and all(reader["cells"] == rows * 10 for reader in readers)
                            assert all(reader["budget_bytes"] == available * 1024 * 1024 // (4 if mode == "serial" else 8) for reader in readers)
                            assert not list(store.iterdir())
                            sample = json.loads(result.stderr.split("MEASURE ")[-1])
                            sample["readers"] = readers
                            sample["logical_temp_upper_bound_bytes"] = (max if mode == "serial" else sum)(reader["temp_bytes"] for reader in readers)
                            if trial:
                                samples[mode].append(sample)
                    for mode, values in samples.items():
                        report["cases"].append({"rows_per_sheet": rows, "sheets": 2, "cardinality": cardinality, "available_mib": available, "mode": mode, "source_sha256": digest, "samples": values, "median": {key: statistics.median(v[key] for v in values) for key in ["seconds", "peak_rss_kib", "logical_temp_upper_bound_bytes"]}, "cleanup_verified": True})
                    print(rows, cardinality, available, "passed", flush=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps([{key: case[key] for key in ["rows_per_sheet", "cardinality", "available_mib", "mode", "median"]} for case in report["cases"]], indent=2))


if __name__ == "__main__":
    main()
