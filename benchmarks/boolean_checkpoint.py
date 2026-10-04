"""M2 scalar correctness/RSS probe and numeric hot-path regression measurement."""
import argparse
import json
import subprocess
from pathlib import Path
import openpyxl

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--numeric-before', required=True, help='Release sum executable built at M1 checkpoint')
parser.add_argument('--output', default=str(ROOT / 'benchmarks/results/m2-boolean.json'))
args = parser.parse_args()
assert openpyxl.__version__ == '3.1.5'
launcher = ROOT / 'benchmarks/measure'

def run(command):
    result = subprocess.run([str(launcher), *map(str, command)], text=True, capture_output=True, check=True)
    metrics = json.loads(next(line[8:] for line in result.stderr.splitlines() if line.startswith('MEASURE ')))
    metrics['output'] = result.stdout.strip()
    return metrics

report = {'rust': '1.88.0', 'openpyxl': openpyxl.__version__, 'm1_revision': '74c082a58e8b', 'runs': 3, 'warmup': 1, 'temporary_storage_bytes': 0, 'note': 'Numeric comparison uses identical sum executables/workloads before and after boolean dispatch; booleans generated through public write-only API. Inputs and retained results reside on disk, no runtime temporary files.', 'workloads': []}
for count in [10000, 100000]:
    path = ROOT / f'benchmarks/data/booleans-{count}.xlsx'
    book = openpyxl.Workbook(write_only=True)
    sheet = book.create_sheet('Sheet')
    for row in range(count):
        sheet.append([bool((row + column) % 2) for column in range(10)])
    book.save(path)
    command = [ROOT / 'target/release/examples/scalar_counts', path]
    expected = f'0 {count*10} {count*5} 0 0 0'
    assert run(command)['output'] == expected
    results = [run(command) for _ in range(3)]
    assert all(result['output'] == expected for result in results)
    report['workloads'].append({'type':'booleans','rows':count,'columns':10,'input_bytes':path.stat().st_size,'results':results})
    print('Boolean', count, 'verified', flush=True)
for count in [100000, 1000000]:
    path = ROOT / f'benchmarks/data/numbers-{count}.xlsx'
    commands = {'before':[args.numeric_before,path], 'after':[ROOT / 'target/release/examples/sum',path]}
    expected = run(commands['before'])['output']
    assert run(commands['after'])['output'] == expected
    results = []
    for index in range(3):
        for implementation in (['before','after'] if index % 2 == 0 else ['after','before']):
            result = run(commands[implementation])
            assert result['output'] == expected
            results.append({'implementation':implementation,**result})
    report['workloads'].append({'type':'numeric-regression','rows':count,'columns':10,'results':results})
    print('Numeric', count, 'verified', flush=True)
Path(args.output).parent.mkdir(parents=True, exist_ok=True)
Path(args.output).write_text(json.dumps(report,indent=2)+'\n')
