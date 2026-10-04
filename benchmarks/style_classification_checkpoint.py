"""Large declared-format catalogs with equivalent styled-value projection."""
import argparse
import json
import os
from pathlib import Path
import platform
import statistics
import sys
import tempfile
import zipfile
from styled_checkpoint import generate
from shared_strings_checkpoint import ROOT, HERE
from iso_checkpoint import measure
import openpyxl

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline', type=Path, required=True)
    args = parser.parse_args()
    target = Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target'))
    report = {'reference': openpyxl.__version__, 'baseline_core': '541d8dfe83704ded24ceb3a81aa982ea254fb6c7', 'platform': platform.platform(), 'measurement': 'One warmup and five rotating serial cold-process samples; no builds during timing', 'semantics': 'Each reader verifies all 100,000 numeric/date/clock/duration/boolean/text/error/cache/integer values. Both native engines prepare the complete declared style catalog and stream cache-only values. Public openpyxl read-only projects equivalent values and prepares its catalogs. Additional declared formats are deliberately unused in cells, exposing preparation costs. No worksheet materialization or temporary storage.', 'cases': []}
    for extra in (1000, 50000):
        path = HERE / 'data' / f'style-classification-{extra}.xlsx'
        generate(path, 10000)
        with zipfile.ZipFile(path) as archive:
            parts = {name: archive.read(name) for name in archive.namelist()}
        additional = ''.join(f'<numFmt numFmtId="{500+i}" formatCode="0.00&quot;u{i}&quot;"/>' for i in range(extra))
        parts['xl/styles.xml'] = parts['xl/styles.xml'].replace(b'</numFmts>', additional.encode()+b'</numFmts>')
        with zipfile.ZipFile(path, 'w', compression=zipfile.ZIP_DEFLATED) as archive:
            for name, value in parts.items():
                archive.writestr(name, value)
        commands = {'crabxl': [target / 'release/examples/styled_read', path], 'crabxl-before': [args.baseline.resolve(), path], 'openpyxl': [sys.executable, HERE / 'read_styled_openpyxl.py', path]}
        samples = {name: [] for name in commands}
        with tempfile.TemporaryDirectory() as directory:
            for command in commands.values():
                output, _ = measure(command, Path(directory))
                assert output == '100000', output
            names = list(commands)
            for iteration in range(5):
                for name in names[iteration % 3:] + names[:iteration % 3]:
                    output, sample = measure(commands[name], Path(directory))
                    assert output == '100000'
                    assert sample['sampled_temp_peak_bytes'] == 0
                    samples[name].append(sample)
        report['cases'].append({'declared_formats': extra+1, 'cells': 100000, 'input_bytes': path.stat().st_size, 'samples': samples, 'medians': {name: {key: statistics.median(sample[key] for sample in values) for key in ('seconds', 'cpu_seconds', 'peak_rss_kib', 'sampled_temp_peak_bytes')} for name, values in samples.items()}})
        (HERE / 'results/m2-style-classification.json').write_text(json.dumps(report, indent=2)+'\n')
        print(json.dumps(report['cases'][-1]['medians']), flush=True)

if __name__ == '__main__':
    main()
