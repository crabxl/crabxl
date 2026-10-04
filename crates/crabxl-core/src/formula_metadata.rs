//! Typed formula records shared by XLSX and language adapters.
use crate::{CellAddress, CellRange, Error, ErrorKind, Result};
use std::borrow::Cow;

/// Formula encoding category; shared indices are local to one worksheet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FormulaType {
    /// Ordinary expression.
    #[default]
    Normal,
    /// Shared template or follower; expanded text remains separately available.
    Shared {
        /// Sparse group identity.
        index: u32,
        /// Whether this source record contains the template.
        master: bool,
    },
    /// Array expression, including baseline array-based dynamic functions.
    Array,
    /// What-if data table; it may have no expression text.
    DataTable,
}
/// Boolean meaning plus optional original XML spelling for public properties
/// which the pinned reference exposes as source strings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormulaFlag {
    value: bool,
    source: Option<Box<str>>,
}
impl FormulaFlag {
    /// Construct a literal boolean without source spelling.
    pub const fn new(value: bool) -> Self {
        Self {
            value,
            source: None,
        }
    }
    /// Parse XML boolean identity while retaining its source spelling.
    pub fn from_xml(value: impl Into<Box<str>>) -> Result<Self> {
        let source = value.into();
        let value = match source.trim_matches([' ', '\t', '\n', '\r']) {
            "1" | "true" => true,
            "0" | "false" => false,
            _ => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "Invalid formula boolean attribute",
                ));
            }
        };
        Ok(Self {
            value,
            source: Some(source),
        })
    }
    /// Typed boolean meaning.
    pub const fn value(&self) -> bool {
        self.value
    }
    /// Original XML property, absent for literal construction.
    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }
    /// Retained source payload bytes.
    pub fn heap_bytes(&self) -> usize {
        self.source.as_ref().map_or(0, |v| v.len())
    }
}
impl From<bool> for FormulaFlag {
    fn from(value: bool) -> Self {
        Self::new(value)
    }
}
/// A finite formula range with optional source spelling (such as absolute axes).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormulaRange {
    /// Validated physical bounds.
    range: CellRange,
    spelling: Option<Box<str>>,
}
impl FormulaRange {
    /// Construct typed bounds without a source literal.
    pub fn new(range: CellRange) -> Result<Self> {
        CellRange::new(range.start, range.end)?;
        Ok(Self {
            range,
            spelling: None,
        })
    }
    /// Parse bounds while retaining the public source property.
    pub fn from_xml(value: impl Into<Box<str>>) -> Result<Self> {
        let spelling = value.into();
        let range = spelling.parse()?;
        Ok(Self {
            range,
            spelling: Some(spelling),
        })
    }
    /// Validated physical bounds.
    pub const fn range(&self) -> CellRange {
        self.range
    }
    /// Replace typed bounds and discard obsolete source spelling.
    pub fn set_range(&mut self, range: CellRange) -> Result<()> {
        CellRange::new(range.start, range.end)?;
        self.range = range;
        self.spelling = None;
        Ok(())
    }
    /// Source spelling or the canonical A1 form.
    pub fn spelling(&self) -> Cow<'_, str> {
        self.spelling.as_ref().map_or_else(
            || Cow::Owned(self.range.to_string()),
            |v| Cow::Borrowed(v.as_ref()),
        )
    }
    /// Retained source bytes.
    pub fn heap_bytes(&self) -> usize {
        self.spelling.as_ref().map_or(0, |v| v.len())
    }
}
/// Optional common calculation hints; no calculation engine is implied.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FormulaFlags {
    /// Always recalculate an array formula.
    pub always_calculate: Option<FormulaFlag>,
    /// Calculate the formula cell.
    pub calculate_cell: Option<FormulaFlag>,
    /// Data-box calculation flag.
    pub data_box: Option<FormulaFlag>,
}
impl FormulaFlags {
    /// Owned optional source spelling bytes.
    pub fn heap_bytes(&self) -> usize {
        [&self.always_calculate, &self.calculate_cell, &self.data_box]
            .into_iter()
            .flatten()
            .map(FormulaFlag::heap_bytes)
            .sum()
    }
}
/// What-if input layout and optional source flags.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DataTableOptions {
    /// Two-input table.
    pub two_dimensions: Option<FormulaFlag>,
    /// Row-oriented table.
    pub row_table: Option<FormulaFlag>,
    /// First input reference with its original absolute-axis spelling.
    pub input1: Option<Box<str>>,
    /// Second input reference.
    pub input2: Option<Box<str>>,
    /// First input was deleted.
    pub deleted1: Option<FormulaFlag>,
    /// Second input was deleted.
    pub deleted2: Option<FormulaFlag>,
}
impl DataTableOptions {
    /// Validate input coordinates without normalizing their source property.
    pub fn validate(&self) -> Result<()> {
        for reference in [&self.input1, &self.input2].into_iter().flatten() {
            reference.parse::<CellAddress>()?;
        }
        Ok(())
    }
    /// Retained optional source bytes.
    pub fn heap_bytes(&self) -> usize {
        self.input1.as_ref().map_or(0, |v| v.len())
            + self.input2.as_ref().map_or(0, |v| v.len())
            + [
                &self.two_dimensions,
                &self.row_table,
                &self.deleted1,
                &self.deleted2,
            ]
            .into_iter()
            .flatten()
            .map(FormulaFlag::heap_bytes)
            .sum::<usize>()
    }
}
/// Structured formula metadata; optional fields retain absence rather than
/// installing flags/ranges or fabricated cached results.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FormulaMetadata {
    /// Encoding category.
    pub kind: FormulaType,
    /// Source array/table/shared range, when present.
    pub reference: Option<FormulaRange>,
    /// Common calculation hints.
    pub flags: FormulaFlags,
    /// Data-table fields, only valid on data-table records.
    pub data_table: Option<Box<DataTableOptions>>,
}
impl FormulaMetadata {
    /// Validate shared typed models before I/O/model mutation.
    pub fn validate(&self) -> Result<()> {
        if let Some(reference) = &self.reference {
            CellRange::new(reference.range.start, reference.range.end)?;
        }
        if let Some(table) = &self.data_table {
            if self.kind != FormulaType::DataTable {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "Data-table fields require a data-table formula",
                ));
            }
            table.validate()?;
        }
        Ok(())
    }
    /// Decoded source string bytes, excluding fixed typed wrappers.
    pub fn payload_bytes(&self) -> usize {
        self.reference.as_ref().map_or(0, FormulaRange::heap_bytes)
            + self.flags.heap_bytes()
            + self.data_table.as_ref().map_or(0, |v| v.heap_bytes())
    }
    /// Boxed wrapper plus retained source/reference/options bytes.
    pub fn memory_bytes(&self) -> usize {
        size_of::<Self>()
            + self.reference.as_ref().map_or(0, FormulaRange::heap_bytes)
            + self.flags.heap_bytes()
            + self
                .data_table
                .as_ref()
                .map_or(0, |v| size_of::<DataTableOptions>() + v.heap_bytes())
    }
}
