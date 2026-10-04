"""Record public constructor and date conversion behavior without inspecting source."""
import datetime
import json
import sys
import openpyxl
from openpyxl.styles import Alignment, Font
from openpyxl.utils.datetime import CALENDAR_MAC_1904, CALENDAR_WINDOWS_1900, from_excel

if openpyxl.__version__ != "3.1.5":
    raise RuntimeError("Pinned openpyxl 3.1.5 required")
report = {"reference": openpyxl.__version__, "inspection": "Public API only", "constructors": {}, "serials": []}
for name, constructor in {
    "font_name_80": lambda: Font(name="A"*80),
    "wrap_and_shrink": lambda: Alignment(wrap_text=True, shrink_to_fit=True),
    "family_255": lambda: Font(family=255),
    "family_14": lambda: Font(family=14),
    "family_15": lambda: Font(family=15),
}.items():
    try:
        value = constructor()
        report["constructors"][name] = {"accepted": True}
    except (ValueError, TypeError) as error:
        report["constructors"][name] = {"accepted": False, "error": type(error).__name__, "message": str(error)}
for epoch_name, epoch in [("Windows1900", CALENDAR_WINDOWS_1900), ("Mac1904", CALENDAR_MAC_1904)]:
    for serial in [0, .5, .99999999999, 1, 59, 60, 61, -.5, 45292.123456789, 2958465.000000017, 2958466]:
        for duration in [False, True]:
            result = {"epoch": epoch_name, "serial": serial, "duration": duration}
            try:
                value = from_excel(serial, epoch=epoch, timedelta=duration)
                result.update(type=type(value).__name__, value=str(value))
            except (ValueError, OverflowError) as error:
                result.update(error=type(error).__name__)
            report["serials"].append(result)
output = json.dumps(report, indent=2) + "\n"
if len(sys.argv) == 2:
    with open(sys.argv[1], "w") as target:
        target.write(output)
else:
    print(output, end="")
