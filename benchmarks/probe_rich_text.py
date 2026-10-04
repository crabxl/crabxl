"""Record public reference protection semantics without implementation access."""
import json
from pathlib import Path
import tempfile
import zipfile
import openpyxl
from rich_text_checkpoint import generate

assert openpyxl.__version__ == "3.1.5"
namespace = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
report = {"reference": "openpyxl 3.1.5 public load_workbook API", "implementation_inspected": False, "cases": []}
with tempfile.TemporaryDirectory() as temp:
    path = Path(temp) / "probe.xlsx"
    generate(path, 1, 1)
    with zipfile.ZipFile(path) as archive:
        base = {name: archive.read(name) for name in archive.namelist()}
    for location in ("shared", "inline"):
        for name, content in (
            ("plain-protected", '<t>_x005F_x0041_</t>'),
            ("unformatted-protected-run", '<r><t>_x005F_x0041_</t></r>'),
            ("single-protected-run", '<r><rPr><b/></rPr><t>_x005F_x0041_</t></r>'),
            ("cross-run-protection", '<r><rPr><b/></rPr><t>_x005F</t></r><r><t>_x0041_</t></r>'),
            ("whitespace-empty-styled", '<r><t xml:space="preserve">  </t></r><r><rPr><i/></rPr><t></t></r>'),
        ):
            parts = base.copy()
            parts["xl/sharedStrings.xml"] = f'<sst xmlns="{namespace}"><si>{content}</si></sst>'.encode()
            cell = '<c r="A1" t="s"><v>0</v></c>' if location == "shared" else f'<c r="A1" t="inlineStr"><is>{content}</is></c>'
            parts["xl/worksheets/sheet1.xml"] = f'<worksheet xmlns="{namespace}"><dimension ref="A1"/><sheetData><row r="1">{cell}</row></sheetData></worksheet>'.encode()
            with zipfile.ZipFile(path, "w") as archive:
                for key, value in parts.items():
                    archive.writestr(key, value)
            observed = {}
            for preserve in (False, True):
                book = openpyxl.load_workbook(path, rich_text=preserve)
                value = book["Sheet"]["A1"].value
                observed[str(preserve)] = {"display": str(value), "type": type(value).__name__}
                if preserve and not isinstance(value, str):
                    observed[str(preserve)]["runs"] = [run if isinstance(run, str) else run.text for run in value]
                book.close()
            report["cases"].append({"location": location, "case": name, "results": observed})
print(json.dumps(report, indent=2))
