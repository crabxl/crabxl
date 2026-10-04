//! Runtime-independent spreadsheet values, coordinates, errors, and limits.

mod address;
mod date;
mod error;
mod formula;
mod formula_metadata;
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
pub use formula_metadata::{
    DataTableOptions, FormulaFlag, FormulaFlags, FormulaMetadata, FormulaRange, FormulaType,
};
pub use iso_date::parse_iso8601;
pub use limits::ResourceLimits;
pub use memory::{
    AccessPattern, AutoMemory, DecisionReason, MemoryAllowance, MemoryPolicy, MemorySource,
    ReadDecision, ReadMode,
};
pub use row::{
    Cell, CellMetadataReadPolicy, CellValue, DateReadPolicy, FormulaReadPolicy, ReadOptions, Row,
    RowBatch, SheetData,
};
pub use scalar::{CellError, CellText, ExactInteger};
pub use style::{CellStyle, Font, StyleId};

mod worksheet;
pub use worksheet::{CellRange, EditLimits, Worksheet};

mod translate;
pub use translate::{formula_position, translate_axis, translate_expression};

mod workbook;
pub use workbook::{
    OwnedWorksheets, SheetId, Workbook, WorkbookLimits, WorkbookParts, WorksheetEditor,
};

mod rich_text;
pub use rich_text::{
    ArgbLiteral, Color, ColorKind, FontScheme, PhoneticProperties, PhoneticRun, RichText,
    RichTextRun, RunFont, TextVerticalAlignment, Underline,
};

mod style_components;
pub use style_components::{
    Alignment, Border, BorderLine, BorderSide, Fill, FillPattern, GradientFill, GradientKind,
    GradientStop, HorizontalAlignment, PatternFill, Protection, VerticalAlignment,
};

mod style_catalog;
pub use style_catalog::{CellFormat, NamedStyle, NumberFormat, StyleCatalog, StyleView};

mod number_formats;
pub use number_formats::{
    BUILTIN_NUMBER_FORMATS, builtin_number_format, builtin_number_format_id, classify_number_format,
};

mod style_hash;
mod style_validation;

mod style_registry;
pub use style_registry::{StyleLimits, StyleRegistry};

mod theme;
pub use theme::Theme;
