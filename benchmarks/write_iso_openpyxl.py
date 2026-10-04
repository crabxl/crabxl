"""Pinned public write-only ISO creation with no custom serializer."""
from datetime import date, datetime, time, timedelta
import sys
import openpyxl
assert openpyxl.__version__ == "3.1.5"
path, rows, mac = sys.argv[1], int(sys.argv[2]), sys.argv[3] == "mac"
book = openpyxl.Workbook(write_only=True, iso_dates=True)
if mac:
    from openpyxl.utils.datetime import CALENDAR_MAC_1904
    book.epoch = CALENDAR_MAC_1904
sheet = book.create_sheet("Sheet")
values = (date(1899, 12, 31), datetime(2024, 2, 29, 12, 3, 4, 123456), time(12, 3, 4, 123456), timedelta(days=1, hours=6))
for _ in range(rows):
    sheet.append(values)
book.save(path)
book.close()
print(rows * 4)
