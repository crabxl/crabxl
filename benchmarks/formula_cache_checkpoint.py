"""Equivalent full-sheet formula cache projection with discarded-expression regression."""
import argparse
import io
import json
import os
from pathlib import Path
import platform
import statistics
import sys
import tempfile
import zipfile
import openpyxl
from shared_strings_checkpoint import ROOT, HERE
from iso_checkpoint import measure

def generate(path, rows):
    source = io.BytesIO()
    openpyxl.Workbook().save(source)
    with zipfile.ZipFile(source) as archive:
        parts = {name: archive.read(name) for name in archive.namelist()}
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for name, value in parts.items():
            if name != "xl/worksheets/sheet1.xml":
                archive.writestr(name, value)
        with archive.open("xl/worksheets/sheet1.xml", "w") as sheet:
            sheet.write(b'<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>')
            for index in range(1, rows + 1):
                expression = "+".join([f"A{index}"] * 64)
                sheet.write(f'<row r="{index}"><c r="A{index}"><f>{expression}</f><v>{index}</v></c></row>'.encode())
            sheet.write(b'</sheetData></worksheet>')

def reference(path):
    book = openpyxl.load_workbook(path, read_only=True, data_only=True)
    count = 0
    for index, row in enumerate(book.active, 1):
        assert row[0].value == index
        count += 1
    book.close()
    print(count)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline', type=Path, required=True)
    args = parser.parse_args()
    target = Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target'))
    report = {'reference': openpyxl.__version__, 'baseline_core': '5e643fe70d4facde50155097c3234a0259f033ee', 'platform': platform.platform(), 'semantics': 'All readers stream the complete worksheet and verify every integer cache. Formula evaluation and expression parsing are not requested; cache-only projection is compared. One warmup and five rotating serial cold-process samples; no builds during measurement. No temporary storage or worksheet materialization.', 'cases': []}
    for rows in (10000, 100000):
        path = HERE / 'data' / f'formula-cache-{rows}.xlsx'
        generate(path, rows)
        commands = {'crabxl': [target / 'release/examples/formula_cache_read', path], 'crabxl-before': [args.baseline.resolve(), path], 'openpyxl': [sys.executable, __file__, '--reference', path]}
        samples = {name: [] for name in commands}
        with tempfile.TemporaryDirectory() as directory:
            for command in commands.values():
                output, _ = measure(command, Path(directory))
                assert output == str(rows)
            names = list(commands)
            for iteration in range(5):
                for name in names[iteration % 3:] + names[:iteration % 3]:
                    output, sample = measure(commands[name], Path(directory))
                    assert output == str(rows)
                    assert sample['sampled_temp_peak_bytes'] == 0
                    samples[name].append(sample)
        report['cases'].append({'rows': rows, 'input_bytes': path.stat().st_size, 'samples': samples, 'medians': {name: {key: statistics.median(sample[key] for sample in values) for key in ('seconds', 'cpu_seconds', 'peak_rss_kib', 'sampled_temp_peak_bytes')} for name, values in samples.items()}})
        (HERE / 'results/m2-formula-cache.json').write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps(report['cases'][-1]['medians']), flush=True)

if __name__ == '__main__':
    if len(sys.argv) == 3 and sys.argv[1] == '--reference':
        reference(sys.argv[2])
    else:
        main()
