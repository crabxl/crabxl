//! Canonical differential formatting and table/pivot style definitions.
use crate::{
    Alignment, Border, Error, ErrorKind, Fill, Font, NumberFormat, Protection, Result,
    TableStyleRegion,
};
/// Sparse appearance overrides; absent components do not install cell defaults.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DifferentialStyle {
    /// Font overrides.
    pub font: Option<Box<Font>>,
    /// Literal number-format identity and code, independent of cell declarations.
    pub number_format: Option<NumberFormat>,
    /// Fill overrides.
    pub fill: Option<Box<Fill>>,
    /// Alignment overrides.
    pub alignment: Option<Box<Alignment>>,
    /// Border overrides.
    pub border: Option<Box<Border>>,
    /// Protection overrides.
    pub protection: Option<Protection>,
    /// Source contains an extension requiring original-package preservation.
    pub unmodeled_extensions: bool,
}
impl DifferentialStyle {
    /// Validate shared components without inheriting cell appearance.
    pub fn validate(&self) -> Result<()> {
        if let Some(font) = &self.font {
            font.validate()?;
        }
        if let Some(fill) = &self.fill {
            fill.validate()?;
        }
        if let Some(border) = &self.border {
            border.validate()?;
        }
        if let Some(alignment) = &self.alignment {
            alignment.validate()?;
        }
        Ok(())
    }
    /// Retained component wrappers, strings and gradient capacity.
    pub fn heap_bytes(&self) -> usize {
        self.font
            .as_ref()
            .map_or(0, |v| size_of::<Font>() + v.heap_bytes())
            + self.number_format.as_ref().map_or(0, |v| v.code().len())
            + self
                .fill
                .as_ref()
                .map_or(0, |v| size_of::<Fill>() + v.heap_bytes())
            + self
                .alignment
                .as_ref()
                .map_or(0, |_| size_of::<Alignment>())
            + self
                .border
                .as_ref()
                .map_or(0, |v| size_of::<Border>() + v.heap_bytes())
    }
}
/// One table/pivot region's differential formatting reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableStyleElement {
    /// Region receiving the override.
    pub region: TableStyleRegion,
    /// Stripe/subheading size, when present.
    pub size: Option<u32>,
    /// Workbook-local differential style ID, when present.
    pub differential_style_id: Option<u32>,
}
/// Named custom table/pivot style with source-ordered elements.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableStyle {
    /// Display name.
    pub name: Box<str>,
    /// Applies to pivot tables, retaining source absence.
    pub pivot: Option<bool>,
    /// Applies to tables, retaining source absence.
    pub table: Option<bool>,
    /// Optional source-advertised count; it never controls allocation or validation.
    pub count: Option<u32>,
    /// Source-ordered region definitions.
    pub elements: Vec<TableStyleElement>,
}
impl TableStyle {
    /// Retained name and actual element capacity.
    pub fn heap_bytes(&self) -> usize {
        self.name.len() + self.elements.capacity() * size_of::<TableStyleElement>()
    }
}
/// Default style names and named custom definitions; advertised counts are ignored.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TableStyleCatalog {
    /// Explicit default table style, with absence retained.
    pub default_table_style: Option<Box<str>>,
    /// Explicit default pivot style, with absence retained.
    pub default_pivot_style: Option<Box<str>>,
    /// Named definitions in source order.
    pub styles: Vec<TableStyle>,
}
impl TableStyleCatalog {
    /// Retained vector capacity, names and nested element capacities.
    pub fn heap_bytes(&self) -> usize {
        self.default_table_style.as_ref().map_or(0, |v| v.len())
            + self.default_pivot_style.as_ref().map_or(0, |v| v.len())
            + self.styles.capacity() * size_of::<TableStyle>()
            + self
                .styles
                .iter()
                .map(TableStyle::heap_bytes)
                .sum::<usize>()
    }
    /// Validate differential references without dense allocation or range expansion.
    pub fn validate_references(&self, differential_count: usize) -> Result<()> {
        for style in &self.styles {
            for element in &style.elements {
                if element
                    .differential_style_id
                    .is_some_and(|id| id as usize >= differential_count)
                {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        "Table style references a missing differential style",
                    ));
                }
            }
        }
        Ok(())
    }
}
