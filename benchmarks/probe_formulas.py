"""Record normal/shared/array/data-table public load behavior from generated OOXML."""
import json
from pathlib import Path
import sys
import tempfile
import zipfile
import openpyxl
from openpyxl.worksheet.formula import ArrayFormula, DataTableFormula
assert openpyxl.__version__ == "3.1.5"
MAIN = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
CASES = {
    "shared-anchor": '<row r="1"><c r="B1"><f t="shared" si="4294967295" ref="A1:D2">B1+$B$2+C3</f><v>0</v></c><c r="D1"><f t="shared" si="4294967295"/><v>1</v></c></row><row r="2"><c r="A2"><f t="shared" si="4294967295"/><v>2</v></c></row>',
    "outside-range": '<row><c r="A1"><f t="shared" si="1" ref="A1:A2">A1+1</f><v>0</v></c><c r="B1"><f t="shared" si="1"/><v>1</v></c></row>',
    "missing-master": '<row><c r="A1"><f t="shared" si="1"/><v>1</v></c></row>',
    "text-no-ref": '<row><c r="A1"><f t="shared" si="1">A1+1</f><v>0</v></c><c r="B1"><f t="shared" si="1"/><v>1</v></c></row>',
    "reused-master": '<row><c r="A1"><f t="shared" si="1" ref="A1:B1">A1+1</f><v>0</v></c><c r="B1"><f t="shared" si="1"/><v>1</v></c><c r="C1"><f t="shared" si="1" ref="C1:D1">C1+2</f><v>0</v></c><c r="D1"><f t="shared" si="1"/><v>1</v></c></row>',
    "array-table": '<row><c r="A1"><f t="array" ref="A1:B2" aca="0">SUM(C1:C2)</f><v>5</v></c><c r="D1"><f t="dataTable" ref="D1:E2" dt2D="1" dtr="0" r1="A1" r2="B1" ca="0" del1="0" del2="1"/><v>0</v></c></row>',
    "missing-caches": '<row><c r="A1"><f>1</f></c><c r="B1" t="str"><f>1</f><v/></c><c r="C1" t="b"><f>1</f><v>0</v></c><c r="D1" t="e"><f>1</f><v>#DIV/0!</v></c></row>',
    "overflow-numeric": '<row><c r="A1"><v>1e999</v></c><c r="B1"><v>-1e999</v></c><c r="C1"><f>1</f><v>1e999</v></c></row>',
}
def probe():
    report = {"reference": openpyxl.__version__, "inspection": "Public load APIs and newly generated OOXML only", "cases": []}
    with tempfile.TemporaryDirectory() as temporary:
        path = Path(temporary) / "formulas.xlsx"
        base = openpyxl.Workbook()
        base.save(path)
        base.close()
        with zipfile.ZipFile(path) as archive:
            parts = {name: archive.read(name) for name in archive.namelist()}
        for name, content in CASES.items():
            parts["xl/worksheets/sheet1.xml"] = f'<worksheet xmlns="{MAIN}"><sheetData>{content}</sheetData></worksheet>'.encode()
            with zipfile.ZipFile(path, "w") as archive:
                for member, value in parts.items():
                    archive.writestr(member, value)
            for cached in (False, True):
                case = {"name": name, "data_only": cached}
                try:
                    book = openpyxl.load_workbook(path, data_only=cached)
                    values = {}
                    for row in book.active:
                        for cell in row:
                            if cell.value is not None:
                                value = cell.value
                                if isinstance(value, (ArrayFormula, DataTableFormula)):
                                    value = {"type": type(value).__name__, "properties": vars(value)}
                                elif isinstance(value, float) and (value == float("inf") or value == -float("inf")):
                                    value = {"type": "float", "value": str(value)}
                                values[cell.coordinate] = value
                    case["values"] = values
                    book.close()
                except (ValueError, KeyError, TypeError) as error:
                    case.update(error=type(error).__name__, message=str(error))
                report["cases"].append(case)
    return report


if __name__ == "__main__":
    output = json.dumps(probe(), indent=2) + "\n"
    if len(sys.argv) == 2:
        with open(sys.argv[1], "w") as target:
            target.write(output)
    else:
        print(output, end="")
