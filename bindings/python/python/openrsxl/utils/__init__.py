"""Coordinate helpers backed by the adapter's shared validation."""
from .. import _address, _column, _letters, _range

def get_column_letter(idx): return _letters(idx)
def column_index_from_string(col): return _column(col)
def coordinate_to_tuple(coordinate): return _address(coordinate)
def range_boundaries(range_string):
    row, col, end_row, end_col = _range(range_string)
    return col, row, end_col, end_row
