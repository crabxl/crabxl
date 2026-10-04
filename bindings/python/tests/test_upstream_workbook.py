# Copyright (c) 2010-2024 openpyxl. MIT License.
# Selected pinned test bodies and decorators are unchanged.
# See third_party/python-workbook-tests.json.
import pytest
from openrsxl import Workbook as AdapterWorkbook, Worksheet

@pytest.fixture
def Workbook():
    return AdapterWorkbook

def test_get_active_sheet(Workbook):
    wb = Workbook()
    assert wb.active == wb.worksheets[0]


def test_set_active_by_sheet(Workbook):
    wb = Workbook()
    names = ['Sheet', 'Sheet1', 'Sheet2',]
    for n in names:
        wb.create_sheet(n)

    for n in names:
        sheet = wb[n]
        wb.active = sheet
        assert wb.active == wb[n]


def test_set_active_by_index(Workbook):
    wb = Workbook()
    names = ['Sheet', 'Sheet1', 'Sheet2',]
    for n in names:
        wb.create_sheet(n)

    for idx, name in enumerate(names):
        wb.active = idx
        assert wb.active == wb.worksheets[idx]


def test_set_invalid_sheet_by_name(Workbook):
    wb = Workbook()
    with pytest.raises(TypeError):
        wb.active = "Sheet"


def test_set_invalid_child_as_active(Workbook):
    wb1 = Workbook()
    wb2 = Workbook()
    ws2 = wb2['Sheet']
    with pytest.raises(ValueError):
        wb1.active = ws2


def test_create_sheet(Workbook):
    wb = Workbook()
    new_sheet = wb.create_sheet()
    assert new_sheet == wb.worksheets[-1]


def test_create_sheet_with_name(Workbook):
    wb = Workbook()
    new_sheet = wb.create_sheet(title='LikeThisName')
    assert new_sheet == wb.worksheets[-1]


def test_remove_sheet(Workbook):
    wb = Workbook()
    new_sheet = wb.create_sheet(0)
    wb.remove(new_sheet)
    assert new_sheet not in wb.worksheets


def test_move_sheet(Workbook):
    wb = Workbook()
    for i in range(9):
        wb.create_sheet()
    assert wb.sheetnames == ['Sheet', 'Sheet1', 'Sheet2', 'Sheet3', 'Sheet4',
                            'Sheet5', 'Sheet6', 'Sheet7', 'Sheet8', 'Sheet9']
    wb.move_sheet("Sheet9", -5)
    assert wb.sheetnames == ['Sheet', 'Sheet1', 'Sheet2', 'Sheet3', 'Sheet9',
                            'Sheet4', 'Sheet5', 'Sheet6', 'Sheet7', 'Sheet8']


def test_getitem(Workbook):
    wb = Workbook()
    ws = wb['Sheet']
    assert isinstance(ws, Worksheet)
    with pytest.raises(KeyError):
        wb['NotThere']


def test_del_worksheet(Workbook):
    wb = Workbook()
    del wb['Sheet']
    assert wb.worksheets == []


def test_contains(Workbook):
    wb = Workbook()
    assert "Sheet" in wb
    assert "NotThere" not in wb


def test_iter(Workbook):
    wb = Workbook()
    for ws in wb:
        pass
    assert ws.title == "Sheet"


def test_index(Workbook):
    wb = Workbook()
    new_sheet = wb.create_sheet()
    sheet_index = wb.index(new_sheet)
    assert sheet_index == 1


def test_get_sheet_names(Workbook):
    wb = Workbook()
    names = ['Sheet', 'Sheet1', 'Sheet2', 'Sheet3', 'Sheet4', 'Sheet5']
    for count in range(5):
        wb.create_sheet(0)
    assert wb.sheetnames == names


class TestCopy:

    def test_worksheet_copy(self, Workbook):
        wb = Workbook()
        ws1 = wb.active
        ws2 = wb.copy_worksheet(ws1)
        assert ws2 is not None


    @pytest.mark.parametrize("title, copy",
                             [
                                 ("TestSheet", "TestSheet Copy"),
                                 (u"D\xfcsseldorf", u"D\xfcsseldorf Copy")
                                 ]
                             )
    def test_worksheet_copy_name(self, title, copy, Workbook):
        wb = Workbook()
        ws1 = wb.active
        ws1.title = title
        ws2 = wb.copy_worksheet(ws1)
        assert ws2.title == copy

