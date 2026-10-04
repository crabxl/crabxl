"""Check native creation using reference public values and format-defined XML."""
import json
import sys
import zipfile
from xml.etree import ElementTree as ET
import openpyxl
from openpyxl.cell.rich_text import CellRichText, TextBlock

assert openpyxl.__version__ == "3.1.5"
path = sys.argv[1]
text = " <&> _x005F_x0041_ \r\n🦀 "
book = openpyxl.load_workbook(path, rich_text=True)
value = book["Sheet"]["A1"].value
assert isinstance(value, CellRichText) and len(value) == 3
assert isinstance(value[0], TextBlock) and value[0].text == text.replace("x005F_", "")
assert value[1] == "tail"
assert isinstance(value[2], TextBlock) and value[2].text == ""
font = value[0].font
expected = {"rFont": 'Quoted " &\t\n\r', "sz": 12.5, "b": True, "i": False,
    "strike": False, "outline": True, "shadow": True, "condense": False,
    "extend": True, "u": "doubleAccounting", "vertAlign": "superscript",
    "charset": 128, "family": 3, "scheme": "minor"}
for key, wanted in expected.items():
    assert getattr(font, key) == wanted, (key, getattr(font, key), wanted)
assert font.color.type == "rgb" and font.color.rgb == "80445566"
assert font.color.tint == -0.25
book.close()
book = openpyxl.load_workbook(path, rich_text=False)
assert book["Sheet"]["A1"].value == text + "tail"
book.close()
# The public reference projects phonetics away; verify native creation in OOXML.
ns = {"s": "http://schemas.openxmlformats.org/spreadsheetml/2006/main"}
with zipfile.ZipFile(path) as archive:
    root = ET.fromstring(archive.read("xl/worksheets/sheet1.xml"))
phonetic = root.find(".//s:rPh", ns)
assert phonetic.attrib == {"sb": "0", "eb": "2"}
assert phonetic.find("s:t", ns).text == " pronunciation "
settings = root.find(".//s:phoneticPr", ns)
assert settings.attrib == {"fontId": "0", "type": "Hiragana", "alignment": "center"}
print(json.dumps({"reference": "openpyxl 3.1.5 public API", "typed_runs": 3,
    "explicit_font_fields_verified": len(expected), "color": "80445566, tint -0.25",
    "plain_projection": "verified", "phonetic_xml": "verified", "literal_marker": "raw in plain mode, protection removed per typed run"}))
