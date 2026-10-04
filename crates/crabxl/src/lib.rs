//! Rust spreadsheet processing with bounded streaming XLSX reads.
//!
//! Reads include sparse scalars, bounded plain/rich strings, numeric and ISO dates,
//! shared style catalogs and normal formulas. Sequential creation shares the core models. Other
//! spreadsheet features remain planned; unsupported selected cells fail loudly.
//!
//! ```no_run
//! use crabxl::{AccessPattern, MemoryPolicy, ReadData, WorkbookReader};
//! # fn main() -> crabxl::Result<()> {
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

pub use crabxl_core::{
    AccessPattern, AutoMemory, DecisionReason, MemoryPolicy, MemorySource, ReadDecision, ReadMode,
};
pub use crabxl_core::{
    Cell, CellAddress, CellValue, ColumnIndex, Error, ErrorKind, MAX_COLUMNS, MAX_ROWS,
    ReadOptions, ResourceLimits, Result, Row, RowBatch, RowIndex, SheetData,
};
pub use crabxl_core::{CellError, CellText, ExactInteger, StyleId, parse_iso8601};
pub use crabxl_xlsx::{AdaptiveRead, ReadData};
pub use crabxl_xlsx::{Rows, SheetInfo, SheetKind, WorkbookReader};

pub use crabxl_xlsx::{NonFiniteWritePolicy, WorkbookWriter, WriteOptions, WriteStats};

pub use crabxl_core::{
    BorderLine, BorderSide, CellStyle, DateEpoch, DateKind, ExcelDateTime, Font, Formula,
    HorizontalAlignment, VerticalAlignment,
};

pub use crabxl_core::{CellRange, EditLimits, Worksheet};

pub use crabxl_xlsx::{
    CalculationChainPolicy, EditorOptions, PartInfo, SaveOptions, SaveStats, WorkbookEditor,
};

pub use crabxl_core::MemoryAllowance;
pub use crabxl_xlsx::memory_allowance;

pub use crabxl_core::{formula_position, translate_axis, translate_expression};

pub use crabxl_core::{SheetId, Workbook, WorkbookLimits, WorksheetEditor};

pub use crabxl_xlsx::{SharedStringOptions, SharedStringStats, SharedStringStorage};

pub use crabxl_core::{
    Color, ColorKind, FontScheme, PhoneticProperties, PhoneticRun, RichText, RichTextRun, RunFont,
    TextVerticalAlignment, Underline,
};

pub use crabxl_core::{
    Alignment, Border, Fill, FillPattern, GradientFill, GradientKind, GradientStop, PatternFill,
    Protection,
};

pub use crabxl_core::{CellFormat, NamedStyle, NumberFormat, StyleCatalog, StyleView};

pub use crabxl_core::DateReadPolicy;

pub use crabxl_core::{
    DataTableOptions, FormulaFlag, FormulaFlags, FormulaMetadata, FormulaRange, FormulaType,
};

pub use crabxl_core::FormulaReadPolicy;

pub use crabxl_xlsx::SharedFormulaStats;

pub use crabxl_core::{BUILTIN_NUMBER_FORMATS, builtin_number_format, builtin_number_format_id};
