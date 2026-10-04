"""Finite range compatibility view; the core owns structural cell operations."""
from .. import _letters, _range

class CellRange:
    def __init__(self, range_string):
        self.min_row, self.min_col, self.max_row, self.max_col = _range(range_string)

    @property
    def coord(self):
        first = f"{_letters(self.min_col)}{self.min_row}"
        last = f"{_letters(self.max_col)}{self.max_row}"
        return first if first == last else f"{first}:{last}"

    @property
    def bounds(self):
        return self.min_col, self.min_row, self.max_col, self.max_row

    def shift(self, col_shift=0, row_shift=0):
        bounds = self.min_row + row_shift, self.min_col + col_shift, self.max_row + row_shift, self.max_col + col_shift
        if not 1 <= bounds[0] <= bounds[2] <= 1048576 or not 1 <= bounds[1] <= bounds[3] <= 16384:
            raise ValueError("Shifted range is outside Excel bounds")
        self.min_row, self.min_col, self.max_row, self.max_col = bounds

    def __str__(self): return self.coord
