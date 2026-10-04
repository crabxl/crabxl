// SPDX-License-Identifier: MIT
// Style component/index composition adapted from umya-spreadsheet,
// Copyright (c) 2020 MathNya. IDs and budgets use shared CrabXL models.
use crate::{Alignment, Border, Color, Fill, Font, Protection, StyleId};
/// A declared number-format code without dense allocation from its numeric ID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NumberFormat {
    /// Format identity; built-in or custom definitions may appear in input.
    pub id: u32,
    /// Literal format code.
    pub code: Box<str>,
}
/// A format record referring to workbook component tables.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CellFormat {
    /// Number-format identity, default zero.
    pub number_format_id: u32,
    /// Font table index.
    pub font_id: u32,
    /// Fill table index.
    pub fill_id: u32,
    /// Border table index.
    pub border_id: u32,
    /// Optional base-style format index.
    pub base_format_id: Option<u32>,
    /// Apply number formatting.
    pub apply_number_format: Option<bool>,
    /// Apply font formatting.
    pub apply_font: Option<bool>,
    /// Apply fill formatting.
    pub apply_fill: Option<bool>,
    /// Apply border formatting.
    pub apply_border: Option<bool>,
    /// Apply alignment formatting.
    pub apply_alignment: Option<bool>,
    /// Apply protection formatting.
    pub apply_protection: Option<bool>,
    /// Quoted literal prefix indicator.
    pub quote_prefix: Option<bool>,
    /// Pivot button indicator.
    pub pivot_button: Option<bool>,
    /// Optional alignment; no inherited/default values are installed here.
    pub alignment: Option<Box<Alignment>>,
    /// Optional cell protection.
    pub protection: Option<Protection>,
    /// The original record contains an extension not modeled by this checkpoint.
    pub unmodeled_extensions: bool,
}
impl CellFormat {
    /// Retained heap payload of this record.
    pub fn heap_bytes(&self) -> usize {
        self.alignment
            .as_ref()
            .map_or(0, |_| size_of::<Alignment>())
    }
}
/// Named-style metadata retaining its base-format identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamedStyle {
    /// Display name.
    pub name: Box<str>,
    /// Base-format table index.
    pub base_format_id: u32,
    /// Optional built-in identity.
    pub builtin_id: Option<u32>,
    /// Optional custom built-in indicator.
    pub custom_builtin: Option<bool>,
    /// Optional hidden flag.
    pub hidden: Option<bool>,
    /// Optional outline level.
    pub outline_level: Option<u32>,
}
/// Shared imported style catalog. Original indices remain stable; declarations
/// never reserve from an advertised count. Derived value classification belongs
/// to the format codec, not a second style model.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StyleCatalog {
    /// Declared number formats sorted by identity after validated import.
    pub number_formats: Vec<NumberFormat>,
    /// Font components shared with rich-text overrides.
    pub fonts: Vec<Font>,
    /// Pattern/gradient components.
    pub fills: Vec<Fill>,
    /// Complete border components.
    pub borders: Vec<Border>,
    /// Base formats used by named styles.
    pub base_formats: Vec<CellFormat>,
    /// Cell formats indexed by StyleId.
    pub cell_formats: Vec<CellFormat>,
    /// Named style metadata.
    pub named_styles: Vec<NamedStyle>,
    /// Explicit indexed ARGB palette; an absent palette is not resolved here.
    pub indexed_colors: Vec<u32>,
    /// Explicit recently-used color identities.
    pub recent_colors: Vec<Color>,
    /// Known staged/unknown root sections retained by the original package,
    /// not silently represented as editable typed support.
    pub unmodeled_sections: Vec<Box<str>>,
}
impl StyleCatalog {
    /// Borrow a cell format without cloning component payloads.
    pub fn cell_format(&self, id: StyleId) -> Option<&CellFormat> {
        self.cell_formats.get(id.get() as usize)
    }
    /// Borrow a declared format code; imported declarations take priority over built-ins.
    pub fn declared_number_format(&self, id: u32) -> Option<&str> {
        self.number_formats
            .binary_search_by_key(&id, |n| n.id)
            .ok()
            .map(|i| self.number_formats[i].code.as_ref())
            .or_else(|| {
                self.number_formats
                    .iter()
                    .find(|format| format.id == id)
                    .map(|format| format.code.as_ref())
            })
    }
    /// Estimated retained heap including actual vector capacities and boxed payloads.
    pub fn memory_bytes(&self) -> usize {
        size_of::<Self>()
            + self.number_formats.capacity() * size_of::<NumberFormat>()
            + self
                .number_formats
                .iter()
                .map(|n| n.code.len())
                .sum::<usize>()
            + self.fonts.capacity() * size_of::<Font>()
            + self
                .fonts
                .iter()
                .map(|f| f.name.as_ref().map_or(0, |s| s.len()))
                .sum::<usize>()
            + self.fills.capacity() * size_of::<Fill>()
            + self.fills.iter().map(Fill::heap_bytes).sum::<usize>()
            + self.borders.capacity() * size_of::<Border>()
            + self.base_formats.capacity() * size_of::<CellFormat>()
            + self
                .base_formats
                .iter()
                .map(CellFormat::heap_bytes)
                .sum::<usize>()
            + self.cell_formats.capacity() * size_of::<CellFormat>()
            + self
                .cell_formats
                .iter()
                .map(CellFormat::heap_bytes)
                .sum::<usize>()
            + self.named_styles.capacity() * size_of::<NamedStyle>()
            + self
                .named_styles
                .iter()
                .map(|s| s.name.len())
                .sum::<usize>()
            + self.indexed_colors.capacity() * size_of::<u32>()
            + self.recent_colors.capacity() * size_of::<Color>()
            + self.unmodeled_sections.capacity() * size_of::<Box<str>>()
            + self
                .unmodeled_sections
                .iter()
                .map(|s| s.len())
                .sum::<usize>()
    }
}
