"""Check complete component creation through the pinned public style API."""
import json
import sys
import openpyxl
assert openpyxl.__version__ == "3.1.5"
book = openpyxl.load_workbook(sys.argv[1])
cell = book["Sheet"]["A1"]
assert cell.value == 1.5 and cell.number_format == "0.00"
f = cell.font
assert f.name == "A"*80 and f.sz == 12.5
assert (f.b, f.i, f.strike, f.outline, f.shadow, f.condense, f.extend) == (False, True, True, False, True, False, True)
assert (f.u, f.vertAlign, f.charset, f.family, f.scheme) == ("doubleAccounting", "subscript", 128, 3, "major")
assert (f.color.type, f.color.rgb, f.color.tint) == ("rgb", "80445566", -.25)
g = cell.fill
assert (g.type, g.degree, g.left, g.right, g.top, g.bottom) == ("path", 35, .1, .2, .3, .4)
assert len(g.stop) == 2 and [s.position for s in g.stop] == [0, 1]
assert g.stop[0].color.rgb == "80AABBCC" and g.stop[1].color.indexed == 64
b = cell.border
assert (b.diagonalUp, b.diagonalDown, b.outline) == (True, False, False)
for index, (name, line) in enumerate(zip(("left", "right", "top", "bottom", "diagonal", "vertical", "horizontal", "start", "end"), ("thin", "medium", "thick", "dashed", "slantDashDot", "mediumDashDot", "mediumDashDotDot", "hair", None))):
    edge = getattr(b, name)
    assert edge.style == line and edge.color.type == "auto"
    assert edge.color.auto == (index%2 == 0) and edge.color.tint == .25
alignment = cell.alignment
assert (alignment.horizontal, alignment.vertical, alignment.textRotation) == ("distributed", "justify", 255)
assert alignment.wrapText is True and alignment.shrinkToFit is True
assert (alignment.indent, alignment.relativeIndent, alignment.readingOrder, alignment.justifyLastLine) == (2.5, -1.5, 2, True)
assert cell.protection.locked is False and cell.protection.hidden is True
book.close()
print(json.dumps({"reference": openpyxl.__version__, "value": "verified", "font_properties": 16, "gradient_geometry_and_stops": "verified", "border_positions": 9, "alignment_and_protection": "verified"}, indent=2))
