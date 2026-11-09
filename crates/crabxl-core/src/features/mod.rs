//! Worksheet and workbook features: tables, filters, conditional formats,
//! data validation, comments, protection, header/footer, defined names and
//! document properties.
//!
//! Definitions are small metadata and can be loaded fully. Encoded assets and
//! pivot records use shared memory or disk sources; cell data stays in streaming paths.

mod auto_filter;
mod comment;
mod comment_edit;
pub use comment_edit::CommentEdit;
mod complex;
mod conditional;
mod defined_name;
mod header_footer;
mod pivot;
mod properties;
pub use complex::{
    ExternalLink, MacroPolicy, PrinterSettings, VbaProject, WorkbookFormat, WorkbookMetadata,
};
pub use pivot::{PivotCache, PivotTable, PivotValue};
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
    /// Complete chart-only sheet properties; the drawing reference is managed by the package codec.
    pub chartsheet: Option<crate::XmlElement>,
    /// Chart-only sheet dependencies outside its managed drawing.
    pub chartsheet_parts: Option<crate::PartGraph>,
    /// Printer relationship payload used by page setup.
    pub printer_settings: Option<PrinterSettings>,
    /// Explicit picture replacement. None preserves an existing drawing graph.
    pub images: Option<Vec<crate::Image>>,
    /// Explicit chart replacement. None preserves existing charts.
    pub charts: Option<Vec<crate::drawing::Chart>>,
    /// Explicit shape/connector replacement; None preserves source objects.
    pub shapes: Option<Vec<crate::drawing::Shape>>,
    /// Explicit pivot-table replacement. None preserves the source graph.
    pub pivots: Option<Vec<PivotTable>>,
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
    /// Worksheet `<scenarios>` (what-if input sets), kept as an element tree.
    pub scenarios: Option<crate::XmlElement>,
}

impl SheetFeatures {
    /// Workbook-wide names owned by these tables and pivot tables.
    pub fn object_names(&self) -> impl Iterator<Item = &str> {
        self.tables
            .iter()
            .flat_map(|table| {
                std::iter::once(table.display_name.as_ref()).chain(table.name.as_deref())
            })
            .chain(
                self.pivots
                    .iter()
                    .flatten()
                    .filter_map(|pivot| pivot.definition.attribute("name")),
            )
    }
    /// Give copied table and pivot objects unique workbook-wide names.
    pub fn rename_for_copy(&mut self, taken: &mut std::collections::HashSet<String>) {
        let mut fresh = |name: &str| {
            let mut number = 1u64;
            loop {
                let candidate = format!("{name}_Copy{number}");
                if taken.insert(candidate.to_lowercase()) {
                    break candidate.into_boxed_str();
                }
                number += 1;
            }
        };
        for table in &mut self.tables {
            let name = fresh(&table.display_name);
            if table.name.is_some() {
                table.name = Some(name.clone());
            }
            table.display_name = name;
        }
        for pivot in self.pivots.iter_mut().flatten() {
            if let Some(name) = pivot.definition.attribute("name") {
                let name = fresh(name);
                pivot.definition.set_attribute("name", name);
            }
        }
    }
    /// True when no feature is present.
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
    /// Approximate owned heap bytes, used for memory accounting.
    pub fn heap_bytes(&self) -> usize {
        self.scenarios
            .as_ref()
            .map_or(0, crate::XmlElement::heap_bytes)
            + self
                .chartsheet_parts
                .as_ref()
                .map_or(0, crate::PartGraph::heap_bytes)
            + self
                .shapes
                .iter()
                .flatten()
                .map(|shape| size_of::<crate::drawing::Shape>() + shape.heap_bytes())
                .sum::<usize>()
            + self
                .chartsheet
                .as_ref()
                .map_or(0, crate::XmlElement::heap_bytes)
            + self.printer_settings.as_ref().map_or(0, |settings| {
                size_of::<PrinterSettings>() + settings.heap_bytes()
            })
            + self
                .pivots
                .iter()
                .flatten()
                .map(|v| size_of::<PivotTable>() + v.heap_bytes())
                .sum::<usize>()
            + self
                .charts
                .iter()
                .flatten()
                .map(|v| size_of::<crate::drawing::Chart>() + v.heap_bytes())
                .sum::<usize>()
            + self
                .images
                .iter()
                .flatten()
                .map(|v| size_of::<crate::Image>() + v.heap_bytes())
                .sum::<usize>()
            + self
                .protection
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
    /// Explicit workbook kind replacement; None preserves the source kind.
    pub format: Option<WorkbookFormat>,
    /// VBA preservation, removal or replacement policy.
    pub macros: MacroPolicy,
    /// Explicit ordered external-link replacement. None preserves the original graph.
    pub external_links: Option<Vec<ExternalLink>>,
    /// Explicit replacement of metadata and rich-value roots; None preserves originals.
    pub metadata: Option<Vec<WorkbookMetadata>>,
    /// `<definedNames>`, including print areas and print titles.
    pub defined_names: Vec<DefinedName>,
    /// `<workbookProtection>`.
    pub protection: Option<WorkbookProtection>,
    /// Explicit workbook property metadata; the canonical date epoch remains authoritative.
    pub workbook_properties: Option<crate::XmlElement>,
    /// Explicit workbook-view metadata; None keeps the default/native view handling.
    pub views: Option<crate::XmlElement>,
    /// Explicit calculation settings, retaining complete SpreadsheetML metadata.
    pub calculation: Option<crate::XmlElement>,
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
        self.metadata
            .iter()
            .flatten()
            .map(|part| size_of::<WorkbookMetadata>() + part.heap_bytes())
            .sum::<usize>()
            + self
                .external_links
                .iter()
                .flatten()
                .map(|link| size_of::<ExternalLink>() + link.heap_bytes())
                .sum::<usize>()
            + match &self.macros {
                MacroPolicy::Replace(project) => project.heap_bytes(),
                _ => 0,
            }
            + self
                .defined_names
                .iter()
                .map(|item| size_of::<DefinedName>() + item.heap_bytes())
                .sum::<usize>()
            + self
                .protection
                .as_ref()
                .map_or(0, WorkbookProtection::heap_bytes)
            + self.workbook_properties.as_ref().map_or(0, |value| {
                size_of::<crate::XmlElement>() + value.heap_bytes()
            })
            + self.views.as_ref().map_or(0, |value| {
                size_of::<crate::XmlElement>() + value.heap_bytes()
            })
            + self.calculation.as_ref().map_or(0, |value| {
                size_of::<crate::XmlElement>() + value.heap_bytes()
            })
            + self.properties.heap_bytes()
    }
}
