//! Rust spreadsheet processing with bounded streaming XLSX reads.
//!
//! This checkpoint reads sparse, unstyled scalars, plain inline text and normal formulas.
//! The sequential writer adds dates and basic styles. Other
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

pub use openrsxl_core::{
    AccessPattern, AutoMemory, DecisionReason, MemoryPolicy, MemorySource, ReadDecision, ReadMode,
};
pub use openrsxl_core::{
    Cell, CellAddress, CellValue, ColumnIndex, Error, ErrorKind, MAX_COLUMNS, MAX_ROWS,
    ReadOptions, ResourceLimits, Result, Row, RowBatch, RowIndex, SheetData,
};
pub use openrsxl_core::{CellError, CellText, ExactInteger, StyleId};
pub use openrsxl_xlsx::{AdaptiveRead, ReadData};
pub use openrsxl_xlsx::{Rows, SheetInfo, SheetKind, WorkbookReader};

pub use openrsxl_xlsx::{WorkbookWriter, WriteOptions, WriteStats};

pub use openrsxl_core::{
    BorderLine, BorderSide, CellStyle, DateEpoch, DateKind, ExcelDateTime, Font, Formula,
    HorizontalAlignment, VerticalAlignment,
};
