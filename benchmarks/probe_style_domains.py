"""Observe public style domains and verify native output without source inspection."""
import argparse
import json
from pathlib import Path
import openpyxl
from openpyxl.styles import Font, Color

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--native", type=Path, required=True)
args = parser.parse_args()
observations = []
for name, construct, values in [
    ("family", lambda x: Font(family=x), [-1, 0, 2.5, 14, 14.5]),
    ("charset", lambda x: Font(charset=x), [-1, 255, 256]),
    ("theme", lambda x: Color(theme=x), [-1, 0, 256]),
    ("indexed", lambda x: Color(indexed=x), [-1, 256]),
    ("rgb", lambda x: Color(rgb=x), ["aAbBcC", "aaBbCcDd"]),
]:
    for value in values:
        try:
            item = construct(value)
            observations.append({"field": name, "input": value, "accepted": True, "stored": getattr(item, name)})
        except ValueError as error:
            observations.append({"field": name, "input": value, "accepted": False, "error": str(error)})
root = Path(__file__).resolve().parents[1]
(root / "docs/research/style-domains-public.json").write_text(json.dumps({"reference": openpyxl.__version__, "observations": observations}, indent=2) + "\n")
book = openpyxl.load_workbook(args.native)
checked = []
for row, (charset, kind, value) in enumerate([(-1, "theme", -1), (256, "indexed", -1), (4096, "rgb", "aaBbCcDd"), (0, "rgb", "00aAbBcC")], 1):
    cell = book.active.cell(row, 1)
    assert cell.value == 1
    assert cell.font.family == 2.5
    assert cell.font.charset == charset
    assert cell.font.color.type == kind
    assert getattr(cell.font.color, kind) == value
    checked.append({"cell": cell.coordinate, "family": cell.font.family, "charset": charset, "color_type": kind, "color": value})
(root / "benchmarks/results/m2-style-domains-interop.json").write_text(json.dumps({"reference": openpyxl.__version__, "checked": checked}, indent=2) + "\n")
