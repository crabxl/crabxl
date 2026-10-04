"""Observe nonfinite serialization through pinned public assignment/save/load APIs."""
import json
from pathlib import Path
import sys
import tempfile
import zipfile
import openpyxl
assert openpyxl.__version__ == "3.1.5"
report = {"reference": openpyxl.__version__, "inspection": "Public assignment/save/load APIs and generated OOXML only", "cases": []}
with tempfile.TemporaryDirectory() as directory:
    for label, value in [("nan", float("nan")), ("inf", float("inf")), ("negative-inf", -float("inf"))]:
        book = openpyxl.Workbook()
        book.active["A1"] = value
        path = Path(directory) / "book.xlsx"
        book.save(path)
        book.close()
        with zipfile.ZipFile(path) as archive:
            xml = archive.read("xl/worksheets/sheet1.xml").decode()
        book = openpyxl.load_workbook(path)
        report["cases"].append({"value": label, "worksheet_xml": xml, "loaded": book.active["A1"].value})
        book.close()
output = json.dumps(report, indent=2) + "\n"
if len(sys.argv) == 2:
    Path(sys.argv[1]).write_text(output)
else:
    print(output, end="")
