"""Check native formula creation using pinned public openpyxl APIs."""
import json
from pathlib import Path
import sys
import openpyxl
from openpyxl.worksheet.formula import ArrayFormula, DataTableFormula
assert openpyxl.__version__ == "3.1.5"
book = openpyxl.load_workbook(sys.argv[1])
array, table = book.active["A1"].value, book.active["D1"].value
assert isinstance(array, ArrayFormula)
assert vars(array) == {"ref": "$A$1:$B$2", "text": "=SUM(C1:C2)"}
assert isinstance(table, DataTableFormula)
assert vars(table) == {"ref": "D1:E2", "ca": "0", "dt2D": "1", "dtr": "0", "r1": "$A$1", "r2": "B1", "del1": "0", "del2": "1"}
assert book.active["F1"].value == "==1"
assert book.active["G1"].value == "="
report = {"reference": openpyxl.__version__, "inspection": "Public load APIs only", "array": vars(array), "table": vars(table), "verbatim_expression": book.active["F1"].value, "empty_expression": book.active["G1"].value}
book.close()
book = openpyxl.load_workbook(sys.argv[1], data_only=True)
assert [book.active[cell].value for cell in ("A1", "D1", "F1", "G1")] == [5, 0, None, None]
report["caches"] = [5, 0, None, None]
book.close()
output = json.dumps(report, indent=2) + "\n"
if len(sys.argv) == 3:
    Path(sys.argv[2]).write_text(output)
else:
    print(output, end="")
