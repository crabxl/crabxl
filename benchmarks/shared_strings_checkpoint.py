"""Equivalent full text verification across RAM/disk/Auto, calamine and openpyxl."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys
import tempfile
import zipfile

import openpyxl

ROOT = Path(__file__).resolve().parents[1]
HERE = ROOT / "benchmarks"
MAIN = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
PKG = "http://schemas.openxmlformats.org/package/2006/relationships"


def run(command):
    return subprocess.run(list(map(str, command)), cwd=ROOT, text=True, capture_output=True, check=True)


def generate(path, rows, unique):
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("[Content_Types].xml", '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/></Types>')
        archive.writestr("_rels/.rels", f'<Relationships xmlns="{PKG}"><Relationship Id="book" Type="{REL}/officeDocument" Target="xl/workbook.xml"/></Relationships>')
        archive.writestr("xl/workbook.xml", f'<workbook xmlns="{MAIN}" xmlns:r="{REL}"><sheets><sheet name="Sheet" sheetId="1" r:id="sheet"/></sheets></workbook>')
        archive.writestr("xl/_rels/workbook.xml.rels", f'<Relationships xmlns="{PKG}"><Relationship Id="sheet" Type="{REL}/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="strings" Type="{REL}/sharedStrings" Target="sharedStrings.xml"/></Relationships>')
        with archive.open("xl/sharedStrings.xml", "w") as output:
            output.write(f'<sst xmlns="{MAIN}" count="{rows*10}" uniqueCount="{unique}">'.encode())
            for index in range(unique):
                output.write(f'<si><t>item-{index:08}-{"x"*96}</t></si>'.encode())
            output.write(b'</sst>')
        with archive.open("xl/worksheets/sheet1.xml", "w") as output:
            output.write(f'<worksheet xmlns="{MAIN}"><dimension ref="A1:J{rows}"/><sheetData>'.encode())
            for row in range(rows):
                output.write(f'<row r="{row+1}">'.encode())
                for col in range(10):
                    output.write(f'<c r="{chr(65+col)}{row+1}" t="s"><v>{(row*10+col)%unique}</v></c>'.encode())
                output.write(b'</row>')
            output.write(b'</sheetData></worksheet>')


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rows",type=int,nargs="+",default=[10000,100000])
    parser.add_argument("--runs",type=int,default=5)
    parser.add_argument("--output",type=Path,default=HERE/"shared-strings.local.json")
    args=parser.parse_args()
    if openpyxl.__version__!="3.1.5" or args.runs<1 or any(not 0<r<=1048576 for r in args.rows):
        parser.error("Require pinned openpyxl, positive runs and Excel row bounds")
    run(["cc","-O2","-Wall","-Wextra","-Werror",HERE/"measure.c","-o",HERE/"measure"])
    run(["cargo","build","--release","--locked","-p","crabxl","--example","shared_text"])
    run(["cargo","build","--release","--locked","--manifest-path",HERE/"calamine/Cargo.toml","--bin","shared_text"])
    target=Path(os.environ.get("CARGO_TARGET_DIR",ROOT/"target"))
    report={"workload":"10 columns of 110-byte ASCII strings; every value verified in order","measurement":"Linux fork/exec/wait4; one warmup; five rotating serial runs; generation/build excluded; runtime baseline included","versions":{"openpyxl":openpyxl.__version__,"calamine":"0.36.1","rust":run(["rustc","--version"]).stdout.strip()},"platform":platform.platform(),"semantics":"crabxl streams rows after SST preparation; openpyxl read-only eagerly reads SST; calamine materializes public Range; equivalent text verification, different retention modes","cases":[]}
    data=HERE/"data";data.mkdir(exist_ok=True)
    for rows in args.rows:
        for kind,unique in [("repeated",128),("unique",rows*10)]:
            path=data/f"strings-{kind}-{rows}.xlsx";generate(path,rows,unique)
            with tempfile.TemporaryDirectory(prefix="crabxl-string-bench-") as temp:
                commands={f"crabxl-{policy}":[target/"release/examples/shared_text",path,policy,unique,temp] for policy in ("memory","disk","auto")}
                commands.update({"calamine":[target/"release/shared_text" if os.environ.get("CARGO_TARGET_DIR") else HERE/"calamine/target/release/shared_text",path,unique],"openpyxl":[sys.executable,HERE/"read_shared_openpyxl.py",path,unique]})
                def measure(command):
                    result=run([HERE/"measure",*command])
                    if result.stdout.strip()!=f"{rows*10} {rows*10*110}":raise RuntimeError(result.stdout)
                    if list(Path(temp).iterdir()):raise RuntimeError("Owned temporary files leaked")
                    sample=json.loads(result.stderr.split("MEASURE ")[-1])
                    for line in result.stderr.splitlines():
                        if line.startswith("STRINGS "):sample["strings"]=json.loads(line[8:])
                    return sample
                for command in commands.values():measure(command)
                samples={name:[] for name in commands};names=list(commands)
                for index in range(args.runs):
                    for name in names[index%len(names):]+names[:index%len(names)]:
                        sample=measure(commands[name]);samples[name].append(sample)
                        print(rows,kind,name,sample,flush=True)
                report["cases"].append({"rows":rows,"cells":rows*10,"unique_strings":unique,"kind":kind,"file_bytes":path.stat().st_size,"file_sha256":hashlib.sha256(path.read_bytes()).hexdigest(),"samples":samples,"medians":{name:{key:statistics.median(s[key] for s in values) for key in ("seconds","cpu_seconds","peak_rss_kib")} for name,values in samples.items()}})
            args.output.parent.mkdir(parents=True,exist_ok=True);args.output.write_text(json.dumps(report,indent=2)+"\n")

if __name__=="__main__":main()
