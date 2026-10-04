"""Verify indexed palette spelling against public openpyxl color objects."""
import os
from pathlib import Path
import subprocess
import tempfile
from xml.etree import ElementTree as ET
import zipfile
import openpyxl
from openpyxl.styles.colors import ColorList, RgbColor

ROOT = Path(__file__).resolve().parents[1]
NS = '{http://schemas.openxmlformats.org/spreadsheetml/2006/main}'
LITERALS = ['ff11aa22', 'FF11AA22', 'abc123', 'AbC123', '00ff00FF']

def palette(path):
    with zipfile.ZipFile(path) as archive:
        styles = ET.fromstring(archive.read('xl/styles.xml'))
    return ColorList.from_tree(styles.find(NS + 'colors')).index

def main():
    with tempfile.TemporaryDirectory() as directory:
        source = Path(directory) / 'source.xlsx'
        output = Path(directory) / 'output.xlsx'
        workbook = openpyxl.Workbook()
        workbook.active['A1'] = 1
        workbook.save(source)
        with zipfile.ZipFile(source) as archive:
            parts = {name: archive.read(name) for name in archive.namelist()}
        styles = ET.fromstring(parts['xl/styles.xml'])
        styles.remove(styles.find(NS + 'colors'))
        colors = ET.SubElement(styles, NS + 'colors')
        indexed = ET.SubElement(colors, NS + 'indexedColors')
        for literal in LITERALS:
            ET.SubElement(indexed, NS + 'rgbColor', rgb=literal)
        parts['xl/styles.xml'] = ET.tostring(styles)
        with zipfile.ZipFile(source, 'w', compression=zipfile.ZIP_DEFLATED) as archive:
            for name, value in parts.items():
                archive.writestr(name, value)
        expected = [RgbColor(rgb=value).rgb for value in LITERALS]
        assert palette(source) == expected
        target = Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target'))
        subprocess.run([target / 'release/examples/palette_literal_export', source, output], check=True)
        assert palette(output) == expected
        reopened = openpyxl.load_workbook(output)
        assert reopened.active['A1'].value == 1
        reopened.close()
        print(f'{len(expected)} indexed palette literals match public openpyxl serialization')

if __name__ == '__main__':
    main()
