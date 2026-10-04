"""Pinned public write-only baseline for controlled numeric output."""
import sys
import openpyxl
assert openpyxl.__version__ == '3.1.5'
path,count=sys.argv[1],int(sys.argv[2])
book=openpyxl.Workbook(write_only=True)
sheet=book.create_sheet('Sheet')
for index in range(count): sheet.append([index*10+column for column in range(10)])
book.save(path)
print(count,count*10)
