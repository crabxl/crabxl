"""An openpyxl-compatible adapter over the independent Rust core.

Compatibility is verified per capability. Unsupported features raise explicitly;
this package never falls back to the Python openpyxl implementation.
"""
from datetime import date, datetime, time, timedelta
import math
from pathlib import Path
import re
from weakref import WeakValueDictionary

from ._native import NativeEditor, NativeReader, NativeSheet, cell_address, column_index, column_letters, finite_range, resolve_model_budget, save_models

__version__ = "0.1.0"
_ERRORS = {"#NULL!", "#DIV/0!", "#VALUE!", "#REF!", "#NAME?", "#NUM!", "#N/A", "#GETTING_DATA"}
_ADDRESS = re.compile(r"^\$?([A-Za-z]+)\$?([1-9][0-9]*)$")


def _column(value):
    if isinstance(value, int):
        if not 1 <= value <= 16384:
            raise ValueError("Column index must be between 1 and 16384")
        return value
    if isinstance(value, str) and value.isascii() and value.isalpha():
        return column_index(value)
    raise ValueError("Invalid column index")


def _letters(column):
    return column_letters(_column(column))


def _address(value):
    if not isinstance(value, str):
        raise TypeError("A cell coordinate must be a string")
    return cell_address(value)


def _range(value):
    if not isinstance(value, str):
        raise TypeError("A cell range must be a string")
    return finite_range(value)


def _encode(value):
    if value is None:
        return "empty", None
    if isinstance(value, bool):
        return "bool", value
    if isinstance(value, int):
        return "int", str(value)
    if isinstance(value, float):
        if not math.isfinite(value):
            raise NotImplementedError("Non-finite numeric compatibility is not implemented")
        return "float", value
    if isinstance(value, str):
        value = value[:32767]
        if any(ord(char) < 32 and char not in "\t\n\r" for char in value):
            from .utils.exceptions import IllegalCharacterError
            raise IllegalCharacterError("Text contains an illegal XML character")
        if re.search(r"_[xX][0-9a-fA-F]{4}_", value):
            raise NotImplementedError("OOXML escape-aware text is not implemented")
        if value.startswith("=") and len(value) > 1:
            return "formula", value[1:]
        return ("error" if value in _ERRORS else "text"), value
    if isinstance(value, datetime):
        if value.tzinfo is not None:
            raise TypeError("Excel does not support timezones in datetimes")
        if value.microsecond % 1000:
            raise NotImplementedError("Sub-millisecond date creation is not implemented")
        return "datetime", (value.year, value.month, value.day, value.hour, value.minute, value.second, value.microsecond // 1000)
    if isinstance(value, date):
        raise NotImplementedError("Date-only object/format compatibility is not implemented; datetime values are supported")
    if isinstance(value, time):
        if value.tzinfo is not None:
            raise TypeError("Excel does not support timezones in times")
        return "time", (value.hour * 3600 + value.minute * 60 + value.second + value.microsecond / 1000000) / 86400
    if isinstance(value, timedelta):
        return "duration", value.total_seconds() / 86400
    raise ValueError(f"Cannot convert {type(value).__name__} to Excel")


def _decode(tagged):
    kind, value = tagged
    if kind == "bigint":
        return int(value)
    if kind == "datetime":
        return datetime.fromisoformat(value)
    if kind == "time":
        microseconds = round(value * 86400 * 1000000)
        seconds, microseconds = divmod(microseconds, 1000000)
        return time(seconds // 3600, seconds // 60 % 60, seconds % 60, microseconds)
    if kind == "duration":
        return timedelta(days=value)
    return value


class Cell:
    """A live Python view of a Rust-owned cell; coordinates are one-based."""
    __slots__ = ("parent", "row", "column", "_detached", "__weakref__")

    def __init__(self, worksheet, row, column):
        self.parent, self.row, self.column = worksheet, row, column
        self._detached = None

    @property
    def coordinate(self):
        return f"{_letters(self.column)}{self.row}"

    @property
    def column_letter(self):
        return _letters(self.column)

    @property
    def value(self):
        return _decode(self._tagged())

    @value.setter
    def value(self, value):
        if self._detached is not None:
            tag = _encode(value)
            self._detached = (_data_type(tag), value)
        else:
            self.parent._set(self.row, self.column, value)

    def _tagged(self):
        return self._detached if self._detached is not None else self.parent._get(self.row, self.column)

    @property
    def data_type(self):
        kind = self._tagged()[0]
        return "d" if kind in ("datetime", "duration", "time") else "n" if kind == "bigint" else kind

    @property
    def internal_value(self):
        return self.value

    @property
    def is_date(self):
        return self.data_type == "d"


def _data_type(tag):
    return {"empty": "n", "bool": "b", "int": "n", "float": "n", "text": "s", "error": "e", "formula": "f"}.get(tag[0], "d")


class Worksheet:
    """Sparse worksheet using the same public call conventions as openpyxl."""
    __slots__ = ("parent", "_title", "_existing", "_native", "_cells")

    def __init__(self, parent, title=None, *, _existing=False):
        self.parent = parent
        self._title = (title or "Sheet") if _existing else parent._unique_title(title or "Sheet")
        self._existing = _existing
        self._native = None if _existing else NativeSheet(self._title, parent._max_bytes)
        self._cells = WeakValueDictionary()
        self._validate_title(self._title)

    @staticmethod
    def _validate_title(title):
        if not isinstance(title, str) or not title:
            raise ValueError("Title must have at least one character")
        if re.search(r"[\\*?:/\[\]]", title):
            raise ValueError("Invalid character in sheet title")
        if len(title) > 31:
            raise NotImplementedError("Titles longer than 31 characters are not implemented")

    @property
    def title(self):
        return self._title

    @title.setter
    def title(self, title):
        self._validate_title(title)
        if self._existing:
            raise NotImplementedError("Renaming existing sheets is not implemented")
        if title != self._title:
            title = self.parent._unique_title(title, exclude=self)
            self._native.rename(title)
            self._title = title

    def _model(self):
        self.parent._check_open()
        if self._native is None:
            self._native = self.parent._reader.load_sheet(self.title, self.parent._max_bytes, self.parent.data_only)
        if self._existing:
            self.parent._editor.apply(self.title, self._native)
        return self._native

    def _get(self, row, column):
        self.parent._check_open()
        if self._existing:
            value = self.parent._editor.pending(self.title, row - 1, column - 1)
            if value is not None:
                return value
        return self._model().get(row - 1, column - 1)

    def _set(self, row, column, value):
        self.parent._check_open()
        tagged = _encode(value)
        if self._existing:
            self.parent._editor.set(self.title, row - 1, column - 1, tagged)
        else:
            self._native.set(row - 1, column - 1, tagged)

    def cell(self, row, column, value=None):
        if not isinstance(row, int) or not isinstance(column, int) or not 1 <= row <= 1048576 or not 1 <= column <= 16384:
            raise ValueError("Row or column values must be within Excel bounds")
        key = row, column
        cell = self._cells.get(key)
        if cell is None:
            cell = Cell(self, row, column)
            self._cells[key] = cell
        if not self._existing and not self._native.contains(row - 1, column - 1):
            self._native.set(row - 1, column - 1, ("empty", None))
        if value is not None:
            cell.value = value
        return cell

    def __getitem__(self, key):
        if isinstance(key, int):
            return tuple(self.iter_rows(min_row=key, max_row=key))[0]
        if isinstance(key, str) and ":" not in key and _ADDRESS.fullmatch(key):
            return self.cell(*_address(key))
        if isinstance(key, str) and key.isalpha():
            column = _column(key)
            return tuple(self.iter_cols(min_col=column, max_col=column))[0]
        if isinstance(key, str) and ":" in key:
            first, last = key.split(":", 1)
            if first.isdigit() and last.isdigit():
                return tuple(self.iter_rows(min_row=int(first), max_row=int(last)))
            if first.isalpha() and last.isalpha():
                return tuple(self.iter_cols(min_col=_column(first), max_col=_column(last)))
            row, col, end_row, end_col = _range(key)
            return tuple(self.iter_rows(row, end_row, col, end_col))
        raise ValueError(f"{key} is not a valid coordinate or range")

    def __setitem__(self, key, value):
        row, column = _address(key)
        self.cell(row, column).value = value

    def __delitem__(self, key):
        if self._existing:
            raise NotImplementedError("Deleting existing physical cells is not implemented")
        row, column = _address(key)
        cell = self._cells.pop((row, column), None)
        if cell is not None:
            cell._detached = cell._tagged()
        self._model().remove(row - 1, column - 1)

    @property
    def _current_row(self): return self._model().row_extent()

    @property
    def min_row(self): return self._model().bounds()[0]
    @property
    def min_column(self): return self._model().bounds()[1]
    @property
    def max_row(self): return self._model().bounds()[2]
    @property
    def max_column(self): return self._model().bounds()[3]

    def calculate_dimension(self):
        first_row, first_col, last_row, last_col = self._model().bounds()
        return f"{_letters(first_col)}{first_row}:{_letters(last_col)}{last_row}"

    @property
    def dimensions(self): return self.calculate_dimension()

    def iter_rows(self, min_row=None, max_row=None, min_col=None, max_col=None, values_only=False):
        bounds = self._model().bounds()
        if min_row is None and max_row is None and min_col is None and max_col is None and not self._native.contains(0, 0) and bounds == (1, 1, 1, 1):
            return iter(())
        min_row, min_col = min_row or 1, min_col or 1
        max_row, max_col = max_row or bounds[2], max_col or bounds[3]
        self.cell(min_row, min_col)
        if max_row < min_row or max_col < min_col:
            return iter(())
        def rows():
            for row in range(min_row, max_row + 1):
                cells = tuple(self.cell(row, column) for column in range(min_col, max_col + 1))
                yield tuple(cell.value for cell in cells) if values_only else cells
        return rows()

    def iter_cols(self, min_col=None, max_col=None, min_row=None, max_row=None, values_only=False):
        min_col, min_row = min_col or 1, min_row or 1
        max_col, max_row = max_col or self.max_column, max_row or self.max_row
        for column in range(min_col, max_col + 1):
            cells = tuple(self.cell(row, column) for row in range(min_row, max_row + 1))
            yield tuple(cell.value for cell in cells) if values_only else cells

    def __iter__(self): return self.iter_rows()
    @property
    def rows(self): return self.iter_rows()
    @property
    def columns(self): return self.iter_cols()
    @property
    def values(self): return self.iter_rows(values_only=True)

    def append(self, iterable):
        if self._existing:
            raise NotImplementedError("Appending to loaded sheets requires source extent tracking")
        if isinstance(iterable, (str, bytes)):
            raise TypeError("Append requires a row iterable or column dictionary")
        if isinstance(iterable, dict):
            positions = [(_column(column), value) for column, value in iterable.items()]
            values = [None] * max((column for column, _ in positions), default=0)
            for column, value in positions:
                values[column - 1] = value
        else:
            values = []
            for value in iterable:
                if len(values) == 16384:
                    raise ValueError("Appended row exceeds Excel column bounds")
                values.append(value)
        self._model().append([_encode(value) for value in values])

    def _shift(self, idx, amount, rows, insert):
        if self._existing:
            raise NotImplementedError("Existing-file structural editing is not implemented")
        if idx < 1 or amount < 1:
            raise ValueError("Index and amount must be positive")
        cached = list(self._cells.items())
        detached = {(row, col): cell._tagged() for (row, col), cell in cached if not insert and idx <= (row if rows else col) < idx + amount}
        self._model().shift(idx - 1, amount, rows, insert)
        self._cells.clear()
        for (row, col), cell in cached:
            coordinate = row if rows else col
            if (row, col) in detached:
                cell._detached = detached[row, col]
                continue
            if coordinate >= idx + (0 if insert else amount):
                coordinate += amount if insert else -amount
                if rows: cell.row = coordinate
                else: cell.column = coordinate
            self._cells[cell.row, cell.column] = cell

    def insert_rows(self, idx, amount=1): self._shift(idx, amount, True, True)
    def delete_rows(self, idx, amount=1): self._shift(idx, amount, True, False)
    def insert_cols(self, idx, amount=1): self._shift(idx, amount, False, True)
    def delete_cols(self, idx, amount=1): self._shift(idx, amount, False, False)

    def move_range(self, cell_range, rows=0, cols=0, translate=False):
        if self._existing:
            raise NotImplementedError("Existing-file moves are not implemented")
        range_object = cell_range if hasattr(cell_range, "coord") else None
        first_row, first_col, last_row, last_col = _range(range_object.coord if range_object is not None else cell_range)
        cached = list(self._cells.items())
        overwritten = {(row, col): cell._tagged() for (row, col), cell in cached if first_row + rows <= row <= last_row + rows and first_col + cols <= col <= last_col + cols and not (first_row <= row <= last_row and first_col <= col <= last_col)}
        self._model().move_range((first_row - 1, first_col - 1, last_row - 1, last_col - 1), rows, cols, translate)
        if range_object is not None:
            range_object.shift(row_shift=rows, col_shift=cols)
        self._cells.clear()
        for (row, col), cell in cached:
            if first_row <= row <= last_row and first_col <= col <= last_col:
                cell.row += rows
                cell.column += cols
            elif (row, col) in overwritten:
                cell._detached = overwritten[row, col]
                continue
            self._cells[cell.row, cell.column] = cell


class Workbook:
    """Workbook-compatible entry point; models and package editing remain Rust-owned."""
    __slots__ = ("_max_bytes", "_closed", "_reader", "_editor", "data_only", "read_only", "write_only", "_active", "_sheets")
    def __init__(self, write_only=False, iso_dates=False, *, max_memory_bytes=None):
        if write_only or iso_dates:
            raise NotImplementedError("Write-only binding and ISO-date output are not implemented")
        self._max_bytes = resolve_model_budget(max_memory_bytes)
        self._closed = False
        self._reader = self._editor = None
        self.data_only = self.read_only = self.write_only = False
        self._active = 0
        self._sheets = []
        self._sheets.append(Worksheet(self, "Sheet"))

    def _check_open(self):
        if self._closed:
            raise ValueError("Workbook is closed")

    @property
    def worksheets(self): return list(self._sheets)
    @property
    def sheetnames(self): return [sheet.title for sheet in self._sheets]
    @property
    def active(self): return self._sheets[self._active] if self._sheets else None
    @active.setter
    def active(self, value):
        if self._editor is not None:
            raise NotImplementedError("Changing loaded workbook views is not implemented")
        index = self._sheets.index(value) if isinstance(value, Worksheet) else value
        if index != 0:
            raise NotImplementedError("Serializing non-default active sheet is not implemented")
        self._active = index

    def __getitem__(self, key):
        for sheet in self._sheets:
            if sheet.title == key: return sheet
        raise KeyError(f"Worksheet {key} does not exist")
    def __contains__(self, key): return key in self.sheetnames
    def __iter__(self): return iter(self._sheets)

    def _unique_title(self, title, exclude=None):
        names = [sheet.title for sheet in self._sheets if sheet is not exclude]
        if title.lower() in {name.lower() for name in names}:
            suffixes = [name[len(title):] for name in names if name.lower().startswith(title.lower())]
            number = max((int(suffix) for suffix in suffixes if suffix.isdigit()), default=0) + 1
            title = f"{title}{number}"
        return title

    def create_sheet(self, title=None, index=None):
        if self._editor is not None:
            raise NotImplementedError("Adding existing-file sheets is not implemented")
        title = self._unique_title(title or "Sheet")
        sheet = Worksheet(self, title)
        if index is None: self._sheets.append(sheet)
        else: self._sheets.insert(index, sheet)
        return sheet

    def remove(self, worksheet):
        if self._editor is not None:
            raise NotImplementedError("Removing existing-file sheets is not implemented")
        self._sheets.remove(worksheet)
        self._active = 0

    def save(self, filename):
        self._check_open()
        if not isinstance(filename, (str, Path)):
            raise NotImplementedError("File-like binding output is not implemented")
        if self._editor is not None:
            if self.data_only:
                raise NotImplementedError("Saving data-only loaded workbooks is not implemented")
            self._editor.save(Path(filename), False)
        else:
            save_models(Path(filename), [sheet._model() for sheet in self._sheets])

    def close(self):
        if self._reader is not None:
            self._reader.close()
            self._editor.close()
            self._closed = True


def load_workbook(filename, read_only=False, keep_vba=False, data_only=False, keep_links=True, rich_text=False, *, max_memory_bytes=None):
    """Use openpyxl call names; unsupported modes fail rather than change semantics."""
    if read_only or not keep_links or rich_text:
        raise NotImplementedError("Read-only binding, external-link removal and rich-text binding are not implemented")
    if not isinstance(filename, (str, Path)):
        raise NotImplementedError("File-like binding input is not implemented")
    # VBA removal is staged; require explicit preservation for macro inputs.
    if str(filename).lower().endswith((".xlsm", ".xltm")) and not keep_vba:
        raise NotImplementedError("Macro removal is not implemented; use keep_vba=True")
    workbook = Workbook(max_memory_bytes=max_memory_bytes)
    workbook._reader = NativeReader(Path(filename), workbook._max_bytes)
    try:
        workbook._editor = NativeEditor(Path(filename), max_memory_bytes)
        workbook.data_only = data_only
        workbook._sheets = [Worksheet(workbook, name, _existing=True) for name in workbook._reader.names()]
    except BaseException:
        workbook._reader.close()
        raise
    return workbook
