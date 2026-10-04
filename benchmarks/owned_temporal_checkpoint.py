"""Equivalent owned temporal assignment and a separate streaming regression."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import sys
import tempfile
import openpyxl
from iso_checkpoint import measure
from temporal_style_checkpoint import ROOT, HERE, generate, records, verify

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline', type=Path, required=True)
    args = parser.parse_args()
    target = Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target'))
    report = {'reference': openpyxl.__version__, 'platform': platform.platform(),
        'baseline_core': '26f4f5d7d18aaf36a2669174d925e71041f51159',
        'baseline_sha256': hashlib.sha256(args.baseline.read_bytes()).hexdigest(),
        'measurement': 'One warmup and five rotating serial cold-process samples; no builds/tests during timing; all output values and style properties checked outside timing',
        'semantics': 'Owned engines retain all cells before save. Native assigns canonical style IDs before save and exports by ownership transfer. Public ordinary cells set the source format before the temporal value. Streaming regression is separate: native and public stream rows, with prior native binary predating palette case preservation. No prior owned temporal API baseline is fabricated. Final ZIP is outside monitored TMPDIR; sampling is a lower bound and cleanup is asserted.', 'cases': []}
    with tempfile.TemporaryDirectory() as name:
        root = Path(name)
        temporary = root / 'temporary'
        temporary.mkdir()
        reference = root / 'reference.xlsx'
        generate(reference, 4, streaming=False)
        expected = records(reference)
        for mode in ('owned', 'streaming'):
            for rows in (1000, 10000):
                names = ['crabxl', 'openpyxl'] + (['crabxl-before'] if mode == 'streaming' else [])
                paths = {engine: root / f'{engine}.xlsx' for engine in names}
                commands = {'crabxl': [target / 'release/examples/temporal_style_fixture', paths['crabxl'], rows] + (['owned'] if mode == 'owned' else []),
                    'openpyxl': [sys.executable, __file__, '--reference', paths['openpyxl'], rows, mode]}
                if mode == 'streaming':
                    commands['crabxl-before'] = [args.baseline.resolve(), paths['crabxl-before'], rows]
                samples = {engine: [] for engine in commands}
                for engine, command in commands.items():
                    output, _ = measure(command, temporary)
                    assert output == str(rows*5)
                    verify(paths[engine], rows, expected)
                for iteration in range(5):
                    for engine in names[iteration % len(names):] + names[:iteration % len(names)]:
                        output, sample = measure(commands[engine], temporary)
                        assert output == str(rows*5)
                        verify(paths[engine], rows, expected)
                        sample['output_bytes'] = paths[engine].stat().st_size
                        samples[engine].append(sample)
                case = {'mode': mode, 'cells': rows*5, 'samples': samples,
                    'medians': {engine: {key: statistics.median(sample[key] for sample in values) for key in ('seconds', 'cpu_seconds', 'peak_rss_kib', 'sampled_temp_peak_bytes')} for engine, values in samples.items()}}
                report['cases'].append(case)
                (HERE / 'results/m4-owned-temporal-styles.json').write_text(json.dumps(report, indent=2)+'\n')
                print(mode, rows, case['medians'], flush=True)

if __name__ == '__main__':
    if len(sys.argv) == 5 and sys.argv[1] == '--reference':
        generate(Path(sys.argv[2]), int(sys.argv[3]), streaming=sys.argv[4] == 'streaming')
        print(int(sys.argv[3])*5)
    else:
        main()
