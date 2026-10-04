//! Rust spreadsheet processing with bounded streaming XLSX reads.
//!
//! This first milestone reads sparse, unstyled numeric/empty cells. Other
//! spreadsheet features remain planned; unsupported selected cells fail loudly.

pub use openrsxl_core::{
    Cell, CellAddress, CellValue, ColumnIndex, Error, ErrorKind, MAX_COLUMNS, MAX_ROWS,
    ReadOptions, ResourceLimits, Result, Row, RowBatch, RowIndex, SheetData,
};
pub use openrsxl_xlsx::{Rows, SheetInfo, SheetKind, WorkbookReader};
