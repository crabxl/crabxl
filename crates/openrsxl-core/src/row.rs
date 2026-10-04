use crate::{CellAddress, ColumnIndex, RowIndex};
use std::ops::RangeInclusive;

/// Values supported by the initial numeric reader.
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
}

/// A present cell with its actual coordinate.
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    /// Actual worksheet position.
    pub address: CellAddress,
    /// Literal value.
    pub value: CellValue,
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
        size_of::<Self>() + self.cells.capacity() * size_of::<Cell>()
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
                .map(|r| r.cells.capacity() * size_of::<Cell>())
                .sum::<usize>()
    }
}

/// An explicitly materialized numeric worksheet snapshot.
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
                .map(|row| row.cells.capacity() * size_of::<Cell>())
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
