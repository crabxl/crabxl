"""Canonical catalog transfer and source-format edit compared with public model calls."""
import argparse
import json
import os
from pathlib import Path
import platform
import statistics
import sys
import tempfile
import zipfile
import datetime
import openpyxl
from styled_checkpoint import generate
from shared_strings_checkpoint import ROOT, HERE, run
from iso_checkpoint import measure

def reference(path):
    book = openpyxl.load_workbook(path, data_only=True)
    row = next(book.active.iter_rows())
    date = datetime.datetime(2024, 1, 1, 6)
    assert tuple(cell.value for cell in row) == (1.25, date, datetime.time(12), datetime.timedelta(), False, 'styled-00000000', '#DIV/0!', date, 0, 1.25)
    row[0].number_format = 'mm-dd-yy'
    assert row[0].style_id == 1 and row[0].value == 1.25
    assert row[0].number_format == 'mm-dd-yy'
    book.close()
    print(10)

def main():
    target = Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target'))
    report = {'reference': openpyxl.__version__, 'platform': platform.platform(), 'measurement': 'One warmup and five rotating serial cold-process samples, no builds during timing', 'semantics': 'One physical row with ten typed/cache values. Both engines retain declared catalogs, verify all values and change A1 number format to an existing source date format without changing its scalar or source style identity. Native consumes the reader and moves its catalog into a bounded registry, without catalog copying. Public uses general editable loading. No save/loaded package structural editing claim, no temporary storage, no previous import engine API to benchmark.', 'cases': []}
    for extra in (1000, 50000):
        path = HERE / 'data' / f'style-import-{extra}.xlsx'
        generate(path, 1)
        with zipfile.ZipFile(path) as archive:
            parts = {name: archive.read(name) for name in archive.namelist()}
        additional = ''.join(f'<numFmt numFmtId="{500+i}" formatCode="0.00&quot;u{i}&quot;"/>' for i in range(extra))
        parts['xl/styles.xml'] = parts['xl/styles.xml'].replace(b'</numFmts>', additional.encode()+b'</numFmts>')
        with zipfile.ZipFile(path, 'w', compression=zipfile.ZIP_DEFLATED) as archive:
            for name, value in parts.items(): archive.writestr(name, value)
        commands = {'crabxl': [target / 'release/examples/style_catalog_import', path, extra+1], 'openpyxl': [sys.executable, __file__, '--reference', path]}
        managed = int(run(commands['crabxl']).stderr.split('STYLE_IMPORT_BYTES ')[1].strip())
        samples = {name: [] for name in commands}
        with tempfile.TemporaryDirectory() as directory:
            for command in commands.values():
                output, _ = measure(command, Path(directory))
                assert output == '10'
            names = list(commands)
            for iteration in range(5):
                for name in names[iteration % 2:] + names[:iteration % 2]:
                    output, sample = measure(commands[name], Path(directory))
                    assert output == '10' and sample['sampled_temp_peak_bytes'] == 0
                    samples[name].append(sample)
        report['cases'].append({'declared_formats': extra+1, 'cells': 10, 'managed_registry_bytes': managed, 'input_bytes': path.stat().st_size, 'samples': samples, 'medians': {name: {key: statistics.median(sample[key] for sample in values) for key in ('seconds', 'cpu_seconds', 'peak_rss_kib', 'sampled_temp_peak_bytes')} for name, values in samples.items()}})
        (HERE / 'results/m2-style-import.json').write_text(json.dumps(report, indent=2)+'\n')
        print(json.dumps(report['cases'][-1]['medians']), flush=True)

if __name__ == '__main__':
    if len(sys.argv) == 3 and sys.argv[1] == '--reference': reference(sys.argv[2])
    else: main()
