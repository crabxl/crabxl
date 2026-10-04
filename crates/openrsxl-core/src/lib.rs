//! Runtime-independent spreadsheet values, coordinates, errors, and limits.

mod address;
mod error;
mod limits;
mod row;

pub use address::{CellAddress, ColumnIndex, MAX_COLUMNS, MAX_ROWS, RowIndex};
pub use error::{Error, ErrorKind, Result};
pub use limits::ResourceLimits;
pub use row::{Cell, CellValue, ReadOptions, Row, RowBatch, SheetData};
