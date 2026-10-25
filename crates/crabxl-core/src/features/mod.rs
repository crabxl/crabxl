//! Worksheet and workbook features: tables, filters, conditional formats,
//! data validation, comments, protection, header/footer, defined names and
//! document properties.
//!
//! These models are small metadata. They are loaded fully; cell data stays in
//! the streaming paths.

mod auto_filter;
mod comment;
mod conditional;
mod defined_name;
mod header_footer;
mod properties;
mod protection;
mod table;
mod validation;

pub use auto_filter::*;
pub use comment::*;
pub use conditional::*;
pub use defined_name::*;
pub use header_footer::*;
pub use properties::*;
pub use protection::*;
pub use table::*;
pub use validation::*;

/// Features that belong to one worksheet.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SheetFeatures {
    /// `<sheetProtection>`.
    pub protection: Option<SheetProtection>,
    /// Worksheet `<autoFilter>`.
    pub auto_filter: Option<AutoFilter>,
    /// Worksheet `<sortState>` (outside an auto filter).
    pub sort_state: Option<SortState>,
    /// `<conditionalFormatting>` blocks in source order.
    pub conditional_formats: Vec<ConditionalFormatting>,
    /// `<dataValidations>` entries in source order.
    pub data_validations: DataValidations,
    /// `<headerFooter>`.
    pub header_footer: Option<HeaderFooter>,
    /// Tables referenced by `<tableParts>`, each in its own package part.
    pub tables: Vec<Table>,
    /// Cell comments (notes) from the comments part and its VML drawing.
    pub comments: Vec<Comment>,
}

impl SheetFeatures {
    /// True when no feature is present.
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
    /// Approximate owned heap bytes, used for memory accounting.
    pub fn heap_bytes(&self) -> usize {
        self.protection
            .as_ref()
            .map_or(0, SheetProtection::heap_bytes)
            + self.auto_filter.as_ref().map_or(0, AutoFilter::heap_bytes)
            + self.sort_state.as_ref().map_or(0, SortState::heap_bytes)
            + self
                .conditional_formats
                .iter()
                .map(|item| size_of::<ConditionalFormatting>() + item.heap_bytes())
                .sum::<usize>()
            + self.data_validations.heap_bytes()
            + self
                .header_footer
                .as_ref()
                .map_or(0, HeaderFooter::heap_bytes)
            + self
                .tables
                .iter()
                .map(|item| size_of::<Table>() + item.heap_bytes())
                .sum::<usize>()
            + self
                .comments
                .iter()
                .map(|item| size_of::<Comment>() + item.heap_bytes())
                .sum::<usize>()
    }
}

/// Features that belong to the workbook.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorkbookFeatures {
    /// `<definedNames>`, including print areas and print titles.
    pub defined_names: Vec<DefinedName>,
    /// `<workbookProtection>`.
    pub protection: Option<WorkbookProtection>,
    /// Core, extended and custom document properties.
    pub properties: DocumentProperties,
}

impl WorkbookFeatures {
    /// True when no feature is present.
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
    /// Approximate owned heap bytes, used for memory accounting.
    pub fn heap_bytes(&self) -> usize {
        self.defined_names
            .iter()
            .map(|item| size_of::<DefinedName>() + item.heap_bytes())
            .sum::<usize>()
            + self
                .protection
                .as_ref()
                .map_or(0, WorkbookProtection::heap_bytes)
            + self.properties.heap_bytes()
    }
}
