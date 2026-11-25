//! Cell, range, row and column references.
//!
//! Text references use A1 notation (`"B2"`, `"A1:D10"`, `"5:7"`, `"B:D"`).
//! Numeric positions are zero-based: `CellRef::new(0, 0)` is `A1`.

use crate::{Error, ErrorKind, Result};
use crabxl_core::{CellAddress, CellRange, MAX_COLUMNS, MAX_ROWS};
use std::fmt;
use std::str::FromStr;

/// The position of one cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CellRef(pub(crate) CellAddress);

impl CellRef {
    /// A cell at a zero-based row and column. Fails outside the worksheet
    /// (1,048,576 rows by 16,384 columns).
    pub fn new(row: u32, column: u32) -> Result<Self> {
        CellAddress::new(row, column).map(Self)
    }
    /// The zero-based row.
    pub fn row(self) -> u32 {
        self.0.row.get()
    }
    /// The zero-based column.
    pub fn column(self) -> u32 {
        self.0.column.get()
    }
    /// The column letters, such as `"AB"`.
    pub fn column_name(self) -> String {
        let text = self.to_string();
        text.trim_end_matches(|c: char| c.is_ascii_digit())
            .to_owned()
    }
}

impl FromStr for CellRef {
    type Err = Error;
    fn from_str(text: &str) -> Result<Self> {
        text.replace('$', "")
            .parse::<CellAddress>()
            .map(Self)
            .map_err(|_| invalid_reference(text))
    }
}

impl fmt::Display for CellRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A rectangle of cells, from its top-left to its bottom-right cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RangeRef(pub(crate) CellRange);

impl RangeRef {
    /// The rectangle between two cells, in any order.
    pub fn new(first: CellRef, second: CellRef) -> Result<Self> {
        let start = CellAddress::new(
            first.row().min(second.row()),
            first.column().min(second.column()),
        )?;
        let end = CellAddress::new(
            first.row().max(second.row()),
            first.column().max(second.column()),
        )?;
        CellRange::new(start, end).map(Self)
    }
    /// The top-left cell.
    pub fn start(self) -> CellRef {
        CellRef(self.0.start)
    }
    /// The bottom-right cell.
    pub fn end(self) -> CellRef {
        CellRef(self.0.end)
    }
    /// The number of rows.
    pub fn height(self) -> u32 {
        self.0.end.row.get() - self.0.start.row.get() + 1
    }
    /// The number of columns.
    pub fn width(self) -> u32 {
        self.0.end.column.get() - self.0.start.column.get() + 1
    }
    /// Whether the range is a single cell.
    pub fn is_cell(self) -> bool {
        self.0.start == self.0.end
    }
    /// Whether the range contains a cell.
    pub fn contains(self, cell: CellRef) -> bool {
        self.0.contains(cell.0)
    }
    /// The cells row by row.
    pub fn cells(self) -> impl Iterator<Item = CellRef> {
        let (start, end) = (self.0.start, self.0.end);
        (start.row.get()..=end.row.get()).flat_map(move |row| {
            (start.column.get()..=end.column.get())
                .filter_map(move |column| CellAddress::new(row, column).ok().map(CellRef))
        })
    }
}

impl FromStr for RangeRef {
    type Err = Error;
    fn from_str(text: &str) -> Result<Self> {
        let clean = text.replace('$', "");
        match clean.split_once(':') {
            Some((first, second)) => Self::new(first.parse()?, second.parse()?),
            None => {
                let cell: CellRef = clean.parse().map_err(|_| invalid_reference(text))?;
                Ok(cell.into())
            }
        }
        .map_err(|_| invalid_reference(text))
    }
}

impl fmt::Display for RangeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_cell() {
            self.0.start.fmt(f)
        } else {
            self.0.fmt(f)
        }
    }
}

impl From<CellRef> for RangeRef {
    fn from(cell: CellRef) -> Self {
        Self(CellRange {
            start: cell.0,
            end: cell.0,
        })
    }
}

/// A span of whole rows, such as `"5:7"`, or of whole columns, such as `"B:D"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Span {
    /// Zero-based index of the first row or column.
    pub start: u32,
    /// Number of rows or columns. At least one.
    pub count: u32,
}

impl Span {
    /// `count` rows or columns from the zero-based index `start`.
    pub fn new(start: u32, count: u32) -> Self {
        Self { start, count }
    }
    /// Parse a row span: `"5"` or `"5:7"` (row numbers as shown in Excel).
    pub fn rows(text: &str) -> Result<Self> {
        let parse = |part: &str| -> Result<u32> {
            let row: u32 = part
                .trim_start_matches('$')
                .parse()
                .map_err(|_| invalid_span(text))?;
            if row == 0 || row > MAX_ROWS {
                return Err(invalid_span(text));
            }
            Ok(row - 1)
        };
        Self::from_parts(text, parse)
    }
    /// Parse a column span: `"B"` or `"B:D"`.
    pub fn columns(text: &str) -> Result<Self> {
        let parse = |part: &str| -> Result<u32> {
            let letters = part.trim_start_matches('$');
            if letters.is_empty()
                || letters.len() > 3
                || !letters.bytes().all(|b| b.is_ascii_alphabetic())
            {
                return Err(invalid_span(text));
            }
            let index = letters.bytes().fold(0u32, |sum, b| {
                sum * 26 + u32::from(b.to_ascii_uppercase() - b'A' + 1)
            });
            if index > MAX_COLUMNS {
                return Err(invalid_span(text));
            }
            Ok(index - 1)
        };
        Self::from_parts(text, parse)
    }
    fn from_parts(text: &str, parse: impl Fn(&str) -> Result<u32>) -> Result<Self> {
        let (first, last) = match text.split_once(':') {
            Some((first, last)) => (parse(first)?, parse(last)?),
            None => {
                let index = parse(text)?;
                (index, index)
            }
        };
        Ok(Self {
            start: first.min(last),
            count: first.abs_diff(last) + 1,
        })
    }
}

/// Converts to a cell range. Implemented for `&str` and `String` (A1
/// notation), [`CellRef`], [`RangeRef`] and `(CellRef, CellRef)`.
pub trait IntoRange: sealed::Sealed {
    /// Resolve the range; text that is not a valid reference fails here.
    fn into_range(self) -> Result<RangeRef>;
}

/// Converts to one cell. Implemented for `&str`, `String` and [`CellRef`].
pub trait IntoCellRef: sealed::Sealed {
    /// Resolve the cell; text that is not a valid reference fails here.
    fn into_cell_ref(self) -> Result<CellRef>;
}

/// Converts to a span of rows or columns. Implemented for [`Span`] and for
/// `&str` in A1 notation (`"5:7"` or `"B:D"`).
pub trait IntoSpan: sealed::Sealed {
    /// Resolve a row span.
    fn into_row_span(self) -> Result<Span>;
    /// Resolve a column span.
    fn into_column_span(self) -> Result<Span>;
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for &str {}
    impl Sealed for String {}
    impl Sealed for &String {}
    impl Sealed for super::CellRef {}
    impl Sealed for super::RangeRef {}
    impl Sealed for (super::CellRef, super::CellRef) {}
    impl Sealed for super::Span {}
}

impl IntoRange for &str {
    fn into_range(self) -> Result<RangeRef> {
        self.parse()
    }
}
impl IntoRange for String {
    fn into_range(self) -> Result<RangeRef> {
        self.parse()
    }
}
impl IntoRange for &String {
    fn into_range(self) -> Result<RangeRef> {
        self.parse()
    }
}
impl IntoRange for CellRef {
    fn into_range(self) -> Result<RangeRef> {
        Ok(self.into())
    }
}
impl IntoRange for RangeRef {
    fn into_range(self) -> Result<RangeRef> {
        Ok(self)
    }
}
impl IntoRange for (CellRef, CellRef) {
    fn into_range(self) -> Result<RangeRef> {
        RangeRef::new(self.0, self.1)
    }
}
impl IntoCellRef for &str {
    fn into_cell_ref(self) -> Result<CellRef> {
        self.parse()
    }
}
impl IntoCellRef for String {
    fn into_cell_ref(self) -> Result<CellRef> {
        self.parse()
    }
}
impl IntoCellRef for &String {
    fn into_cell_ref(self) -> Result<CellRef> {
        self.parse()
    }
}
impl IntoCellRef for CellRef {
    fn into_cell_ref(self) -> Result<CellRef> {
        Ok(self)
    }
}
impl IntoSpan for &str {
    fn into_row_span(self) -> Result<Span> {
        Span::rows(self)
    }
    fn into_column_span(self) -> Result<Span> {
        Span::columns(self)
    }
}
impl IntoSpan for Span {
    fn into_row_span(self) -> Result<Span> {
        Ok(self)
    }
    fn into_column_span(self) -> Result<Span> {
        Ok(self)
    }
}

fn invalid_reference(text: &str) -> Error {
    Error::new(
        ErrorKind::InvalidData,
        format!("'{text}' is not a cell or range reference"),
    )
}
fn invalid_span(text: &str) -> Error {
    Error::new(
        ErrorKind::InvalidData,
        format!("'{text}' is not a row or column span"),
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn references_parse_and_display() {
        let cell: CellRef = "$B$3".parse().unwrap();
        assert_eq!(
            (cell.row(), cell.column(), cell.to_string()),
            (2, 1, "B3".into())
        );
        assert_eq!(CellRef::new(0, 27).unwrap().column_name(), "AB");
        let range: RangeRef = "C4:A1".parse().unwrap();
        assert_eq!(
            (range.to_string(), range.width(), range.height()),
            ("A1:C4".into(), 3, 4)
        );
        assert_eq!("B2".parse::<RangeRef>().unwrap().to_string(), "B2");
        assert!("ZZZZ1".parse::<CellRef>().is_err());
        assert_eq!(Span::rows("5:7").unwrap(), Span::new(4, 3));
        assert_eq!(Span::columns("D:B").unwrap(), Span::new(1, 3));
        assert!(Span::rows("0").is_err());
        assert_eq!(range.cells().count(), 12);
    }
}
