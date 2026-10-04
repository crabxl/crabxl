"""Owned workbook model/export and Python writer regression checkpoint."""
import argparse
import json
import os
from pathlib import Path
import statistics
import subprocess
import sys
import tempfile
import time
ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--before-python-path', type=Path, required=True)
args = parser.parse_args()
report = {'scope': 'Owned sparse workbook, stable sheet IDs, aggregate allowances and active-tab metadata; not loaded structural editing', 'baseline': 'Python writer before owned-workbook checkpoint at c50eca7', 'cases': []}
def measure(command, env, directory):
    p = subprocess.Popen([str(ROOT/'benchmarks/measure'), *map(str, command)], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=env)
    peak = 0
    while p.poll() is None:
        total = 0
        for f in directory.iterdir():
            if f.name == 'output.xlsx': continue
            try: total += f.stat().st_size
            except FileNotFoundError: pass
        peak = max(peak, total)
        time.sleep(.01)
    stdout, stderr = p.communicate()
    if p.returncode: raise RuntimeError(stderr)
    result = json.loads(stderr.split('MEASURE ')[-1]); result.update(result=stdout.strip(), observed_temp_bytes=peak)
    return result
for rows in (10000, 100000):
    with tempfile.TemporaryDirectory(prefix='openrsxl-workbook-') as name:
        directory = Path(name); output = directory/'output.xlsx'
        env = dict(os.environ, TMPDIR=name)
        commands = {'openrsxl-native': [ROOT/'target/release/examples/workbook_demo', rows, output], 'openpyxl': [sys.executable, ROOT/'benchmarks/workbook_reference_run.py', rows, output]}
        for command in commands.values(): measure(command, env, directory)
        runs = []
        for iteration in range(3):
            for engine in (list(commands) if iteration%2==0 else list(reversed(commands))):
                run = measure(commands[engine], env, directory); model = json.loads(run.pop('result'))
                cells = rows*10
                assert model['cells'] == 2*cells and model['sum'] == cells*(cells-1)
                import openpyxl
                book = openpyxl.load_workbook(output, read_only=True)
                assert book.sheetnames == ['Copy', 'Renamed'] and book.active.title == 'Renamed'
                for sheet in book:
                    count = checksum = 0
                    for row in sheet.values:
                        for value in row: count += 1; checksum += value
                    assert count == cells and checksum == cells*(cells-1)//2
                book.close()
                run.update(engine=engine, model=model, output_bytes=output.stat().st_size, cleanup=list(directory.iterdir())==[output])
                assert run['cleanup']; runs.append(run)
        report['cases'].append({'workload': 'owned-workbook', 'rows': rows, 'columns': 10, 'sheets': 2, 'runs': runs})
        print(rows, 'owned workbook verified', flush=True)
        command = [sys.executable, ROOT/'benchmarks/python_adapter_run.py', 'openrsxl', 'create', rows, 'unused', output]
        envs = {'before': dict(env, PYTHONPATH=str(args.before_python_path)), 'current': env}
        for environment in envs.values(): measure(command, environment, directory)
        runs=[]
        for iteration in range(3):
            for version in (list(envs) if iteration%2==0 else list(reversed(envs))):
                run=measure(command,envs[version],directory); assert run.pop('result')=='Saved'
                checksum=subprocess.run([ROOT/'target/release/examples/sum', output],check=True,capture_output=True,text=True).stdout.strip()
                cells=rows*10; assert checksum==f'{cells} {cells*(cells-1)//2}'
                run.update(version=version,checksum=checksum,output_bytes=output.stat().st_size,cleanup=list(directory.iterdir())==[output]);assert run['cleanup'];runs.append(run)
        report['cases'].append({'workload':'writer-regression','rows':rows,'columns':10,'runs':runs})
        print(rows,'writer',[(v,statistics.median(r['seconds'] for r in runs if r['version']==v)) for v in envs],flush=True)
(ROOT/'benchmarks/results/m4-workbook.json').write_text(json.dumps(report,indent=2)+'\n')
