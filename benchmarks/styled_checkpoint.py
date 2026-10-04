"""Bounded style-catalog/date correctness and public read-only performance checkpoint."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import sys
import tempfile
import zipfile
import openpyxl
from shared_strings_checkpoint import ROOT, HERE, MAIN, REL, PKG, run


def generate(path, rows):
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("[Content_Types].xml", '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/></Types>')
        archive.writestr("_rels/.rels", f'<Relationships xmlns="{PKG}"><Relationship Id="book" Type="{REL}/officeDocument" Target="xl/workbook.xml"/></Relationships>')
        archive.writestr("xl/workbook.xml", f'<workbook xmlns="{MAIN}" xmlns:r="{REL}"><sheets><sheet name="Sheet" sheetId="1" r:id="sheet"/></sheets></workbook>')
        archive.writestr("xl/_rels/workbook.xml.rels", f'<Relationships xmlns="{PKG}"><Relationship Id="sheet" Type="{REL}/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="styles" Type="{REL}/styles" Target="styles.xml"/></Relationships>')
        archive.writestr("xl/styles.xml", f'<styleSheet xmlns="{MAIN}"><numFmts><numFmt numFmtId="164" formatCode="[h]:mm:ss.000"/></numFmts><fonts><font><name val="Calibri"/><sz val="11"/></font></fonts><fills><fill><patternFill patternType="none"/></fill></fills><borders><border/></borders><cellStyleXfs><xf/></cellStyleXfs><cellXfs><xf/><xf numFmtId="14"/><xf numFmtId="164"/></cellXfs><cellStyles><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles></styleSheet>')
        with archive.open("xl/worksheets/sheet1.xml", "w") as output:
            output.write(f'<worksheet xmlns="{MAIN}"><dimension ref="A1:J{rows}"/><sheetData>'.encode())
            for index in range(rows):
                n = index + 1
                date = 45292.25 + index % 365
                output.write((f'<row r="{n}"><c r="A{n}"><v>1.25</v></c><c r="B{n}" s="1"><v>{date}</v></c><c r="C{n}" s="1"><v>0.5</v></c><c r="D{n}" s="2"><v>{(index%101)/4}</v></c><c r="E{n}" s="1" t="b"><v>{index%2}</v></c><c r="F{n}" t="inlineStr"><is><t>styled-{index:08}</t></is></c><c r="G{n}" t="e"><v>#DIV/0!</v></c><c r="H{n}" s="1"><f>1</f><v>{date}</v></c><c r="I{n}"><v>{index}</v></c><c r="J{n}"><v>1.25</v></c></row>').encode())
            output.write(b'</sheetData></worksheet>')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rows", type=int, nargs="+", default=[10000, 100000])
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--output", type=Path, default=HERE / "results/m2-styles-dates.json")
    args = parser.parse_args()
    if openpyxl.__version__ != "3.1.5" or args.runs < 1 or any(not 0 < n <= 1048576 for n in args.rows):
        parser.error("Require pinned reference and positive sample/row counts")
    run(["cc", "-O2", "-Wall", "-Wextra", "-Werror", HERE / "measure.c", "-o", HERE / "measure"])
    run(["cargo", "build", "--locked", "--release", "-p", "crabxl", "--example", "styled_read"])
    run(["cargo", "build", "--locked", "--release", "--manifest-path", HERE / "calamine/Cargo.toml", "--bin", "styled_read"])
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    report = {"workload": "Ten columns: general floats, dates, clock, elapsed duration, styled bool, inline string, error, date formula cache and exact integer; all values checked", "measurement": f"One warmup; {args.runs} rotating serial samples; build/generation excluded; process baseline included", "versions": {"openpyxl": openpyxl.__version__, "calamine": "0.36.1", "rust": run(["rustc", "--version"]).stdout.strip()}, "platform": platform.platform(), "semantics": "Native rows stream with a lazily loaded shared catalog; openpyxl read-only streaming. No shared-string table or temporary files. This is numeric date/cache overlap, not full style API parity. Calamine materializes a Range and retains date/clock serial wrappers; equivalent values/components checked, not identical public types.", "cases": []}
    data = HERE / "data"
    data.mkdir(exist_ok=True)
    for rows in args.rows:
        path = data / f"styled-{rows}.xlsx"
        generate(path, rows)
        commands = {"calamine": [target / "release/styled_read" if os.environ.get("CARGO_TARGET_DIR") else HERE / "calamine/target/release/styled_read", path], "crabxl": [target / "release/examples/styled_read", path], "openpyxl": [sys.executable, HERE / "read_styled_openpyxl.py", path]}
        def measure(command):
            result = run([HERE / "measure", *command])
            assert result.stdout.strip() == str(rows * 10), result.stdout
            sample = json.loads(result.stderr.split("MEASURE ")[-1])
            for line in result.stderr.splitlines():
                if line.startswith("STYLE_BYTES "):
                    sample["retained_style_bytes"] = int(line.split()[1])
            return sample
        for command in commands.values():
            measure(command)
        samples = {name: [] for name in commands}
        names = list(commands)
        for iteration in range(args.runs):
            for name in names[iteration % len(names):] + names[:iteration % len(names)]:
                sample = measure(commands[name])
                samples[name].append(sample)
                print(rows, name, sample, flush=True)
        report["cases"].append({"rows": rows, "cells": rows*10, "file_bytes": path.stat().st_size, "file_sha256": hashlib.sha256(path.read_bytes()).hexdigest(), "samples": samples, "medians": {name: {key: statistics.median(s[key] for s in values) for key in ("seconds", "cpu_seconds", "peak_rss_kib")} for name, values in samples.items()}})
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + "\n")

if __name__ == "__main__":
    main()
