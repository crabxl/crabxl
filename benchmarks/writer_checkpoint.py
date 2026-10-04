"""Sequential writer interoperability, native RSS, temporary storage and timings."""
import json
import os
import subprocess
import sys
import tempfile
import time
import warnings
import datetime
import platform
import posixpath
import zipfile
import xml.etree.ElementTree as ET
from pathlib import Path
import openpyxl

ROOT=Path(__file__).resolve().parents[1]
assert openpyxl.__version__=='3.1.5'
(ROOT/'benchmarks/data').mkdir(parents=True,exist_ok=True)
(ROOT/'benchmarks/results').mkdir(parents=True,exist_ok=True)
warnings.simplefilter('error')

def measure(command,directory):
    environment=dict(os.environ,TMPDIR=str(directory))
    process=subprocess.Popen([str(ROOT/'benchmarks/measure'),*map(str,command)],env=environment,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
    observed=0
    while process.poll() is None:
        total=0
        for path in directory.iterdir():
            try:total+=path.stat().st_size
            except FileNotFoundError:pass
        observed=max(observed,total)
        time.sleep(0.025)
    stdout,stderr=process.communicate()
    if process.returncode:raise RuntimeError(stderr)
    assert not list(directory.iterdir()),'Temporary worksheets leaked'
    return {'stdout':stdout.strip(),'observed_temp_bytes':observed,**json.loads(stderr.split('MEASURE ')[-1])}

def numeric_readback(path,count):
    result=subprocess.run([str(ROOT/'target/release/examples/sum'),str(path)],check=True,text=True,capture_output=True)
    cells=count*10
    assert result.stdout.strip()==f'{cells} {cells*(cells-1)//2}'

def python_readback(path,count,mixed):
    book=openpyxl.load_workbook(path,read_only=True)
    seen=0
    for row in book.active.values:
        expected=(9007199254740993,999999999999999999999999999999,1.25,bool(seen%2),'#DIV/0!',' repeated ',f'row{seen}','',None,seen) if mixed else tuple(seen*10+c for c in range(10))
        assert row==expected,(seen,row)
        if mixed:assert type(row[0]) is int and type(row[1]) is int and type(row[2]) is float and type(row[3]) is bool
        seen+=1
    book.close();assert seen==count

report={'platform':platform.platform(),'cpu':next((line.split(':',1)[1].strip() for line in Path('/proc/cpuinfo').read_text().splitlines() if line.startswith('model name')),'unknown'),'rust':'1.88.0','openpyxl':'3.1.5','measurement':'Linux native wait4, one warmup and three Rust runs; Python numeric writer warmup plus one measured run; TMPDIR sampled every 25ms, Rust also reports exact logical spooled XML bytes','cases':[], 'scope':'M3 sequential creation; core shared date/formula/basic-style types; M2 full style/date/shared-string reading remains staged'}
for count in [10000,100000,1000000]:
    path=ROOT/f'benchmarks/data/written-numeric-{count}.xlsx'
    runs=[]
    with tempfile.TemporaryDirectory(prefix='crabxl-benchmark-') as temp:
        directory=Path(temp)
        command=[ROOT/'target/release/examples/write_demo',path,count,'numeric',directory]
        measure(command,directory)
        for _ in range(3):
            result=measure(command,directory)
            rows,cells,peak=map(int,result.pop('stdout').split());assert (rows,cells)==(count,count*10)
            result['temp_bytes']=peak;result['output_bytes']=path.stat().st_size;runs.append(result)
        numeric_readback(path,count)
        if count<=100000:python_readback(path,count,False)
        baseline_path=ROOT/f'benchmarks/data/python-written-{count}.xlsx'
        baseline=[sys.executable,ROOT/'benchmarks/write_baseline.py',baseline_path,count]
        if count<=100000:measure(baseline,directory)
        python_result=measure(baseline,directory)
        assert python_result.pop('stdout')==f'{count} {count*10}'
        python_result['output_bytes']=baseline_path.stat().st_size
        numeric_readback(baseline_path,count)
    report['cases'].append({'workload':'numeric','rows':count,'columns':10,'rust_runs':runs,'python_run':python_result,'python_warmup':count<=100000,'cross_tool_python_verified':count<=100000})
    print('Numeric writer',count,'checksums and cleanup passed',flush=True)
for count in [10000,100000]:
    path=ROOT/f'benchmarks/data/written-mixed-{count}.xlsx';runs=[]
    with tempfile.TemporaryDirectory(prefix='crabxl-benchmark-') as temp:
        directory=Path(temp)
        command=[ROOT/'target/release/examples/write_demo',path,count,'mixed',directory]
        measure(command,directory)
        for _ in range(3):
            result=measure(command,directory)
            rows,cells,peak=map(int,result.pop('stdout').split());assert (rows,cells)==(count,count*10)
            result['temp_bytes']=peak;result['output_bytes']=path.stat().st_size;runs.append(result)
        python_readback(path,count,True)
        result=subprocess.run([str(ROOT/'target/release/examples/scalar_counts'),str(path)],check=True,text=True,capture_output=True)
        assert result.stdout.strip()==f'{count*4} {count} {count//2} {count} {count*3} {count}'
    report['cases'].append({'workload':'mixed','rows':count,'columns':10,'rust_runs':runs,'cross_tool_python_verified':True})
    print('Mixed writer',count,'exact values, types and cleanup passed',flush=True)
def verify_package(path):
    with zipfile.ZipFile(path) as archive:
        assert archive.testzip() is None
        names=set(archive.namelist())
        for name in names:
            if name.endswith('.xml') or name.endswith('.rels'):
                root=ET.fromstring(archive.read(name))
                if name.endswith('.rels'):
                    base='' if name=='_rels/.rels' else name.split('/_rels/')[0]
                    ids=set()
                    for rel in root:
                        assert rel.attrib['Id'] not in ids; ids.add(rel.attrib['Id'])
                        if rel.attrib.get('TargetMode')!='External':
                            target=rel.attrib['Target']
                            resolved=target.lstrip('/') if target.startswith('/') else posixpath.normpath(posixpath.join(base,target))
                            assert resolved in names,(name,resolved)
        ns={'s':'http://schemas.openxmlformats.org/spreadsheetml/2006/main'}
        root=ET.fromstring(archive.read('xl/styles.xml'))
        for tag in ['numFmts','fonts','fills','borders','cellXfs','cellStyleXfs','cellStyles']:
            collection=root.find('s:'+tag,ns)
            assert int(collection.attrib['count'])==len(collection)
        cells=ET.fromstring(archive.read('xl/worksheets/sheet1.xml')).find('s:sheetData/s:row',ns)
        assert cells[3].find('s:v',ns) is None, 'Missing formula cache was fabricated'
        assert cells[4].find('s:v',ns).text=='0'
        assert cells[5].attrib['t']=='b' and cells[5].find('s:v',ns).text=='0'

def feature_readback(path,count,epoch1904):
    expected_values=(datetime.datetime(2024,2,29,12,34,56,789000),datetime.time(6,30),datetime.timedelta(days=1.5),None,0,False,' cached ','#DIV/0!',1.25,' styled ')
    expected_formulas=expected_values[:3]+('=1+1','=0','=FALSE()','=" cached "','=1/0')+expected_values[8:]
    for data_only in [False,True]:
        book=openpyxl.load_workbook(path,read_only=True,data_only=data_only)
        assert book.epoch==datetime.datetime(1904,1,1) if epoch1904 else book.epoch==datetime.datetime(1899,12,30)
        seen=0
        for row in book.active.rows:
            assert tuple(cell.value for cell in row)==(expected_values if data_only else expected_formulas)
            assert type(row[4].value) is (int if data_only else str)
            assert type(row[5].value) is (bool if data_only else str)
            style=row[8]
            assert style.number_format=='0.00'
            assert style.font.bold and style.font.italic and style.font.underline=='single'
            assert style.font.name=='Calibri' and style.font.sz==11.0 and style.font.color.rgb=='FF123456'
            assert style.fill.patternType=='solid' and style.fill.fgColor.rgb=='FFFFE699'
            assert style.border.left.style=='thin' and style.border.left.color.rgb=='FFABCDEF'
            assert style.alignment.horizontal=='center' and style.alignment.vertical=='top' and style.alignment.wrap_text and style.alignment.textRotation==30
            assert style.protection.locked is False and style.protection.hidden is True
            seen+=1
        assert seen==count
        book.close()
for mode in ['features','features1904']:
    for count in [10000,100000]:
        path=ROOT/f'benchmarks/data/written-{mode}-{count}.xlsx'; runs=[]
        with tempfile.TemporaryDirectory(prefix='crabxl-benchmark-') as temp:
            directory=Path(temp)
            command=[ROOT/'target/release/examples/write_demo',path,count,mode,directory]
            measure(command,directory)
            for _ in range(3):
                result=measure(command,directory)
                rows,cells,peak=map(int,result.pop('stdout').split()); assert (rows,cells)==(count,count*10)
                result['temp_bytes']=peak; result['output_bytes']=path.stat().st_size; runs.append(result)
            verify_package(path)
            feature_readback(path,count,mode=='features1904')
        report['cases'].append({'workload':mode,'rows':count,'columns':10,'rust_runs':runs,'cross_tool_python_verified':True,'formula_and_data_only_verified':True,'basic_style_attributes_verified':True,'relationship_targets_and_collection_counts_verified':True})
        print('Date/formula/style writer',mode,count,'values, styles, relationships and cleanup passed',flush=True)
(ROOT/'benchmarks/results/m3-writer.json').write_text(json.dumps(report,indent=2)+'\n')
