"""Verify generated styled cells through pinned public read-only APIs."""
import datetime
import sys
import openpyxl
book = openpyxl.load_workbook(sys.argv[1], read_only=True, data_only=True)
count = 0
for index, row in enumerate(book["Sheet"].iter_rows(values_only=True)):
    date = datetime.datetime(2024, 1, 1, 6) + datetime.timedelta(days=index % 365)
    expected = (1.25, date, datetime.time(12), datetime.timedelta(seconds=(index % 101)*21600), bool(index % 2), f"styled-{index:08}", "#DIV/0!", date, index, 1.25)
    assert row == expected, (index, row, expected)
    assert type(row[2]) is datetime.time and type(row[3]) is datetime.timedelta
    count += len(row)
book.close()
print(count)
