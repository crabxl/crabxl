use crate::{
    CellAddress, CellError, CellText, ColumnIndex, ExactInteger, ExcelDateTime, Formula, RowIndex,
    StyleId,
};
use std::ops::RangeInclusive;

/// Values supported by the current scalar reader.
///
/// Further value variants will be added with the M2 codecs. Unsupported input
/// currently produces an error rather than a silently incorrect value.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum CellValue {
    /// A physically present cell without a literal value.
    Empty,
    /// A finite IEEE-754 numeric value.
    Number(f64),
    /// A literal spreadsheet boolean, distinct from numeric zero or one.
    Boolean(bool),
    /// An exact signed integer literal within i64.
    Integer(i64),
    /// An exact decimal integer outside i64; no floating-point conversion.
    BigInteger(Box<ExactInteger>),
    /// Owned text with whitespace preserved.
    Text(Box<CellText>),
    /// A literal spreadsheet error token.
    Error(Box<CellError>),
    /// Excel date/time serial with epoch and interpretation.
    DateTime(Box<ExcelDateTime>),
    /// Normal formula with optional typed cached result.
    Formula(Box<Formula>),
}

impl CellValue {
    /// Bytes retained outside the fixed-size cell value, excluding allocator overhead.
    pub fn heap_bytes(&self) -> usize {
        match self {
            Self::DateTime(_) => size_of::<ExcelDateTime>(),
            Self::Formula(value) => value.memory_bytes(),
            Self::BigInteger(value) => value.memory_bytes(),
            Self::Text(value) => value.memory_bytes(),
            Self::Error(value) => value.memory_bytes(),
            _ => 0,
        }
    }
    /// Construct an owned text value.
    pub fn text(value: impl Into<Box<str>>) -> Self {
        Self::Text(Box::new(CellText::new(value)))
    }
    /// Construct an owned spreadsheet error value.
    pub fn error(code: impl Into<Box<str>>) -> Self {
        Self::Error(Box::new(CellError::new(code)))
    }
}

/// A present cell with its actual coordinate.
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    /// Actual worksheet position.
    pub address: CellAddress,
    /// Value or formula with its optional cache.
    pub value: CellValue,
    /// Workbook-local cell format (zero selects the default record).
    pub style: StyleId,
}

/// An owned sparse row. Missing columns are not expanded into empty cells.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// Actual zero-based row index.
    pub index: RowIndex,
    /// Present cells surviving projection, ordered by column.
    pub cells: Vec<Cell>,
}
impl Row {
    /// Construct an empty reusable row buffer.
    pub fn new(index: RowIndex) -> Self {
        Self {
            index,
            cells: Vec::new(),
        }
    }
    /// Estimate resident row allocation, including the vector's capacity.
    pub fn memory_bytes(&self) -> usize {
        size_of::<Self>()
            + self.cells.capacity() * size_of::<Cell>()
            + self
                .cells
                .iter()
                .map(|cell| cell.value.heap_bytes())
                .sum::<usize>()
    }
}

/// An owned, bounded collection of rows.
#[derive(Debug)]
pub struct RowBatch {
    /// Sparse rows with their actual coordinates.
    pub rows: Vec<Row>,
}
impl RowBatch {
    /// Estimate batch allocation, including outer and inner vector capacities.
    pub fn memory_bytes(&self) -> usize {
        size_of::<Self>()
            + self.rows.capacity() * size_of::<Row>()
            + self
                .rows
                .iter()
                .map(|r| r.memory_bytes() - size_of::<Row>())
                .sum::<usize>()
    }
}

/// An explicitly materialized scalar worksheet snapshot.
///
/// This owns all sparse rows; memory grows with loaded cells. It is not the
/// future editable workbook model and cannot save or preserve workbook parts.
#[derive(Debug)]
pub struct SheetData {
    /// All physically present sparse rows, with actual coordinates.
    pub rows: Vec<Row>,
}
impl SheetData {
    /// Estimate resident row/vector allocations, excluding allocator overhead.
    pub fn memory_bytes(&self) -> usize {
        size_of::<Self>()
            + self.rows.capacity() * size_of::<Row>()
            + self
                .rows
                .iter()
                .map(|row| row.memory_bytes() - size_of::<Row>())
                .sum::<usize>()
    }
}

/// Projection performed before expensive cell-value decoding.
#[derive(Clone, Debug, Default)]
pub struct ReadOptions {
    /// Optional inclusive zero-based row bounds. XML outside the bounds is scanned.
    pub rows: Option<RangeInclusive<RowIndex>>,
    /// Optional inclusive zero-based column bounds.
    pub columns: Option<RangeInclusive<ColumnIndex>>,
    /// Return cached formula results instead of formulas. Missing caches are Empty.
    pub data_only: bool,
}
impl ReadOptions {
    /// Whether the selected row range includes this index.
    pub fn includes_row(&self, row: RowIndex) -> bool {
        self.rows.as_ref().is_none_or(|r| r.contains(&row))
    }
    /// Whether both selected ranges include this cell.
    pub fn includes(&self, address: CellAddress) -> bool {
        self.includes_row(address.row)
            && self
                .columns
                .as_ref()
                .is_none_or(|r| r.contains(&address.column))
    }
}
