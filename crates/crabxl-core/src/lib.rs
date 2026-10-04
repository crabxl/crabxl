//! Runtime-independent spreadsheet values, coordinates, errors, and limits.

mod address;
mod date;
mod error;
mod formula;
mod iso_date;
mod limits;
mod memory;
mod row;
mod scalar;
mod style;

pub use address::{CellAddress, ColumnIndex, MAX_COLUMNS, MAX_ROWS, RowIndex};
pub use date::{DateEpoch, DateKind, ExcelDateTime};
pub use error::{Error, ErrorKind, Result};
pub use formula::Formula;
pub use iso_date::parse_iso8601;
pub use limits::ResourceLimits;
pub use memory::{
    AccessPattern, AutoMemory, DecisionReason, MemoryAllowance, MemoryPolicy, MemorySource,
    ReadDecision, ReadMode,
};
pub use row::{Cell, CellValue, DateReadPolicy, ReadOptions, Row, RowBatch, SheetData};
pub use scalar::{CellError, CellText, ExactInteger};
pub use style::{CellStyle, Font, StyleId};

mod worksheet;
pub use worksheet::{CellRange, EditLimits, Worksheet};

mod translate;
pub use translate::{formula_position, translate_axis, translate_expression};

mod workbook;
pub use workbook::{SheetId, Workbook, WorkbookLimits, WorksheetEditor};

mod rich_text;
pub use rich_text::{
    Color, ColorKind, FontScheme, PhoneticProperties, PhoneticRun, RichText, RichTextRun, RunFont,
    TextVerticalAlignment, Underline,
};

mod style_components;
pub use style_components::{
    Alignment, Border, BorderLine, BorderSide, Fill, FillPattern, GradientFill, GradientKind,
    GradientStop, HorizontalAlignment, PatternFill, Protection, VerticalAlignment,
};

mod style_catalog;
pub use style_catalog::{CellFormat, NamedStyle, NumberFormat, StyleCatalog};
