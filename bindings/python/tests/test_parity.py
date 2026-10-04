"""One public-API test body runs against both implementations."""
from datetime import datetime, time, timedelta
import zipfile

import openpyxl
import openrsxl
import pytest


@pytest.fixture(params=[openpyxl, openrsxl], ids=["openpyxl", "openrsxl"])
def engine(request):
    return request.param


def test_scalar_types_formulas_dimensions_and_live_views(engine):
    workbook = engine.Workbook()
    sheet = workbook.active
    values = [None, True, False, 5, 1.25, " whitespace ", "#DIV/0!", "=1+2"]
    sheet.append(values)
    assert list(sheet.values) == [tuple(values)]
    assert [sheet.cell(1, column).data_type for column in range(1, 9)] == ["n", "b", "b", "n", "n", "s", "e", "f"]
    cell = sheet["D1"]
    assert sheet["D1"] is cell
    sheet.insert_rows(1, 2)
    assert cell.coordinate == "D3" and cell.value == 5
    sheet.delete_cols(1, 2)
    assert cell.coordinate == "B3" and sheet["B3"] is cell
    sheet.move_range("B3:C3", rows=1, cols=2)
    assert cell.coordinate == "D4" and sheet["D4"] is cell
    del sheet["D4"]
    assert cell.value == 5 and sheet["D4"].value is None
    cell.value = 42
    assert sheet["D4"].value is None


def test_new_save_public_readback_and_repeat(engine, tmp_path):
    workbook = engine.Workbook()
    sheet = workbook.active
    sheet.title = "Data"
    sheet.append([True, 5, 1.25, " text ", "#N/A", "=SUM(B1:C1)", datetime(2020, 2, 29, 12, 3, 4, 123000), time(12, 3, 4), timedelta(days=2, seconds=3)])
    workbook.create_sheet("Other").append(["second"])
    path = tmp_path / "new.xlsx"
    workbook.save(path)
    book = openpyxl.load_workbook(path)
    assert book.sheetnames == ["Data", "Other"]
    row = tuple(book["Data"].values)[0]
    assert row[:6] == (True, 5, 1.25, " text ", "#N/A", "=SUM(B1:C1)")
    assert row[6:] == (datetime(2020, 2, 29, 12, 3, 4, 123000), time(12, 3, 4), timedelta(days=2, seconds=3))
    book.close()
    workbook.save(tmp_path / "repeat.xlsx")
    assert set(tmp_path.iterdir()) == {path, tmp_path / "repeat.xlsx"}


def test_loaded_numeric_edit_and_sparse_insertion(engine, tmp_path):
    source = tmp_path / "source.xlsx"
    original = openpyxl.Workbook()
    original.active.append([1, 2, "=A1+B1"])
    original.save(source)
    workbook = engine.load_workbook(source)
    sheet = workbook.active
    assert sheet["B1"].value == 2
    sheet["A1"] = 5
    sheet["D5"] = "new"
    assert sheet["A1"].value == 5 and sheet["D5"].value == "new"
    assert sheet.max_row == 5 and sheet.max_column == 4
    assert list(sheet.iter_rows(min_row=5, max_row=5, values_only=True)) == [(None, None, None, "new")]
    output = tmp_path / "output.xlsx"
    workbook.save(output)
    workbook.save(tmp_path / "repeat.xlsx")
    workbook.close()
    verified = openpyxl.load_workbook(output)
    assert verified.active["A1"].value == 5 and verified.active["D5"].value == "new"
    assert verified.active["C1"].value == "=A1+B1"
    verified.close()


def test_loaded_formula_data_only(engine, tmp_path):
    source = tmp_path / "formula.xlsx"
    workbook = openpyxl.Workbook()
    workbook.active.append(["=1+2"])
    workbook.save(source)
    loaded = engine.load_workbook(source, data_only=True)
    assert loaded.active["A1"].value is None
    loaded.close()


def test_empty_append_cursor_and_dictionary_columns(engine):
    workbook = engine.Workbook()
    sheet = workbook.active
    sheet.append([])
    sheet.append({"C": 3})
    assert sheet["C2"].value == 3
    sheet.delete_rows(1, 2)
    sheet.append([5])
    assert sheet["A1"].value == 5


def test_loaded_style_image_comment_and_unknown_parts_are_preserved(tmp_path):
    from PIL import Image
    from openpyxl.drawing.image import Image as DrawingImage
    from openpyxl.styles import Font
    from openpyxl.comments import Comment
    image_path = tmp_path / "image.png"
    Image.new("RGB", (8, 8), "red").save(image_path)
    source = tmp_path / "source.xlsx"
    workbook = openpyxl.Workbook()
    workbook.active["A1"] = 1
    workbook.active["A1"].font = Font(bold=True)
    workbook.active["A1"].comment = Comment("comment", "author")
    workbook.active.add_image(DrawingImage(image_path), "D1")
    workbook.save(source)
    with zipfile.ZipFile(source, "a") as archive:
        archive.writestr("custom/unknown.xml", '<x xmlns="urn:custom">keep</x>')
    loaded = openrsxl.load_workbook(source)
    loaded.active["A1"] = 9
    for filename in ("first.xlsx", "second.xlsx"):
        target = tmp_path / filename
        loaded.save(target)
        with zipfile.ZipFile(source) as original, zipfile.ZipFile(target) as saved:
            for name in ("xl/media/image1.png", "xl/styles.xml", "custom/unknown.xml"):
                assert original.read(name) == saved.read(name)
        verified = openpyxl.load_workbook(target)
        assert verified.active["A1"].value == 9
        assert verified.active["A1"].font.bold
        assert verified.active["A1"].comment.text == "comment"
        verified.close()
    loaded.close()


def test_adapter_limits_exact_integers_closed_sources_and_unsupported_operations(tmp_path):
    workbook = openrsxl.Workbook(max_memory_bytes=1000)
    sheet = workbook.active
    sheet["A1"] = 10**100
    assert sheet["A1"].value == 10**100
    with pytest.raises(MemoryError):
        sheet["A1"] = "x" * 2000
    assert sheet["A1"].value == 10**100
    with pytest.raises(AttributeError):
        sheet["A1"].font = object()
    with pytest.raises(AttributeError):
        sheet.freeze_panes = "A1"
    with pytest.raises(NotImplementedError):
        openrsxl.Workbook(write_only=True)
    source = tmp_path / "source.xlsx"
    workbook.save(source)
    loaded = openrsxl.load_workbook(source)
    loaded.close()
    with pytest.raises(ValueError, match="closed"):
        loaded.active["A1"].value
    assert len(list(tmp_path.iterdir())) == 1


def test_atomic_new_output_failure_preserves_existing_target(tmp_path):
    target = tmp_path / "target.xlsx"
    target.write_bytes(b"original")
    workbook = openrsxl.Workbook()
    # All sheets removed is invalid for the writer. The target must survive.
    workbook.remove(workbook.active)
    with pytest.raises((ValueError, RuntimeError)):
        workbook.save(target)
    assert target.read_bytes() == b"original"
    assert list(tmp_path.iterdir()) == [target]


def test_sheet_names_and_order_match(engine):
    workbook = engine.Workbook()
    workbook.create_sheet("Sheet2")
    next_sheet = workbook.create_sheet()
    assert next_sheet.title == "Sheet3"
    next_sheet.title = "Data"
    workbook.create_sheet("data", index=0)
    assert workbook.sheetnames == ["data1", "Sheet", "Sheet2", "Data"]
    assert "Data" in workbook
    assert [sheet.title for sheet in workbook] == workbook.sheetnames
    workbook.remove(workbook["Sheet2"])
    assert workbook.sheetnames == ["data1", "Sheet", "Data"]


@pytest.mark.parametrize("source,expected", [
    ("=A1+$B2+C$3+$D$4", "=B2+$B3+D$3+$D$4"),
    ("='A1'!A1+A1!B2", "='A1'!B2+A1!C3"),
    ("=T1[A1]+T2[[#Headers],[B2]]+A1", "=T1[A1]+T2[[#Headers],[B2]]+B2"),
    ('="A1"&"say ""B2"""+A1', '="A1"&"say ""B2"""+B2'),
    ("=LOG10(A1)+SUM(A1:B2:C3)", "=LOG10(B2)+SUM(B2:C3:D4)"),
    ("=XFD1048576", "=XFE1048577"),
    ("=AA1001001001+R1C1+named1", "=AA1001001001+R1C1+named1"),
])
def test_formula_translation_context(engine, source, expected):
    from importlib import import_module
    translator = import_module(engine.__name__ + ".formula.translate").Translator
    assert translator(source, "A1").translate_formula("B2") == expected


def test_formula_move_translation_and_failure_atomicity(engine):
    workbook = engine.Workbook()
    sheet = workbook.active
    sheet["B2"] = "=C3+$D$4"
    sheet.move_range("B2", rows=1, cols=1, translate=True)
    assert sheet["C3"].value == "=D4+$D$4"
    assert sheet["B2"].value is None
    if engine is openrsxl:
        sheet["C4"] = "=A1"
        before = tuple(sheet.values)
        with pytest.raises(ValueError):
            sheet.move_range("C3:C4", rows=-1, translate=True)
        assert tuple(sheet.values) == before


def test_active_sheet_creation_and_loaded_catalog_match(engine, tmp_path):
    workbook = engine.Workbook()
    first = workbook.active
    second = workbook.create_sheet("Other")
    first["A1"] = 1
    second["A1"] = 2
    workbook.active = second
    assert workbook.active is second
    path = tmp_path / "active.xlsx"
    workbook.save(path)
    verified = openpyxl.load_workbook(path)
    assert verified.active.title == "Other"
    verified.close()
    loaded = engine.load_workbook(path)
    assert loaded.active.title == "Other" and loaded.active["A1"].value == 2
    loaded.close()


def test_owned_workbook_copy_order_removed_aliases_and_independent_values(engine, tmp_path):
    workbook = engine.Workbook()
    source = workbook.active
    source.title = "Data"
    source.append([1, "text", "=A1+1"])
    source.append([])
    copied = workbook.copy_worksheet(source)
    assert copied.title == "Data Copy"
    assert copied.parent is workbook and copied["C1"].value == "=A1+1"
    copied["A1"] = 7
    assert source["A1"].value == 1
    assert workbook.copy_worksheet(source).title == "Data Copy1"
    workbook.move_sheet(copied, offset=-1)
    assert workbook.sheetnames == ["Data Copy", "Data", "Data Copy1"]
    assert workbook.index(source) == 1
    workbook.active = source
    cell = copied["A1"]
    workbook.remove(copied)
    assert cell.value == 7 and copied["A1"] is cell
    cell.value = 8
    assert copied["A1"].value == 8
    assert workbook.sheetnames == ["Data", "Data Copy1"]
    del workbook["Data Copy1"]
    assert workbook.active is None
    workbook.active = source
    target = tmp_path / "bank.xlsx"
    workbook.save(target)
    verified = openpyxl.load_workbook(target)
    assert verified.sheetnames == ["Data"] and verified.active["A1"].value == 1
    verified.close()
    with pytest.raises(ValueError):
        workbook.copy_worksheet(engine.Workbook().active)


@pytest.mark.parametrize("offset", [-10, -4, -2, -1, 0, 1, 2, 10])
def test_move_sheet_offsets_match_list_insertion(engine, offset):
    workbook = engine.Workbook()
    workbook.create_sheet("B")
    source = workbook.create_sheet("C")
    expected = workbook.worksheets
    old = expected.index(source)
    expected.remove(source)
    expected.insert(old + offset, source)
    workbook.move_sheet("C", offset=offset)
    assert workbook.worksheets == expected


def test_python_bank_aggregate_limit_atomic_copy_and_freed_space():
    workbook = openrsxl.Workbook(max_memory_bytes=1600)
    first = workbook.active
    first["A1"] = 1
    second = workbook.create_sheet("B")
    second["A1"] = 2
    assert workbook._book.charged_bytes() == 1030
    with pytest.raises(MemoryError):
        workbook.copy_worksheet(first)
    assert workbook.sheetnames == ["Sheet", "B"]
    first["A2"] = 3
    second["A2"] = 4
    before = workbook._book.charged_bytes()
    with pytest.raises(MemoryError):
        second["A3"] = 5
    assert workbook._book.charged_bytes() == before and not second._native.contains(2, 0)
    with pytest.raises(MemoryError):
        first.insert_rows(1)
    assert first["A1"].value == 1
    workbook.remove(first)
    assert first["A1"].value == 1
    # Detached models remain caller-owned, outside the bank's allowance.
    second["A3"] = 5
    assert second["A3"].value == 5
    first["A1"] = 9
    assert first["A1"].value == 9


def test_native_bank_concurrent_copies_keep_owned_handles_valid():
    from concurrent.futures import ThreadPoolExecutor
    from openrsxl._native import NativeBook
    book = NativeBook(10_000_000)
    source = book.create_sheet("Source")
    for row in range(400):
        source.append([("int", str(row))])

    def copies(worker):
        for iteration in range(10):
            copied = book.copy_sheet(source, f"Copy{worker}-{iteration}")
            assert copied.get(0, 0) == ("n", 0)
            assert copied.get(399, 0) == ("n", 399)
            book.remove_sheet(copied)
            assert copied.get(399, 0) == ("n", 399)
        return worker

    with ThreadPoolExecutor(max_workers=4) as pool:
        assert list(pool.map(copies, range(4))) == [0, 1, 2, 3]
    assert source.get(399, 0) == ("n", 399)
    assert book.charged_bytes() < 10_000_000
