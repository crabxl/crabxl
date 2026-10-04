"""Compare generated formula cases through public APIs; no reference engine source."""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile
import zipfile
import openpyxl
from probe_formulas import CASES, MAIN, probe


def parse_output(output):
    values = {}
    for line in output.splitlines():
        address, kind, *fields = line.split("\t")
        if kind == "empty":
            continue
        if kind == "integer":
            value = int(fields[0])
        elif kind == "number":
            value = float(fields[0])
        elif kind == "boolean":
            value = fields[0] == "true"
        elif kind in ("text", "error"):
            value = fields[0]
        elif kind == "array":
            value = {"type": "ArrayFormula", "properties": dict(zip(("ref", "text"), fields, strict=True))}
        elif kind == "table":
            names = ("ref", "ca", "dt2D", "dtr", "r1", "r2", "del1", "del2")
            value = {"type": "DataTableFormula", "properties": dict(zip(names, fields, strict=True))}
        else:
            raise AssertionError(kind)
        values[address] = value
    return values


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    reference = probe()
    checked = []
    deferred = []
    with tempfile.TemporaryDirectory() as temporary:
        path = Path(temporary) / "formulas.xlsx"
        book = openpyxl.Workbook()
        book.save(path)
        book.close()
        with zipfile.ZipFile(path) as archive:
            parts = {name: archive.read(name) for name in archive.namelist()}
        for case in reference["cases"]:
            name = case["name"]
            if name == "overflow-numeric":
                deferred.append({"name": name, "data_only": case["data_only"], "reason": "Nonfinite numeric compatibility remains a separate M2 value checkpoint"})
                continue
            parts["xl/worksheets/sheet1.xml"] = f'<worksheet xmlns="{MAIN}"><sheetData>{CASES[name]}</sheetData></worksheet>'.encode()
            with zipfile.ZipFile(path, "w") as archive:
                for member, value in parts.items():
                    archive.writestr(member, value)
            command = [str(args.binary.resolve()), str(path)]
            if case["data_only"]:
                command.append("cached")
            completed = subprocess.run(command, text=True, capture_output=True, check=True)
            actual = parse_output(completed.stdout)
            assert actual == case["values"], (name, case["data_only"], actual, case["values"])
            checked.append({"name": name, "data_only": case["data_only"], "values": actual})
    result = {"reference": reference["reference"], "inspection": reference["inspection"], "checked": checked, "deferred": deferred}
    output = json.dumps(result, indent=2) + "\n"
    if args.output:
        args.output.write_text(output)
    else:
        print(output, end="")


if __name__ == "__main__":
    main()
