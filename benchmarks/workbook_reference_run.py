"""Public openpyxl equivalent of the owned sparse workbook/export workload."""
import sys
import time
import openpyxl
rows, output = int(sys.argv[1]), sys.argv[2]
book = openpyxl.Workbook()
sheet = book.active
sheet.title = 'Original'
begin = time.perf_counter()
for row in range(rows):
    sheet.append([row*10+column for column in range(10)])
build_seconds = time.perf_counter()-begin
begin = time.perf_counter()
copied = book.copy_worksheet(sheet)
copied.title = 'Copy'
copy_seconds = time.perf_counter()-begin
book.move_sheet(copied, offset=-1)
sheet.title = 'Renamed'
book.active = sheet
begin = time.perf_counter()
book.save(output)
write_seconds = time.perf_counter()-begin
book.close()
import json
cells = rows*10
print(json.dumps(dict(cells=2*cells,sum=cells*(cells-1),build_seconds=build_seconds,copy_seconds=copy_seconds,write_seconds=write_seconds)))
