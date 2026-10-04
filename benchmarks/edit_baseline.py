"""Pinned openpyxl general-mode edit/save, including load and serialization costs."""
import sys
import openpyxl
assert openpyxl.__version__ == '3.1.5'
book=openpyxl.load_workbook(sys.argv[1])
book['Sheet']['A1']=-1
book.save(sys.argv[2])
book.close()
print('Edited A1')
