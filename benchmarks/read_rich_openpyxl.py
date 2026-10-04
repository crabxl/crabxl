"""Verify rich runs through pinned public APIs; no implementation inspection."""
import sys
import openpyxl
from openpyxl.cell.rich_text import CellRichText, TextBlock
path, unique = sys.argv[1], int(sys.argv[2])
book = openpyxl.load_workbook(path, read_only=True, rich_text=True)
count = size = 0
suffix = "x" * 96
for row in book["Sheet"].iter_rows(values_only=True):
    for value in row:
        assert isinstance(value, CellRichText) and len(value) == 2
        a, b = value
        assert isinstance(a, TextBlock) and isinstance(b, TextBlock)
        assert a.text == f"item-{count % unique:08}-" and b.text == suffix
        assert a.font.b is True and a.font.i is False
        assert a.font.color.type == "theme" and a.font.color.theme == 3
        assert a.font.color.tint == 0.25
        assert b.font.rFont == "Aptos" and b.font.u == "double"
        size += len(str(value).encode("utf-8"))
        count += 1
book.close()
print(count, size)
