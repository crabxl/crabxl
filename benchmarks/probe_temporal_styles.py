"""Public format assignment/readback and canonical component-preserving output."""
import json
import os
from pathlib import Path
import subprocess
from temporal_style_checkpoint import ROOT, FORMATS, DATES, generate, records
reference = ROOT / 'benchmarks/data/temporal-style-reference.xlsx'
native = ROOT / 'benchmarks/data/temporal-style-native.xlsx'
target = Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target'))
generate(reference, 4, streaming=False)
assert subprocess.check_output([target / 'release/examples/temporal_style_fixture', native], text=True).strip() == '20'
expected = records(reference)
assert records(native) == expected
report = {'assigned_formats': FORMATS, 'temporal_kinds': [type(value).__name__ for value in DATES], 'public_records_equal': True, 'records': expected}
(ROOT / 'benchmarks/results/m2-temporal-styles-interop.json').write_text(json.dumps(report, indent=2)+'\n')
print(json.dumps(report))
