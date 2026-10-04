"""Compare every common public default style property without engine source access."""
import json
from pathlib import Path
import sys
import openpyxl
assert openpyxl.__version__ == "3.1.5"


def color(value):
    if value is None:
        return None
    return {"type": value.type, "value": getattr(value, value.type), "tint": value.tint}


def snapshot(cell):
    font_fields = ("name", "sz", "b", "i", "strike", "outline", "shadow", "condense", "extend", "u", "vertAlign", "charset", "family", "scheme")
    alignment_fields = ("horizontal", "vertical", "textRotation", "wrap_text", "shrink_to_fit", "indent", "relativeIndent", "readingOrder", "justifyLastLine")
    border = cell.border
    return {"font": {**{name: getattr(cell.font, name) for name in font_fields}, "color": color(cell.font.color)}, "fill": {"pattern": cell.fill.patternType, "foreground": color(cell.fill.fgColor), "background": color(cell.fill.bgColor)}, "border": {**{name: None if getattr(border, name) is None else {"style": getattr(border, name).style, "color": color(getattr(border, name).color)} for name in ("left", "right", "top", "bottom", "diagonal", "vertical", "horizontal", "start", "end")}, "outline": border.outline, "diagonalUp": border.diagonalUp, "diagonalDown": border.diagonalDown}, "alignment": {name: getattr(cell.alignment, name) for name in alignment_fields}, "protection": {"locked": cell.protection.locked, "hidden": cell.protection.hidden}, "number_format": cell.number_format, "has_style": cell.has_style}


reference = openpyxl.Workbook()
expected = snapshot(reference.active["A1"])
book = openpyxl.load_workbook(sys.argv[1])
actual = snapshot(book.active["A1"])
assert actual == expected, (actual, expected)
assert book.style_names == reference.style_names == ["Normal"]
book.close()
reference.close()
report = {"reference": openpyxl.__version__, "inspection": "Public workbook and cell style properties only", "defaults": actual, "style_names": ["Normal"], "equal": True}
text = json.dumps(report, indent=2) + "\n"
if len(sys.argv) == 3:
    Path(sys.argv[2]).write_text(text)
else:
    print(text, end="")
