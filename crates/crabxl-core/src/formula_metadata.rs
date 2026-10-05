//! Typed formula records shared by XLSX and language adapters.
use crate::{CellAddress, CellRange, Error, ErrorKind, Result, SharedFormulaIndex};
use std::borrow::Cow;

/// Formula encoding category; shared indices are local to one worksheet.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum FormulaType {
    /// Ordinary expression.
    #[default]
    Normal,
    /// Shared template or follower; expanded text remains separately available.
    Shared {
        /// Sparse group identity.
        index: SharedFormulaIndex,
        /// Whether this source record contains the template.
        master: bool,
    },
    /// Array expression, including baseline array-based dynamic functions.
    Array,
    /// What-if data table; it may have no expression text.
    DataTable,
}
/// Optional boolean meaning plus original spelling for public properties
/// which the pinned reference exposes as source strings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormulaFlag {
    value: Option<bool>,
    source: Option<Box<str>>,
}
impl FormulaFlag {
    /// Construct a literal boolean without source spelling.
    pub const fn new(value: bool) -> Self {
        Self {
            value: Some(value),
            source: None,
        }
    }
    /// Parse XML boolean identity while retaining its source spelling.
    pub fn from_xml(value: impl Into<Box<str>>) -> Result<Self> {
        let source = value.into();
        let value = Self::literal_meaning(&source).ok_or_else(|| {
            Error::new(ErrorKind::InvalidData, "Invalid formula boolean attribute")
        })?;
        Ok(Self {
            value: Some(value),
            source: Some(source),
        })
    }
    /// Own a public literal flag without requiring XML boolean semantics.
    pub fn from_literal(value: impl Into<Box<str>>) -> Self {
        let source = value.into();
        let value = Self::literal_meaning(&source);
        Self {
            value,
            source: Some(source),
        }
    }
    fn literal_meaning(source: &str) -> Option<bool> {
        match source.trim_matches([' ', '\t', '\n', '\r']) {
            "1" | "true" => Some(true),
            "0" | "false" => Some(false),
            _ => None,
        }
    }
    /// Typed boolean meaning, absent for opaque literal properties.
    pub const fn value(&self) -> Option<bool> {
        self.value
    }
    /// Borrow the retained literal or the schema spelling of a typed boolean.
    pub fn spelling(&self) -> &str {
        self.source
            .as_deref()
            .unwrap_or(if self.value == Some(true) { "1" } else { "0" })
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
/// A literal formula reference or explicitly validated worksheet bounds.
/// Literal spelling is retained without imposing geometry on public properties.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormulaReference {
    value: ReferenceValue,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum ReferenceValue {
    Physical {
        range: CellRange,
        spelling: Option<Box<str>>,
    },
    Literal(Box<str>),
}
impl FormulaReference {
    /// Construct typed bounds without a source literal.
    pub fn new(range: CellRange) -> Result<Self> {
        CellRange::new(range.start, range.end)?;
        Ok(Self {
            value: ReferenceValue::Physical {
                range,
                spelling: None,
            },
        })
    }
    /// Parse validated bounds while retaining the source spelling.
    pub fn from_xml(value: impl Into<Box<str>>) -> Result<Self> {
        let spelling = value.into();
        let range = spelling.parse()?;
        Ok(Self {
            value: ReferenceValue::Physical {
                range,
                spelling: Some(spelling),
            },
        })
    }
    /// Own any literal property, including empty/qualified/nonrange references.
    /// Geometry is parsed only when explicitly requested by a physical operation.
    pub fn from_literal(value: impl Into<Box<str>>) -> Self {
        Self {
            value: ReferenceValue::Literal(value.into()),
        }
    }
    /// Resolve physical worksheet-local bounds; opaque/invalid literals return an
    /// error rather than fabricating geometry or panicking on user properties.
    pub fn range(&self) -> Result<CellRange> {
        match &self.value {
            ReferenceValue::Physical { range, .. } => Ok(*range),
            ReferenceValue::Literal(spelling) => spelling.parse(),
        }
    }
    /// Replace bounds and discard obsolete literal spelling.
    pub fn set_range(&mut self, range: CellRange) -> Result<()> {
        CellRange::new(range.start, range.end)?;
        self.value = ReferenceValue::Physical {
            range,
            spelling: None,
        };
        Ok(())
    }
    /// Original spelling or canonical A1 form, without parsing literal properties.
    pub fn spelling(&self) -> Cow<'_, str> {
        match &self.value {
            ReferenceValue::Physical {
                spelling: Some(value),
                ..
            }
            | ReferenceValue::Literal(value) => Cow::Borrowed(value),
            ReferenceValue::Physical {
                range,
                spelling: None,
            } => Cow::Owned(range.to_string()),
        }
    }
    /// Retained original property bytes.
    pub fn heap_bytes(&self) -> usize {
        match &self.value {
            ReferenceValue::Physical {
                spelling: Some(value),
                ..
            }
            | ReferenceValue::Literal(value) => value.len(),
            ReferenceValue::Physical { spelling: None, .. } => 0,
        }
    }
}
/// Existing name retained for callers constructing validated formula ranges.
pub type FormulaRange = FormulaReference;
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
    /// Explicitly validate nonempty input geometry without changing literal properties.
    pub fn validate_inputs(&self) -> Result<()> {
        for reference in [&self.input1, &self.input2].into_iter().flatten() {
            if !reference.is_empty() {
                reference.parse::<CellAddress>()?;
            }
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
/// Read-side cell/value metadata references attached to a formula. Literal
/// source indices are retained independently of expression and cache. Their
/// metadata part graphs are not interpreted or recreated by this model.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FormulaAnnotations {
    /// Original cm spelling, including empty or opaque literal values.
    pub cell_metadata: Option<Box<str>>,
    /// Original vm spelling, independently optional.
    pub value_metadata: Option<Box<str>>,
}
impl FormulaAnnotations {
    /// Retained literal index bytes, excluding the fixed wrapper.
    pub fn payload_bytes(&self) -> usize {
        self.cell_metadata.as_ref().map_or(0, |value| value.len())
            + self.value_metadata.as_ref().map_or(0, |value| value.len())
    }
    /// Owned wrapper and literal payload charge.
    pub fn memory_bytes(&self) -> usize {
        size_of::<Self>() + self.payload_bytes()
    }
}
/// Structured formula metadata; optional fields retain absence rather than
/// installing flags/ranges or fabricated cached results.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FormulaMetadata {
    /// The owned expression stores the public literal array text, including its
    /// first character. XML output removes that character; source records are false.
    pub literal_array_text: bool,
    /// Encoding category.
    pub kind: FormulaType,
    /// Source array/table/shared range, when present.
    pub reference: Option<FormulaReference>,
    /// Common calculation hints.
    pub flags: FormulaFlags,
    /// Data-table fields, only valid on data-table records.
    pub data_table: Option<Box<DataTableOptions>>,
    /// Explicitly requested read-side formula cm/vm references. Recreating their
    /// dependent metadata graphs is a separate unsupported write capability.
    pub annotations: Option<Box<FormulaAnnotations>>,
}
impl FormulaMetadata {
    /// Validate shared typed models before I/O/model mutation.
    pub fn validate(&self) -> Result<()> {
        if self.literal_array_text && self.kind != FormulaType::Array {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Literal array text requires an array formula",
            ));
        }
        if self.data_table.is_some() && self.kind != FormulaType::DataTable {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Data-table fields require a data-table formula",
            ));
        }
        Ok(())
    }
    /// Decoded source string bytes, excluding fixed typed wrappers.
    pub fn payload_bytes(&self) -> usize {
        (match &self.kind {
            FormulaType::Shared { index, .. } => index.payload_bytes(),
            _ => 0,
        }) + self.reference.as_ref().map_or(0, FormulaRange::heap_bytes)
            + self.flags.heap_bytes()
            + self.data_table.as_ref().map_or(0, |v| v.heap_bytes())
            + self.annotations.as_ref().map_or(0, |v| v.payload_bytes())
    }
    /// Boxed wrapper plus retained source/reference/options bytes.
    pub fn memory_bytes(&self) -> usize {
        size_of::<Self>()
            + match &self.kind {
                FormulaType::Shared { index, .. } => index.heap_bytes(),
                _ => 0,
            }
            + self.reference.as_ref().map_or(0, FormulaRange::heap_bytes)
            + self.flags.heap_bytes()
            + self
                .data_table
                .as_ref()
                .map_or(0, |v| size_of::<DataTableOptions>() + v.heap_bytes())
            + self.annotations.as_ref().map_or(0, |v| v.memory_bytes())
    }
}
