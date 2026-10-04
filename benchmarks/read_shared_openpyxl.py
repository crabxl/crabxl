"""Pinned public read-only string verification; no implementation source access."""
import sys
import openpyxl

path, unique = sys.argv[1], int(sys.argv[2])
book = openpyxl.load_workbook(path, read_only=True, data_only=True)
count = size = 0
suffix = "x" * 96
for row in book["Sheet"].iter_rows(values_only=True):
    for value in row:
        if value != f"item-{count % unique:08}-{suffix}":
            raise RuntimeError("Shared-string value/order mismatch")
        size += len(value.encode("utf-8"))
        count += 1
book.close()
print(count, size)
