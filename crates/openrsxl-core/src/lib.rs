//! Runtime-independent spreadsheet values, coordinates, errors, and limits.

mod address;
mod date;
mod error;
mod formula;
mod limits;
mod memory;
mod row;
mod scalar;
mod style;

pub use address::{CellAddress, ColumnIndex, MAX_COLUMNS, MAX_ROWS, RowIndex};
pub use date::{DateEpoch, DateKind, ExcelDateTime};
pub use error::{Error, ErrorKind, Result};
pub use formula::Formula;
pub use limits::ResourceLimits;
pub use memory::{
    AccessPattern, AutoMemory, DecisionReason, MemoryPolicy, MemorySource, ReadDecision, ReadMode,
};
pub use row::{Cell, CellValue, ReadOptions, Row, RowBatch, SheetData};
pub use scalar::{CellError, CellText, ExactInteger};
pub use style::{
    BorderLine, BorderSide, CellStyle, Font, HorizontalAlignment, StyleId, VerticalAlignment,
};
