"""Verify canonical borrowed built-in codes against the public pinned mapping."""
import json
from pathlib import Path
import subprocess
import sys
import openpyxl
from openpyxl.styles.numbers import BUILTIN_FORMATS
assert openpyxl.__version__ == "3.1.5"
output = subprocess.check_output([sys.argv[1]], text=True)
actual = {int(row.split("\t", 1)[0]): row.split("\t", 1)[1] for row in output.splitlines()}
assert actual == BUILTIN_FORMATS
report = {"reference": "openpyxl 3.1.5", "inspection": "Public constant mapping only; no source inspection", "formats_checked": len(actual), "equal": True}
text = json.dumps(report, indent=2) + "\n"
if len(sys.argv) == 3:
    Path(sys.argv[2]).write_text(text)
else:
    print(text, end="")
