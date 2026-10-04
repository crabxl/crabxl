"""Verify literal microsecond creation after public reference serial loading."""
from datetime import datetime, time, timedelta
import json
import sys
import openpyxl
assert openpyxl.__version__ == "3.1.5"
results = []
for epoch, path in zip(("win", "mac"), sys.argv[1:]):
    book = openpyxl.load_workbook(path)
    actual = list(book.active.values)[0]
    expected = (datetime(2024, 2, 29, 12, 3, 4, 123000), time(2, 57, 46, 667000), timedelta(0), datetime(1899, 12, 31, 12, 0, 0, 123000) if epoch == "mac" else time(12, 0, 0, 123000))
    assert actual == expected, (actual, expected)
    results.append({"epoch": epoch, "values": [str(value) for value in actual], "types": [type(value).__name__ for value in actual], "reference": openpyxl.__version__})
    book.close()
assert len(results) == 2, "Require Windows and Mac fixtures in that order"
print(json.dumps(results, indent=2))
