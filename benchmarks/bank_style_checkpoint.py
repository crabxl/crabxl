"""Equivalent explicit owned-model style creation and public save/reload evidence."""
import json
import os
from pathlib import Path
import platform
import statistics
import sys
import tempfile
import subprocess
from style_registry_checkpoint import reference, verify
from shared_strings_checkpoint import ROOT, HERE, run
from iso_checkpoint import measure
import openpyxl

def main():
    assert openpyxl.__version__ == '3.1.5'
    target = Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target'))
    report = {'reference': openpyxl.__version__, 'platform': platform.platform(), 'measurement': 'One warmup and five rotating serial wall/CPU/process RSS samples; generation/build/readback excluded', 'semantics': 'Both engines explicitly retain a worksheet and style catalogs before save. One cell per distinct shared font/fill/alignment combination; all visible properties are checked outside timing through public readback and canonical streaming values. Native re-registers each style to check stable IDs; reference shares font/fill/alignment objects and interns cell formats during save. Ownership transfer adds no full style/cell snapshot. Native bank has no complete loaded-package feature graph. No prior-equivalent native bank-style API or native competitor claim is fabricated.', 'cases': []}
    for count in (1000, 8000):
        paths = {name: HERE / 'data' / f'bank-style-{name}-{count}.xlsx' for name in ('crabxl', 'openpyxl')}
        commands = {'crabxl': [target / 'release/examples/bank_style_fixture', paths['crabxl'], count], 'openpyxl': [sys.executable, __file__, '--reference', paths['openpyxl'], count]}
        samples = {name: [] for name in commands}
        tables = {}
        with tempfile.TemporaryDirectory() as directory:
            for name, command in commands.items():
                output, _ = measure(command, Path(directory))
                assert output == str(count)
                tables[name] = verify(paths[name], count)
            names = list(commands)
            for iteration in range(5):
                for name in names[iteration % 2:] + names[:iteration % 2]:
                    output, sample = measure(commands[name], Path(directory))
                    assert output == str(count)
                    assert verify(paths[name], count) == tables[name]
                    assert run([target / 'release/examples/style_registry_read', paths[name]]).stdout.strip() == str(count)
                    if name == 'crabxl':
                        assert sample['logical_temp_peak_bytes'] == tables[name]['worksheet_xml_bytes']
                    samples[name].append(sample)
        stats = subprocess.run(list(map(str, commands['crabxl'])), text=True, capture_output=True, check=True)
        charged = int(next(line.split()[1] for line in stats.stderr.splitlines() if line.startswith('BANK_BYTES ')))
        report['cases'].append({'cells': count, 'tables': tables, 'native_managed_bank_bytes': charged, 'samples': samples, 'medians': {name: {key: statistics.median(sample[key] for sample in values) for key in ('seconds', 'cpu_seconds', 'peak_rss_kib', 'sampled_temp_peak_bytes')} for name, values in samples.items()}})
        (HERE / 'results/m4-bank-styles.json').write_text(json.dumps(report, indent=2)+'\n')
        print(json.dumps(report['cases'][-1]['medians']), flush=True)

if __name__ == '__main__':
    if len(sys.argv) > 1 and sys.argv[1] == '--reference':
        reference(Path(sys.argv[2]), int(sys.argv[3]), materialized=True)
    else:
        main()
