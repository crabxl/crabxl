"""Serial creation and original-package view editing with public feature verification."""
import argparse
from pathlib import Path
import zipfile
from openpyxl.worksheet.views import SheetView, SheetViewList, Pane
from worksheet_feature_checkpoint import run_checkpoint

ROOT = Path(__file__).resolve().parents[1]
HERE = ROOT / "benchmarks"


def settings(sheet):
    sheet.freeze_panes = "B3"
    view = sheet.sheet_view
    for name, value in dict(windowProtection=False, showFormulas=True, showGridLines=False,
                            showRowColHeaders=False, showZeros=False, rightToLeft=True,
                            tabSelected=True, showRuler=False, showOutlineSymbols=False,
                            defaultGridColor=False, showWhiteSpace=False, zoomToFit=True,
                            view="pageLayout", topLeftCell="B3", colorId=12, zoomScale=145,
                            zoomScaleNormal=80, zoomScaleSheetLayoutView=90,
                            zoomScalePageLayoutView=95).items():
        setattr(view, name, value)
    view.selection[2].activeCellId = 2
    view.selection[2].sqref = "A1 C3:D4"
    sheet.views.sheetView.append(SheetView(workbookViewId=2, pane=Pane(
        xSplit=-1.5, ySplit=2.5, topLeftCell="opaque", activePane="bottomRight", state="frozenSplit")))


def signature(views):
    """Public descriptor values and source order, with load-time defaults applied."""
    records = []
    for view in views.sheetView:
        record = {key: value for key, value in vars(view).items() if key not in ("pane", "selection")}
        record["pane"] = None if view.pane is None else vars(view.pane)
        record["selection"] = [vars(value) for value in view.selection]
        records.append(record)
    return records


def file_signature(path):
    with zipfile.ZipFile(path) as archive:
        from openpyxl.xml.functions import fromstring
        from xml.etree import ElementTree as ET
        root = ET.fromstring(archive.read("xl/worksheets/sheet1.xml"))
        element = root.find("{http://schemas.openxmlformats.org/spreadsheetml/2006/main}sheetViews")
        actual = SheetViewList.from_tree(fromstring(ET.tostring(element)))
        return signature(actual)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rows", type=int, nargs="+", default=[5000, 50000])
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--output", type=Path, default=HERE / "results/m5-worksheet-views.json")
    parser.add_argument("--python-operation", choices=["create", "edit"])
    parser.add_argument("--source", type=Path)
    parser.add_argument("--target", type=Path)
    args = parser.parse_args()
    run_checkpoint(args, {"configure": settings, "signature": file_signature, "name": "views", "example": "worksheet_views", "script": Path(__file__).resolve(), "telemetry": "VIEW_BYTES", "managed_key": "managed_view_bytes", "semantics": "Create: sequential spools in both engines. Edit: native bounded original-package overlay, reference ordinary loaded workbook. Both output identical public view properties and every numeric value. No claim of equivalent ownership cost. Read/export interoperability checked outside timing. 25ms temporary sampling is a lower bound and excludes final ZIP; native creation reports exact worksheet spool peak. No upstream comparison: multiple views and arbitrary public attributes are not yet mapped to an equivalent upstream public workload."})

if __name__ == "__main__": main()
