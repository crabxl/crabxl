//! Rust spreadsheet processing with bounded streaming XLSX reads.
//!
//! Streaming reads, sequential creation and source-preserving edits share native
//! cell, style, worksheet, drawing, chart, pivot and workbook metadata models.
//! Binary assets and pivot records can stay on disk while cells stream. Formulas
//! and pivot results are retained or marked for recalculation; CrabXL does not calculate them.
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
pub use crabxl_core::{
    CellError, CellText, ExactInteger, StyleComponent, StyleId, StyleInteger, parse_iso8601,
};
pub use crabxl_xlsx::{AdaptiveRead, ReadData};
pub use crabxl_xlsx::{LoadOptions, LoadedWorkbook};
pub use crabxl_xlsx::{Rows, SheetInfo, SheetKind, WorkbookReader};

pub use crabxl_xlsx::{
    FormulaWritePolicy, NonFiniteWritePolicy, StyleWritePolicy, WorkbookWriter, WriteOptions,
    WriteStats,
};

pub use crabxl_core::{
    BorderLine, BorderSide, CellStyle, DateEpoch, DateKind, ExcelDateTime, Font, Formula,
    HorizontalAlignment, VerticalAlignment,
};

pub use crabxl_core::{ActiveViewSelection, normalize_active_view, resolve_sheet_index};
pub use crabxl_core::{CellRange, EditLimits, SheetVisibility, Worksheet, WorksheetRange};
pub use crabxl_core::{Hyperlink, Hyperlinks};
pub use crabxl_core::{MergedCellRange, MergedRangeIter, MergedRanges};
pub use crabxl_xlsx::resolve_relationship_targets;
pub use crabxl_xlsx::unprotect_rich_text_literal;
pub use crabxl_xlsx::{HyperlinkOutput, visit_owned_hyperlink_ids};
pub use crabxl_xlsx::{read_rich_text, write_rich_text};

pub use crabxl_xlsx::{
    CalculationChainPolicy, EditorOptions, PartInfo, SaveOptions, SaveStats, WorkbookEditor,
};

pub use crabxl_core::MemoryAllowance;
pub use crabxl_xlsx::memory_allowance;

pub use crabxl_core::{
    FormulaScan, FormulaToken, TokenKind, TokenSubtype, classify_formula_operand,
    formula_scientific_prefix, scan_formula_fragment, tokenize_formula,
};
pub use crabxl_core::{formula_position, translate_axis, translate_expression};

pub use crabxl_core::{
    OwnedWorksheets, SheetId, Workbook, WorkbookLimits, WorkbookParts, WorksheetEditor,
};

pub use crabxl_xlsx::{SharedStringOptions, SharedStringStats, SharedStringStorage};

pub use crabxl_core::{
    ArgbLiteral, Color, ColorKind, FontScheme, PhoneticProperties, PhoneticRun, RichText,
    RichTextRun, RunFont, TextVerticalAlignment, Underline,
};

pub use crabxl_core::{
    Alignment, Border, Fill, FillPattern, GradientFill, GradientKind, GradientStop, PatternFill,
    Protection,
};

pub use crabxl_core::{
    CellFormat, NamedStyle, NamedStyleOptions, NumberFormat, StyleCatalog, StyleView,
};

pub use crabxl_core::DateReadPolicy;

pub use crabxl_core::{
    DataTableOptions, FormulaAnnotations, FormulaFlag, FormulaFlags, FormulaMetadata, FormulaRange,
    FormulaReference, FormulaType, SharedFormulaIndex,
};

pub use crabxl_core::FormulaReadPolicy;

pub use crabxl_xlsx::SharedFormulaStats;

pub use crabxl_core::{
    BUILTIN_NUMBER_FORMATS, builtin_number_format, builtin_number_format_id, classify_number_format,
};

pub use crabxl_core::{StyleLimits, StyleRegistry, TemporalStyleIds};

pub use crabxl_core::{
    Theme, ThemeCatalog, ThemeColor, ThemeFontCollection, ThemeScriptFont, ThemeTypeface,
};
pub use crabxl_xlsx::ThemeWritePolicy;

pub use crabxl_core::CellMetadataReadPolicy;

pub use crabxl_core::{
    DifferentialStyle, TableStyle, TableStyleCatalog, TableStyleElement, TableStyleRegion,
};

pub use crabxl_core::{Pane, PanePosition, PaneState, Selection, SheetView, SheetViews, ViewMode};

pub use crabxl_core::{
    PageBreak, PageMargins, PageOrder, PageOrientation, PageSetup, PaperDimension, PrintOptions,
    PrintSettings, PrintSettingsChange, PrintedComments, PrintedErrors,
};

/// Sparse worksheet dimension metadata and shared appearance identities.
pub use crabxl_core::{ColumnDimension, RowDimension, SheetDimensions};

/// Tables, filters, conditional formats, validation, comments, protection,
/// header/footer, defined names and document properties.
pub use crabxl_core::features;
pub use crabxl_core::{SheetFeatures, WorkbookFeatures};
/// Convert features from and to SpreadsheetML XML, for language bindings.
pub use crabxl_xlsx::feature_xml;

pub use crabxl_core::drawing;
pub use crabxl_core::drawing::{
    DrawingAnchor, DrawingEditAs, DrawingExtent, DrawingMarker, Image, ImageFormat,
};

pub use crabxl_core::drawing::{Chart, ChartKind, PartGraph, Shape, XmlContent, XmlElement};

pub use crabxl_core::{BinaryPayload, BinaryReader};

pub use crabxl_xlsx::{PivotRecords, write_pivot_records};

pub use crabxl_core::AnnotatedValue;
pub use crabxl_core::CellStorageType;
