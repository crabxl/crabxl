import importlib, inspect, json, pkgutil, subprocess, hashlib
from pathlib import Path
import openpyxl
assert openpyxl.__version__=='3.1.5'
import argparse
parser=argparse.ArgumentParser(description='Catalog public runtime metadata and pinned release documentation without reading implementation source')
parser.add_argument('--reference-checkout',default='/workspace/openpyxl')
parser.add_argument('--output',default='docs/research/openpyxl-3.1.5-public-surface.json')
args=parser.parse_args()
rev='13627b03ca25a1a98becf40e533b955615b13429'
modules={}
for name in ['openpyxl']+sorted(m.name for m in pkgutil.walk_packages(openpyxl.__path__,'openpyxl.')):
 module=importlib.import_module(name)
 classes={}; functions={}; exports={}
 for attr,obj in inspect.getmembers(module):
  if attr.startswith('_'): continue
  if inspect.isclass(obj) or inspect.isfunction(obj):
   origin=getattr(obj,'__module__','')
   if not origin.startswith('openpyxl'): continue
   exports[attr]=origin+'.'+getattr(obj,'__name__',attr)
   if origin!=name: continue
   if inspect.isclass(obj):
    classes[attr]=sorted(member for member,_ in inspect.getmembers_static(obj) if not member.startswith('_'))
   else:
    try: functions[attr]=[{'name':p.name,'kind':p.kind.name,'has_default':p.default is not inspect.Parameter.empty} for p in inspect.signature(obj).parameters.values()]
    except (ValueError,TypeError): functions[attr]=[]
 modules[name]={'classes':classes,'functions':functions,'exports':exports}
files=subprocess.check_output(['hg','--cwd',args.reference_checkout,'files','-r',rev,'glob:doc/**.rst'],text=True).splitlines()
docs={}
for file in files:
 content=subprocess.check_output(['hg','--cwd',args.reference_checkout,'cat','-r',rev,file]).decode()
 headings=[]; lines=content.splitlines()
 for i,line in enumerate(lines[:-1]):
  under=lines[i+1].strip()
  if line.strip() and len(under)>=3 and len(set(under))==1 and under[0] in '=~-^"*+#' and len(under)>=len(line.strip()): headings.append(line.strip())
 docs[file]={'sha256':hashlib.sha256(content.encode()).hexdigest(),'headings':headings}
report={'baseline':{'version':openpyxl.__version__,'revision':rev},'method':'Public runtime attributes and callable parameter names only; no implementation source, bytecode or call graphs inspected. RST headings/hashes from pinned release documentation. Underscore modules are architecture metadata; public members of their public classes are cataloged. This catalog is an audit index, not implementation support.','modules':modules,'documents':docs}
Path(args.output).write_text(json.dumps(report,indent=2,sort_keys=True)+'\n')
print(len(modules),'modules',sum(len(m['classes']) for m in modules.values()),'classes',len(docs),'documents')
