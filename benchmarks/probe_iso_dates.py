"""Observe pinned public ISO utilities without inspecting reference source."""
import json
import sys
import openpyxl
from openpyxl.utils.datetime import from_ISO8601, to_ISO8601
assert openpyxl.__version__ == "3.1.5"
inputs = ["", "2024-02-29", "2024-02-29T12:34:56.123456", "12:34:56.123456", "2024-02-29T12:34", "12:34", "2024-02-29Z", "2024-02-29T12:34:56+02:00", "PT1H2M3.123456S", "P1DT2H", "PT0.123456S", "2024-02-29garbage", "2024-02", "2024-02-29T24:00:00", "12", "12:", "12:34:", "12:34:56.1", "12:34:56.12", "12:34:56.1234", "2024-02-29T", "2024-02-29T12", "2024-02-29t12:34", "PT1H", "PT1M", "PT1S", "PT1.2S", "PT1.234S", "PT1H2.3S", "PT1H2M3.123S", "PT1H2M3.12S", "PT0S", "PT", "-PT1S", "PT100H", "PT1Mgarbage", "PT1.5H", "12:99garbage", "2024-02-29 12:34:56", " 2024-02-29", "2024-02-30", "0000-01-01"]
report = {"reference": openpyxl.__version__, "inspection": "Public APIs only; prefix acceptance and fraction truncation are observed baseline behavior", "cases": []}
for text in inputs:
    case = {"input": text}
    try:
        value = from_ISO8601(text)
        case.update(type=type(value).__name__, value=str(value))
        if value is not None and hasattr(value, "isoformat"):
            case["iso"] = to_ISO8601(value)
    except (ValueError, OverflowError) as error:
        case.update(error=type(error).__name__, message=str(error))
    report["cases"].append(case)
output = json.dumps(report, indent=2) + "\n"
if len(sys.argv) == 2:
    with open(sys.argv[1], "w") as target:
        target.write(output)
else:
    print(output, end="")
