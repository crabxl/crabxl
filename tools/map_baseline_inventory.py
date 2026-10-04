"""Map every pinned public surface and documentation topic into staged delivery."""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CATALOG = ROOT / 'docs/research/openpyxl-3.1.5-public-surface.json'
INVENTORY = ROOT / 'docs/features.json'

def stage(name):
    if any(part in name for part in ['chart', 'drawing', 'pivot', 'external', '.ole']):
        return 'M6'
    if any(part in name for part in ['styles', 'formatting', 'comments', 'table', 'validation', 'filters', 'protection', 'print', 'page', 'header', 'defined_name', 'views', 'scenario']):
        return 'M5'
    if name.startswith(('openpyxl.workbook', 'openpyxl.worksheet', 'openpyxl.packaging')):
        return 'M4'
    if '.writer' in name or name.startswith('openpyxl.writer'):
        return 'M3'
    if name.startswith(('openpyxl.compat', 'openpyxl.descriptors')):
        return 'M7'
    return 'M2'

def main():
    catalog = json.loads(CATALOG.read_text())
    inventory = json.loads(INVENTORY.read_text())
    inventory['items'] = [item for item in inventory['items'] if not item['id'].startswith(('planned.', 'baseline.', 'document.'))]
    for module, surface in catalog['modules'].items():
        inventory['items'].append({
            'id': 'baseline.' + module,
            'description': 'Public types, callable behavior, fields, and exports of ' + module,
            'status': 'planned', 'milestone': stage(module),
            'capabilities': dict.fromkeys(['read', 'create', 'edit', 'preserve'], 'planned'),
            'modes': ['mode-specific support to verify'],
            'reference': 'docs/research/openpyxl-3.1.5-public-surface.json',
            'reference_module': module,
            'public_classes': surface['classes'],
            'public_functions': sorted(surface['functions']),
            'public_exports': surface['exports'],
            'tests': [],
            'limitations': 'Inventory coverage, not implemented support. Python descriptors/compatibility scaffolding map to Rust typed models, validation, or later adapters; spreadsheet semantics and fields remain required. Read/create/edit/preserve and mode applicability require individual verification.',
        })
    for document, content in catalog['documents'].items():
        inventory['items'].append({
            'id': 'document.' + document,
            'description': 'Release documentation topics in ' + document,
            'status': 'planned', 'milestones': ['M2', 'M3', 'M4', 'M5', 'M6', 'M7'],
            'capabilities': dict.fromkeys(['read', 'create', 'edit', 'preserve'], 'planned'),
            'reference': document, 'headings': content['headings'], 'sha256': content['sha256'],
            'tests': [], 'limitations': 'Topic audit index; introductory/development documents do not imply file feature support. Cross-reference module entries and roadmap when verifying behavior.',
        })
    inventory['inventory_complete'] = True
    inventory['inventory_scope'] = 'Complete pinned runtime public-module/class/member/function/export and release RST-topic mapping; this is a feature audit index, not a claim of complete behavioral specification or implementation.'
    inventory['policy'] = 'Every cataloged public surface and release documentation topic stays in staged scope. Verified entries are separate, narrow checkpoints. Planned baseline entries do not inherit verified status from a related reader feature. Language-specific calls map through future adapters.'
    assert {x['reference_module'] for x in inventory['items'] if 'reference_module' in x} == set(catalog['modules'])
    assert {x['reference'] for x in inventory['items'] if x['id'].startswith('document.')} == set(catalog['documents'])
    assert len({x['id'] for x in inventory['items']}) == len(inventory['items'])
    INVENTORY.write_text(json.dumps(inventory, indent=2) + '\n')
    print(len(catalog['modules']), 'module mappings;', len(catalog['documents']), 'document mappings; no unmapped catalog entries')

if __name__ == '__main__':
    main()
