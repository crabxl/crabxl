"""Exact mixed-scalar fidelity, resource measurements, and numeric regression probe."""
import argparse
import io
import json
import subprocess
import zipfile
from pathlib import Path
import openpyxl

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--numeric-before', required=True, help='Release M1 sum executable')
parser.add_argument('--output', default=str(ROOT / 'benchmarks/results/m2-scalars.json'))
args = parser.parse_args()
assert openpyxl.__version__ == '3.1.5'

def measure(command):
    result = subprocess.run([str(ROOT / 'benchmarks/measure'), *map(str, command)], capture_output=True, text=True, check=True)
    return {'output':result.stdout.strip(), **json.loads(result.stderr.split('MEASURE ')[-1])}

def generate(path, count):
    book = openpyxl.Workbook()
    source = io.BytesIO()
    book.save(source)
    with zipfile.ZipFile(source) as template, zipfile.ZipFile(path,'w',compression=zipfile.ZIP_DEFLATED) as out:
        for name in template.namelist():
            if name != 'xl/worksheets/sheet1.xml': out.writestr(name,template.read(name))
        with out.open('xl/worksheets/sheet1.xml','w') as sheet:
            sheet.write(b'<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>')
            for row in range(count):
                sheet.write((f'<row r="{row+1}"><c><v>9007199254740993</v></c><c><v>999999999999999999999999999999</v></c><c><v>1.25</v></c><c t="b"><v>{row%2}</v></c><c t="e"><v>#DIV/0!</v></c><c t="inlineStr"><is><t xml:space="preserve"> repeated </t></is></c><c t="inlineStr"><is><t>row{row}</t></is></c><c t="inlineStr"><is><t/></is></c><c/><c><v>{row}</v></c></row>').encode())
            sheet.write(b'</sheetData></worksheet>')

def verify_python(path,count):
    book = openpyxl.load_workbook(path,read_only=True)
    seen=0
    for row in book.active.values:
        assert row == (9007199254740993,999999999999999999999999999999,1.25,bool(seen%2),'#DIV/0!',' repeated ',f'row{seen}','',None,seen),row
        assert type(row[0]) is int and type(row[2]) is float and type(row[3]) is bool
        seen+=1
    book.close()
    assert seen==count

report={'rust':'1.88.0','openpyxl':'3.1.5','before_revision':'74c082a58e8b','measurement':'Native Linux wait4 RSS; three alternating measured runs, one warmup; fixture generation/public API verification excluded','runtime_temporary_bytes':0,'cases':[]}
for count in [10000,100000]:
    path=ROOT/f'benchmarks/data/scalars-{count}.xlsx'
    generate(path,count)
    verify_python(path,count)
    command=[ROOT/'target/release/examples/scalar_counts',path]
    expected=f'{count*4} {count} {count//2} {count} {count*3} {count}'
    assert measure(command)['output']==expected
    runs=[measure(command) for _ in range(3)]
    assert all(run['output']==expected for run in runs)
    report['cases'].append({'workload':'mixed','rows':count,'columns':10,'input_bytes':path.stat().st_size,'runs':runs})
    print('Mixed exact scalar fidelity',count,'passed',flush=True)
for count in [100000,1000000]:
    path=ROOT/f'benchmarks/data/numbers-{count}.xlsx'
    commands={'M1':[args.numeric_before,path],'M2':[ROOT/'target/release/examples/sum',path]}
    expected=measure(commands['M1'])['output']
    assert measure(commands['M2'])['output']==expected
    runs=[]
    for index in range(3):
        for version in (['M1','M2'] if index%2==0 else ['M2','M1']):
            value=measure(commands[version]); assert value['output']==expected
            runs.append({'version':version,**value})
    report['cases'].append({'workload':'numeric','rows':count,'columns':10,'runs':runs})
    print('Numeric',count,'passed',flush=True)
Path(args.output).parent.mkdir(parents=True,exist_ok=True)
Path(args.output).write_text(json.dumps(report,indent=2)+'\n')
