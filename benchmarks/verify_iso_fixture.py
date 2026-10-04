"""Verify every generated ISO value through pinned public read-only APIs."""
from datetime import date, datetime, time, timedelta
import sys
import openpyxl
assert openpyxl.__version__ == "3.1.5"
book = openpyxl.load_workbook(sys.argv[1], read_only=True)
expected = (date(1899, 12, 31), datetime(2024, 2, 29, 12, 3, 4, 123000), time(12, 3, 4, 123000), timedelta(days=1, hours=6))
count = 0
for row in book["Sheet"].iter_rows(values_only=True):
    assert row == expected, row
    assert [type(value) for value in row] == [date, datetime, time, timedelta]
    count += len(row)
book.close()
print(count)
