use crate::{
    CellAddress, CellError, CellText, ColumnIndex, ExactInteger, ExcelDateTime, Formula, RowIndex,
    StyleId,
};
use std::ops::RangeInclusive;

/// Values supported by the current scalar reader.
///
/// Further value variants will be added with the M2 codecs. Unsupported input
/// currently produces an error rather than a silently incorrect value.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum CellValue {
    /// A physically present cell without a literal value.
    Empty,
    /// An IEEE-754 numeric value, including overflow infinities from numeric lexemes.
    Number(f64),
    /// A literal spreadsheet boolean, distinct from numeric zero or one.
    Boolean(bool),
    /// An exact signed integer literal within i64.
    Integer(i64),
    /// An exact decimal integer outside i64; no floating-point conversion.
    BigInteger(Box<ExactInteger>),
    /// Owned text with whitespace preserved.
    Text(Box<CellText>),
    /// A literal spreadsheet error token.
    Error(Box<CellError>),
    /// Rich display runs and optional separate pronunciation annotations.
    RichText(Box<crate::RichText>),
    /// Excel date/time serial with epoch and interpretation.
    DateTime(Box<ExcelDateTime>),
    /// Normal formula with optional typed cached result.
    Formula(Box<Formula>),
}

impl CellValue {
    /// Borrow a temporal literal or a formula's temporal cached result.
    pub fn temporal_value(&self) -> Option<&ExcelDateTime> {
        match self {
            Self::DateTime(value) => Some(value),
            Self::Formula(value) => value.cached().and_then(Self::temporal_value),
            _ => None,
        }
    }

    /// Bytes retained outside the fixed-size cell value, excluding allocator overhead.
    pub fn heap_bytes(&self) -> usize {
        match self {
            Self::DateTime(_) => size_of::<ExcelDateTime>(),
            Self::Formula(value) => value.memory_bytes(),
            Self::BigInteger(value) => value.memory_bytes(),
            Self::Text(value) => value.memory_bytes(),
            Self::Error(value) => value.memory_bytes(),
            Self::RichText(value) => value.memory_bytes(),
            _ => 0,
        }
    }
    /// Construct an owned text value.
    pub fn text(value: impl Into<Box<str>>) -> Self {
        Self::Text(Box::new(CellText::new(value)))
    }
    /// Retain immutable shared text without copying its UTF-8 payload.
    pub fn shared_text(value: std::sync::Arc<str>) -> Self {
        Self::Text(Box::new(CellText::from_shared(value)))
    }
    /// Construct an owned spreadsheet error value.
    pub fn error(code: impl Into<Box<str>>) -> Self {
        Self::Error(Box::new(CellError::new(code)))
    }
}

/// A present cell with its actual coordinate.
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    /// Actual worksheet position.
    pub address: CellAddress,
    /// Value or formula with its optional cache.
    pub value: CellValue,
    /// Workbook-local cell format (zero selects the default record).
    pub style: StyleId,
}

impl Cell {
    /// Assign an explicit format without copying the value or inferring dates.
    /// Returns whether the style or temporal serialization preference changed.
    /// The owner/writer must validate this workbook-local identity.
    pub fn set_style(&mut self, style: StyleId) -> bool {
        let temporal_changed = match &mut self.value {
            CellValue::DateTime(date) => date.prefer_serial_encoding(),
            _ => false,
        };
        let changed = self.style != style || temporal_changed;
        self.style = style;
        changed
    }
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
        size_of::<Self>()
            + self.cells.capacity() * size_of::<Cell>()
            + self
                .cells
                .iter()
                .map(|cell| cell.value.heap_bytes())
                .sum::<usize>()
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
                .map(|r| r.memory_bytes() - size_of::<Row>())
                .sum::<usize>()
    }
}

/// An explicitly materialized scalar worksheet snapshot.
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
                .map(|row| row.memory_bytes() - size_of::<Row>())
                .sum::<usize>()
    }
}

/// How numeric date-formatted values outside the reference calendar range are read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DateReadPolicy {
    /// Return a spreadsheet #VALUE! error for unrepresentable baseline dates/durations.
    #[default]
    Compatible,
    /// Keep finite source serials even when a calendar/runtime conversion is unavailable.
    RetainSerial,
}
/// Validation of worksheet-local shared formula groups.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FormulaReadPolicy {
    /// Match reference expansion, including missing templates and source range quirks.
    #[default]
    Compatible,
    /// Reject unresolved/duplicate masters and followers outside declared ranges.
    ValidateGroups,
}
/// Treatment of opaque cell/value metadata references during value projection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CellMetadataReadPolicy {
    /// Match reference values/formulas without interpreting cm/vm graphs.
    #[default]
    Compatible,
    /// Retain formula cm/vm references in FormulaMetadata, without interpreting
    /// dependent graphs. Annotated scalar cells and data-only reads are rejected.
    RetainFormulaReferences,
    /// Reject selected annotated cells instead of projecting their visible value.
    Reject,
}
/// Projection performed before expensive cell-value decoding.
#[derive(Clone, Debug, Default)]
pub struct ReadOptions {
    /// Optional inclusive zero-based row bounds. XML outside the bounds is scanned.
    pub rows: Option<RangeInclusive<RowIndex>>,
    /// Stop at the last selected row instead of validating the unread XML tail/ZIP CRC.
    /// Applies only with explicit row bounds; default reads validate the complete part.
    pub stop_after_last_row: bool,
    /// Optional inclusive zero-based column bounds.
    pub columns: Option<RangeInclusive<ColumnIndex>>,
    /// Return cached formula results instead of formulas. Missing caches are Empty.
    pub data_only: bool,
    /// Preserve rich-text runs and pronunciation metadata rather than projecting
    /// display text. Plain projection is the reference-compatible default.
    pub rich_text: bool,
    /// Baseline date errors by default; exact raw-serial retention is an extension.
    pub date_policy: DateReadPolicy,
    /// Expose shared source metadata instead of only expanded normal expressions.
    /// Array/table metadata is always retained when returning formulas.
    pub formula_metadata: bool,
    /// Reference-compatible shared groups or explicit structural validation.
    pub formula_policy: FormulaReadPolicy,
    /// Reference-compatible value projection or explicit rejection of opaque cm/vm references.
    pub cell_metadata_policy: CellMetadataReadPolicy,
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
