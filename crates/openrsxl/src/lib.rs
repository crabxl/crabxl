//! Rust spreadsheet processing with bounded streaming XLSX reads.
//!
//! This first milestone reads sparse, unstyled numeric/empty cells. Other
//! spreadsheet features remain planned; unsupported selected cells fail loudly.
//!
//! ```no_run
//! use openrsxl::{AccessPattern, MemoryPolicy, ReadData, WorkbookReader};
//! # fn main() -> openrsxl::Result<()> {
//! let mut workbook = WorkbookReader::open("numbers.xlsx")?;
//! let output = workbook.read_with_policy(
//!     "Sheet", AccessPattern::RepeatedAccess, MemoryPolicy::default(),
//! )?;
//! println!("{:?}", output.decision);
//! match output.data {
//!     ReadData::Materialized(sheet) => println!("{} rows", sheet.rows.len()),
//!     ReadData::Streaming(mut rows) => while let Some(row) = rows.next_row()? {
//!         println!("{:?}", row.index);
//!     },
//! }
//! # Ok(())
//! # }
//! ```

pub use openrsxl_core::StyleId;
pub use openrsxl_core::{
    AccessPattern, AutoMemory, DecisionReason, MemoryPolicy, MemorySource, ReadDecision, ReadMode,
};
pub use openrsxl_core::{
    Cell, CellAddress, CellValue, ColumnIndex, Error, ErrorKind, MAX_COLUMNS, MAX_ROWS,
    ReadOptions, ResourceLimits, Result, Row, RowBatch, RowIndex, SheetData,
};
pub use openrsxl_xlsx::{AdaptiveRead, ReadData};
pub use openrsxl_xlsx::{Rows, SheetInfo, SheetKind, WorkbookReader};
