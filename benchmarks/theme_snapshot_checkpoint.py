"""Immutable native theme holders; common-driver clone and content verification."""
import argparse
import json
import os
import platform
import statistics
import tempfile
from pathlib import Path
from shared_strings_checkpoint import ROOT, HERE
from iso_checkpoint import measure

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline', type=Path, required=True)
    args = parser.parse_args()
    target = Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target'))
    report = {'baseline_core': '9a13237f06b349eef12ac2f358b4eee4c66f1d19', 'platform': platform.platform(), 'semantics': '64 immutable holders cloned from one theme. Source is dropped before verifying every byte in every holder. New ownership shares the immutable payload; old ownership copies it. Caller-held snapshots are distinct from library-owned workbook allowances. Native ownership extension only: no cross-package speed or binding claim. One warmup and five rotating serial cold-process samples; no builds or temporary storage.', 'cases': []}
    for size in (65536, 1048576):
        commands = {'crabxl': [target / 'release/examples/theme_snapshots', size], 'crabxl-before': [args.baseline.resolve(), size]}
        samples = {name: [] for name in commands}
        with tempfile.TemporaryDirectory() as directory:
            for command in commands.values():
                output, _ = measure(command, Path(directory))
                assert output == str(size*64)
            names = list(commands)
            for iteration in range(5):
                for name in names[iteration % 2:] + names[:iteration % 2]:
                    output, sample = measure(commands[name], Path(directory))
                    assert output == str(size*64)
                    assert sample['sampled_temp_peak_bytes'] == 0
                    samples[name].append(sample)
        report['cases'].append({'payload_bytes': size, 'holders': 64, 'verified_bytes': size*64, 'samples': samples, 'medians': {name: {key: statistics.median(sample[key] for sample in values) for key in ('seconds', 'cpu_seconds', 'peak_rss_kib', 'sampled_temp_peak_bytes')} for name, values in samples.items()}})
        (HERE / 'results/m4-theme-snapshots.json').write_text(json.dumps(report, indent=2)+'\n')
        print(json.dumps(report['cases'][-1]['medians']), flush=True)

if __name__ == '__main__':
    main()
