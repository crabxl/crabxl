"""Serial canonical printing creation/editing and independent public read/export probes."""
import argparse
from pathlib import Path
import zipfile
from openpyxl.worksheet.page import PageMargins, PrintOptions, PrintPageSetup
from openpyxl.worksheet.pagebreak import RowBreak, ColBreak, Break
from openpyxl.worksheet.properties import PageSetupProperties
from openpyxl.xml.functions import fromstring
from worksheet_feature_checkpoint import run_checkpoint, HERE

def settings(sheet):
    sheet.sheet_properties.pageSetUpPr = PageSetupProperties(autoPageBreaks=False, fitToPage=True)
    sheet.page_margins = PageMargins(left=-1.5, right=.25, top=.5, bottom=.75, header=.2, footer=.3)
    sheet.print_options = PrintOptions(horizontalCentered=True, verticalCentered=False, headings=True, gridLines=False, gridLinesSet=True)
    sheet.page_setup = PrintPageSetup(orientation="landscape", paperSize=9, scale=85, fitToHeight=0, fitToWidth=1, firstPageNumber=7, useFirstPageNumber=True, paperHeight="11.5in tail", paperWidth="8.25in", pageOrder="overThenDown", usePrinterDefaults=False, blackAndWhite=True, draft=False, cellComments="atEnd", errors="NA", horizontalDpi=300, verticalDpi=600, copies=2)
    sheet.row_breaks = RowBreak(brk=[Break(id=10, min=2, max=7, man=False, pt=True), Break(id=25), Break(id=None, min=None, max=None, man=False, pt=False)])
    sheet.col_breaks = ColBreak(brk=[Break(id=3, min=1, max=1048575, man=None, pt=False)])

def signature(path):
    with zipfile.ZipFile(path) as archive:
        root = fromstring(archive.read("xl/worksheets/sheet1.xml"))
    ns = "{http://schemas.openxmlformats.org/spreadsheetml/2006/main}"
    result = {}
    for tag, cls in [("pageMargins", PageMargins), ("printOptions", PrintOptions), ("pageSetup", PrintPageSetup), ("rowBreaks", RowBreak), ("colBreaks", ColBreak)]:
        element = root.find(ns + tag)
        value = cls() if element is None else cls.from_tree(element)
        record = {key: item for key, item in vars(value).items() if key not in ("_parent", "brk")}
        if tag in ("rowBreaks", "colBreaks"):
            if element is not None:
                assert element.attrib["count"] == str(len(value.brk))
                assert element.attrib["manualBreakCount"] == str(len(value.brk))
            record["brk"] = [vars(entry) for entry in value.brk]
            record["count"] = value.count
            record["manualBreakCount"] = value.manualBreakCount
        result[tag] = record
    element = root.find(ns + "sheetPr/" + ns + "pageSetUpPr")
    result["pageSetUpPr"] = vars(PageSetupProperties() if element is None else PageSetupProperties.from_tree(element))
    return result

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rows", type=int, nargs="+", default=[5000, 50000])
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--output", type=Path, default=HERE / "results/m5-printing.json")
    parser.add_argument("--python-operation", choices=["create", "edit"])
    parser.add_argument("--source", type=Path)
    parser.add_argument("--target", type=Path)
    args = parser.parse_args()
    run_checkpoint(args, {"configure": settings, "signature": signature, "name": "printing", "example": "printing", "script": Path(__file__).resolve(), "telemetry": "PRINT_BYTES", "managed_key": "managed_print_bytes", "semantics": "Create: sequential spools in both engines. Edit: native bounded original-package overlay with full source metadata validation/decompression plus streamed save, reference ordinary loaded workbook. Identical public margins/options/page setup/properties/breaks and every numeric value verified; ownership costs differ. Native reader scans full worksheet to find printing tail without loading cells; source read/export interoperability checked outside timing. 25ms temporary samples are lower bounds and exclude final ZIP; native creation reports exact spool peak, native edit adds none. No upstream comparison: source literal dimensions and optional page/break fields have not yet been mapped to an equivalent upstream public workload."})

if __name__ == "__main__": main()
