# Copyright (c) 2010-2024 openpyxl. MIT License.
# Selected release tests; exact methods/assertions/decorators retained.
# Imports/fixture construction target openrsxl instead of reference internals.
# See third_party/python-tests.json and third_party/licenses/openpyxl-MIT.txt.
import pytest
from itertools import islice
from openrsxl import Workbook, Worksheet as NativeWorksheet, Cell
from openrsxl.worksheet.cell_range import CellRange

@pytest.fixture
def Worksheet():
    return NativeWorksheet

@pytest.fixture
def dummy_worksheet(Worksheet):
    ws = Worksheet(Workbook())
    for row in ws.iter_rows(max_row=6, max_col=8):
        for cell in row:
            cell.value = cell.coordinate
    return ws

class TestWorksheet:

    def test_new_worksheet(self, Worksheet):
        wb = Workbook()
        ws = Worksheet(wb)
        assert ws.parent == wb

    def test_get_cell(self, Worksheet):
        ws = Worksheet(Workbook())
        cell = ws.cell(row=1, column=1)
        assert cell.coordinate == 'A1'

    def test_invalid_cell(self, Worksheet):
        wb = Workbook()
        ws = Worksheet(wb)
        with pytest.raises(ValueError):
            ws.cell(row=0, column=0)

    def test_worksheet_dimension(self, Worksheet):
        ws = Worksheet(Workbook())
        assert 'A1:A1' == ws.calculate_dimension()
        ws['B12'].value = 'AAA'
        assert 'B12:B12' == ws.calculate_dimension()

    @pytest.mark.parametrize("row, column, coordinate",
                             [
                                 (1, 0, 'A1'),
                                 (9, 2, 'C9'),
                             ])
    def test_fill_rows(self, Worksheet, row, column, coordinate):
        ws = Worksheet(Workbook())
        ws['A1'] = 'first'
        ws['C9'] = 'last'
        assert ws.calculate_dimension() == 'A1:C9'
        rows = ws.iter_rows()
        first_row = next(islice(rows, row - 1, row))
        assert first_row[column].coordinate == coordinate

    def test_iter_rows(self, Worksheet):
        ws = Worksheet(Workbook())
        expected = [
            ('A1', 'B1', 'C1'),
            ('A2', 'B2', 'C2'),
            ('A3', 'B3', 'C3'),
            ('A4', 'B4', 'C4'),
        ]

        rows = ws.iter_rows(min_row=1, min_col=1, max_row=4, max_col=3)
        for row, coord in zip(rows, expected):
            assert tuple(c.coordinate for c in row) == coord

    def test_cell_alternate_coordinates(self, Worksheet):
        ws = Worksheet(Workbook())
        cell = ws.cell(row=8, column=4)
        assert 'D8' == cell.coordinate

    def test_cell_insufficient_coordinates(self, Worksheet):
        ws = Worksheet(Workbook())
        with pytest.raises(TypeError):
            ws.cell(row=8)

    def test_append(self, Worksheet):
        ws = Worksheet(Workbook())
        ws.append(['value'])
        assert ws['A1'].value == "value"

    def test_append_list(self, Worksheet):
        ws = Worksheet(Workbook())

        ws.append(['This is A1', 'This is B1'])

        assert 'This is A1' == ws['A1'].value
        assert 'This is B1' == ws['B1'].value

    def test_append_dict_letter(self, Worksheet):
        ws = Worksheet(Workbook())

        ws.append({'A' : 'This is A1', 'C' : 'This is C1'})

        assert 'This is A1' == ws['A1'].value
        assert 'This is C1' == ws['C1'].value

    def test_append_dict_index(self, Worksheet):
        ws = Worksheet(Workbook())

        ws.append({1 : 'This is A1', 3 : 'This is C1'})

        assert 'This is A1' == ws['A1'].value
        assert 'This is C1' == ws['C1'].value

    def test_bad_append(self, Worksheet):
        ws = Worksheet(Workbook())
        with pytest.raises(TypeError):
            ws.append("test")

    def test_append_range(self, Worksheet):
        ws = Worksheet(Workbook())
        ws.append(range(30))
        assert ws['AD1'].value == 29

    def test_append_iterator(self, Worksheet):
        def itty():
            for i in range(30):
                yield i

        ws = Worksheet(Workbook())
        gen = itty()
        ws.append(gen)
        assert ws['AD1'].value == 29

    def test_append_2d_list(self, Worksheet):

        ws = Worksheet(Workbook())

        ws.append(['This is A1', 'This is B1'])
        ws.append(['This is A2', 'This is B2'])

        expected = (
            ('This is A1', 'This is B1'),
            ('This is A2', 'This is B2'),
        )
        for e, v in zip(expected, ws.values):
            assert e == tuple(v)

    def test_values(self, Worksheet):
        ws = Worksheet(Workbook())
        ws.append([1, 2, 3])
        ws.append([4, 5, 6])
        vals = ws.values
        assert next(vals) == (1, 2, 3)
        assert next(vals) == (4, 5, 6)

    def test_getitem(self, Worksheet):
        ws = Worksheet(Workbook())
        c = ws['A1']
        assert isinstance(c, Cell)
        assert c.coordinate == "A1"
        assert ws['A1'].value is None

    @pytest.mark.parametrize("key", [
        slice(None, None),
        slice(None, -1),
        ":",
        "A0",
        ]
    )
    def test_getitem_invalid(self, Worksheet, key):
        ws = Worksheet(Workbook())
        with pytest.raises((IndexError, ValueError)):
            ws[key]

    def test_setitem(self, Worksheet):
        ws = Worksheet(Workbook())
        ws['A12'] = 5
        assert ws['A12'].value == 5

class TestEditableWorksheet:

    def test_insert_rows(self, dummy_worksheet):
        ws = dummy_worksheet

        ws.insert_rows(2, 2)

        assert ws.max_row == 8
        assert ws._current_row == 8
        assert [c.value for c in ws[2]] == [None]*8

    def test_insert_cols(self, dummy_worksheet):
        ws = dummy_worksheet

        ws.insert_cols(3)

        assert ws.max_column == 9
        assert [c.value for c in ws['G']] == ['F1', 'F2', 'F3', 'F4', 'F5', 'F6']

    def test_delete_rows(self, dummy_worksheet):
        ws = dummy_worksheet

        ws.delete_rows(2, 3)

        assert ws.max_row == 3
        assert ws._current_row == 3
        assert [c.value for c in ws['B']] == ['B1', 'B5', 'B6']

    def test_deleta_all_rows(self, dummy_worksheet):
        ws = dummy_worksheet

        ws.delete_rows(1, 6)

        assert ws.max_row == 1
        assert ws._current_row == 0

    def test_delete_cols(self, dummy_worksheet):
        ws = dummy_worksheet

        ws.delete_cols(5, 2)

        assert ws.max_column == 6
        assert [c.value for c in ws[3]] == ['A3', 'B3', 'C3', 'D3', 'G3', 'H3']

    def test_delete_missing_cols(self, dummy_worksheet):
        ws = dummy_worksheet
        del ws['H2']

        ws.delete_cols(7)

        assert ws['G2'].value is None

    def test_delete_missing_rows(self, dummy_worksheet):
        ws = dummy_worksheet
        del ws['B4']

        ws.delete_rows(3)

        assert ws['B3'].value is None

    def test_delete_last_col(self, dummy_worksheet):
        ws = dummy_worksheet
        ws.delete_cols(8)
        assert ws.max_column == 7
        assert ws['H8'].value == None

    def test_delete_last_row(self, dummy_worksheet):
        ws = dummy_worksheet
        ws.delete_rows(6)
        assert ws.max_row == 5
        assert ws['A6'].value == None

    def test_move_nothing(self, dummy_worksheet):
        ws = dummy_worksheet
        ws.move_range("B2:E5")
        assert ws['B2'].value == "B2"

    def test_move_range_down(self, dummy_worksheet):
        ws = dummy_worksheet
        cr = CellRange("B2:E5")
        ws.move_range(cr, rows=2)
        assert ws['B4'].value == "B2"
        assert cr.coord == "B4:E7"

    def test_move_range_up(self, dummy_worksheet):
        ws = dummy_worksheet
        cr = CellRange("B4:E5")
        ws.move_range(cr, rows=-2)
        assert ws['B2'].value == "B4"
        assert cr.coord == "B2:E3"

    def test_move_range_right(self, dummy_worksheet):
        ws = dummy_worksheet
        cr = CellRange("B2:E5")
        ws.move_range(cr, cols=2)
        assert ws['D2'].value == "B2"
        assert cr.coord == "D2:G5"

    def test_move_range_left(self, dummy_worksheet):
        ws = dummy_worksheet
        cr = CellRange("D2:E5")
        ws.move_range(cr, cols=-2)
        assert ws['B2'].value == "D2"
        assert cr.coord == "B2:C5"

    def test_move_empty_range(self, dummy_worksheet):
        ws = dummy_worksheet
        cr = CellRange("A7:E15")
        ws.move_range(cr, rows=-2)
        assert ws['A6'].value is None
        assert cr.coord == "A5:E13"

    def test_move_range_from_string(self, dummy_worksheet):
        ws = dummy_worksheet
        ws.move_range("B2:E5", rows=2)
        assert ws['B4'].value == "B2"

    def test_move_range_with_formula(self, dummy_worksheet):
        ws = dummy_worksheet
        ws['G4'] = "=SUM(G1:G3)"
        ws.move_range("G4", 1, 1, True)
        assert ws['H5'].value == "=SUM(H2:H4)"
