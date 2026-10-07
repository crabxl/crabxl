"""Ordinary typed rich-value creation and source-backed structural editing."""

import importlib
import json
import resource
import sys
import time
from pathlib import Path

engine_name, mode, count = sys.argv[1], sys.argv[2], int(sys.argv[3])
engine = importlib.import_module(engine_name)
rich = importlib.import_module(engine_name + ".cell.rich_text")
InlineFont = importlib.import_module(engine_name + ".cell.text").InlineFont
source = Path(f"/tmp/crabxl-w13-rich-source-{count}.xlsx")
output = Path(f"/tmp/crabxl-w13-rich-{engine_name}-{mode}-{count}.xlsx")
started = time.perf_counter()
if mode in ("create", "fixture"):
    book = engine.Workbook()
    sheet = book.active
    font = InlineFont(b=True, color="80445566")
    for row in range(1, count + 1):
        sheet.cell(
            row,
            1,
            rich.CellRichText(" prefix ", rich.TextBlock(font, "bold"), f" {row}"),
        )
elif mode == "edit":
    book = engine.load_workbook(source, rich_text=True)
    sheet = book.active
    checksum = sum(len(str(value[0])) for value in sheet.values)
    assert checksum == sum(len(f" prefix bold {row}") for row in range(1, count + 1))
else:
    raise ValueError("mode must be create or edit")
if mode == "fixture":
    book.save(source)
    book.close()
    sys.exit(0)
prepared = time.perf_counter() - started
started = time.perf_counter()
value = sheet.cell(count, 1).value
assert isinstance(value, rich.CellRichText) and len(value) == 3
value[1].text = "changed"
sheet.insert_rows(1)
edited = time.perf_counter() - started
started = time.perf_counter()
book.save(output)
saved = time.perf_counter() - started
peak = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss

# Keep reference import/reopening outside the timed interval and RSS sample.
import openpyxl

reference = openpyxl.load_workbook(output, rich_text=True)
last = reference.active.cell(count + 1, 1).value
assert str(last) == f" prefix changed {count}" and last[1].font.b
assert last[1].font.color.rgb == "80445566"
assert reference.active.max_row == count + 1
reference.close()
book.close()
print(
    json.dumps(
        {
            "engine": engine_name,
            "mode": mode,
            "rows": count,
            "prepare_seconds": prepared,
            "edit_seconds": edited,
            "save_seconds": saved,
            "peak_rss_kib": peak,
            "output_bytes": output.stat().st_size,
        }
    )
)
