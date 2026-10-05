"""Measure explicitly requested native theme catalogs without worksheet decoding."""
import argparse
import hashlib
import json
from pathlib import Path
import statistics
import subprocess
import tempfile
import zipfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--measure", type=Path, required=True)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    report = {
        "scope": "Linux native release read of caller-owned theme catalogs; no worksheet "
        "cells are decoded. One warmup and three rotating serial samples. "
        "Fixture generation and builds/tests are excluded; no comparative speed claim.",
        "binary_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest(),
        "cases": [],
    }
    with tempfile.TemporaryDirectory(prefix="crabxl-theme-") as temporary:
        paths = {}
        for count in [0, 1000, 100000]:
            fonts = "".join(f'<a:font script="S{i}" typeface="Font{i}"/>' for i in range(count))
            xml = ('<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">'
                   '<a:themeElements><a:fontScheme name="Generated"><a:majorFont>'
                   f'{fonts}</a:majorFont></a:fontScheme></a:themeElements></a:theme>').encode()
            path = Path(temporary) / f"{count}.xlsx"
            with zipfile.ZipFile(args.source) as original, zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as output:
                for name in original.namelist():
                    output.writestr(name, xml if name == "xl/theme/theme1.xml" else original.read(name))
            with zipfile.ZipFile(path) as archive:
                assert archive.read("xl/theme/theme1.xml") == xml
                assert archive.testzip() is None
            paths[count] = (path, len(xml), hashlib.sha256(xml).hexdigest())
        samples = {count: [] for count in paths}
        for trial in range(4):
            for count in list(paths)[::(-1 if trial % 2 else 1)]:
                path = paths[count][0]
                result = subprocess.run([str(args.measure), str(args.binary), str(path)], check=True, capture_output=True, text=True)
                fields = dict(field.split("=") for field in result.stdout.split())
                assert int(fields["fonts"]) == count
                sample = json.loads(result.stderr.split("MEASURE ")[-1])
                sample.update({key: int(fields[key]) for key in ["fonts", "catalog_bytes", "opaque_bytes"]})
                if trial:
                    samples[count].append(sample)
        for count, values in samples.items():
            report["cases"].append({
                "supplemental_fonts": count,
                "theme_xml_bytes": paths[count][1],
                "theme_sha256": paths[count][2],
                "temporary_store_bytes": 0,
                "samples": values,
                "median": {key: statistics.median(v[key] for v in values) for key in ["seconds", "peak_rss_kib", "catalog_bytes", "opaque_bytes"]},
            })
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps([case["median"] for case in report["cases"]], indent=2))


if __name__ == "__main__":
    main()
