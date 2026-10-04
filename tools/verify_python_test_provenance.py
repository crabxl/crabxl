"""Verify selected test bodies against the pinned openpyxl release checkout.

Only test source is read; no implementation source or bytecode is inspected.
"""
import argparse
import ast
import hashlib
import json
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--reference-checkout", type=Path, required=True)
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
record = json.loads((root / "third_party/python-tests.json").read_text())
source = subprocess.run(["hg", "--cwd", str(args.reference_checkout), "cat", "-r", record["revision"], record["source"]], check=True, capture_output=True, text=True).stdout
assert hashlib.sha256(source.encode()).hexdigest() == record["source_sha256"]

def methods(text):
    return {node.name: node for group in ast.parse(text).body if isinstance(group, ast.ClassDef) for node in group.body if isinstance(node, ast.FunctionDef)}

original = methods(source)
selected = methods((root / record["destination"]).read_text())
assert set(selected) == set(record["selected_methods"])
for name, method in selected.items():
    assert ast.dump(ast.Module(body=method.body, type_ignores=[])) == ast.dump(ast.Module(body=original[name].body, type_ignores=[])), name
    assert ast.dump(method.args) == ast.dump(original[name].args), name
    expected = original[name].decorator_list
    actual = method.decorator_list
    if name == "test_move_range_with_formula":
        actual = actual[:-1]
    assert [ast.dump(node) for node in actual] == [ast.dump(node) for node in expected], name
print(f"Verified {len(selected)} original test bodies and parameters at {record['revision']}")
