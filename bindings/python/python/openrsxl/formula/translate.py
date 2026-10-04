"""A1 translator compatibility entry point; tokenizer API remains staged."""
import re
from .._native import formula_position, translate_formula as _translate, translate_axis as _axis

class TranslatorError(ValueError):
    """Relative reference translation crosses a formula axis boundary."""

class Translator:
    # Public baseline regex metadata; semantics run through the Rust scanner.
    ROW_RANGE_RE = re.compile(r"(\$?[1-9][0-9]{0,6}):(\$?[1-9][0-9]{0,6})$")
    COL_RANGE_RE = re.compile(r"(\$?[A-Za-z]{1,3}):(\$?[A-Za-z]{1,3})$")
    CELL_REF_RE = re.compile(r"(\$?[A-Za-z]{1,3})(\$?[1-9][0-9]{0,6})$")
    MAX_FORMULA_BYTES = 1024 * 1024

    def __init__(self, formula, origin):
        self.formula = formula
        self.row, self.col = formula_position(origin)

    @property
    def tokenizer(self):
        raise NotImplementedError("The complete formula tokenizer is not implemented")

    def get_tokens(self):
        raise NotImplementedError("The complete formula tokenizer is not implemented")

    @classmethod
    def _invoke(cls, function, *args):
        try:
            return function(*args)
        except ValueError as error:
            if "out of range" in str(error):
                raise TranslatorError(str(error)) from error
            raise

    @staticmethod
    def strip_ws_name(range_str):
        if "!" in range_str:
            sheet, reference = range_str.rsplit("!", 1)
            return sheet + "!", reference
        return "", range_str

    @classmethod
    def translate_row(cls, row_str, rdelta):
        return cls._invoke(_axis, row_str, rdelta, True)

    @classmethod
    def translate_col(cls, col_str, cdelta):
        return cls._invoke(_axis, col_str, cdelta, False)

    @classmethod
    def translate_range(cls, range_str, rdelta, cdelta):
        return cls._invoke(_translate, "=" + range_str, rdelta, cdelta, cls.MAX_FORMULA_BYTES)[1:]

    def translate_formula(self, dest=None, row_delta=0, col_delta=0):
        if dest is not None:
            row, col = formula_position(dest)
            row_delta, col_delta = row - self.row, col - self.col
        if not self.formula.startswith("="):
            return self.formula
        return self._invoke(_translate, self.formula, row_delta, col_delta, self.MAX_FORMULA_BYTES)
