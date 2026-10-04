// SPDX-License-Identifier: MIT
// Style component/index composition adapted from umya-spreadsheet,
// Copyright (c) 2020 MathNya. IDs and budgets use shared CrabXL models.
use crate::{Alignment, Border, Color, Fill, Font, Protection, StyleId};
/// A declared number-format code without dense allocation from its numeric ID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NumberFormat {
    id: u32,
    code: Box<str>,
    date_kind: Option<crate::DateKind>,
}
impl NumberFormat {
    /// Retain the literal code and its allocation-free canonical classification.
    pub fn new(id: u32, code: impl Into<Box<str>>) -> Self {
        let code = code.into();
        let date_kind = crate::classify_number_format(&code);
        Self {
            id,
            code,
            date_kind,
        }
    }
    /// Source identity, including custom declarations overriding built-ins.
    pub fn id(&self) -> u32 {
        self.id
    }
    /// Borrow the original format spelling.
    pub fn code(&self) -> &str {
        &self.code
    }
    /// Cached date/duration classification of the literal code.
    pub fn date_kind(&self) -> Option<crate::DateKind> {
        self.date_kind
    }
    /// Replace the code while keeping its classification consistent.
    pub fn set_code(&mut self, code: impl Into<Box<str>>) {
        let code = code.into();
        self.date_kind = crate::classify_number_format(&code);
        self.code = code;
    }
}
/// A format record referring to workbook component tables.
#[derive(Clone, Debug, Default, PartialEq, Hash)]
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
/// Borrowed appearance components for an existing cell-format identity.
/// Source application/base-style flags remain accessible through format.
/// This view installs no inferred defaults or locale-specific format codes.
#[derive(Clone, Copy, Debug)]
pub struct StyleView<'a> {
    /// Original format record, including flags and table identities.
    pub format: &'a CellFormat,
    /// Declared or portable built-in number format, absent for an unknown ID.
    pub number_format: Option<&'a str>,
    /// Shared font component.
    pub font: &'a Font,
    /// Shared fill component.
    pub fill: &'a Fill,
    /// Shared border component.
    pub border: &'a Border,
    /// Explicit alignment component, retaining source absence.
    pub alignment: Option<&'a Alignment>,
    /// Explicit protection overrides.
    pub protection: Option<&'a Protection>,
}
/// Shared imported style catalog. Original indices remain stable; declarations
/// never reserve from an advertised count. Number-format classification is cached
/// by canonical records; format codecs build bounded indexed cell lookups.
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
    /// Resolve shared components without cloning names, gradients or wrappers.
    /// This does not invent base-style inheritance beyond explicit table links.
    pub fn cell_style(&self, id: StyleId) -> crate::Result<StyleView<'_>> {
        let invalid = || {
            crate::Error::new(
                crate::ErrorKind::InvalidData,
                "Cell format references a missing style component",
            )
        };
        let format = self.cell_format(id).ok_or_else(invalid)?;
        Ok(StyleView {
            format,
            number_format: self.number_format(format.number_format_id),
            font: self
                .fonts
                .get(format.font_id as usize)
                .ok_or_else(invalid)?,
            fill: self
                .fills
                .get(format.fill_id as usize)
                .ok_or_else(invalid)?,
            border: self
                .borders
                .get(format.border_id as usize)
                .ok_or_else(invalid)?,
            alignment: format.alignment.as_deref(),
            protection: format.protection.as_ref(),
        })
    }
    /// Validate component/base/custom-format links without cloning or dense IDs.
    /// Unknown extension markers remain explicit and are not interpreted here.
    pub fn validate_references(&self) -> crate::Result<()> {
        let invalid = || {
            crate::Error::new(
                crate::ErrorKind::InvalidData,
                "Style catalog contains a missing reference",
            )
        };
        for format in self.cell_formats.iter().chain(&self.base_formats) {
            self.validate_format(format)?;
        }
        if self
            .named_styles
            .iter()
            .any(|v| v.base_format_id as usize >= self.base_formats.len())
        {
            return Err(invalid());
        }
        Ok(())
    }
    /// Validate one candidate format against these existing canonical components.
    pub fn validate_format(&self, format: &CellFormat) -> crate::Result<()> {
        let invalid = || {
            crate::Error::new(
                crate::ErrorKind::InvalidData,
                "Style catalog contains a missing reference",
            )
        };
        if format.number_format_id >= 164
            && self
                .declared_number_format(format.number_format_id)
                .is_none()
        {
            return Err(invalid());
        }
        if format.font_id as usize >= self.fonts.len()
            || format.fill_id as usize >= self.fills.len()
            || format.border_id as usize >= self.borders.len()
            || format
                .base_format_id
                .is_some_and(|id| id as usize >= self.base_formats.len())
        {
            return Err(invalid());
        }
        if let Some(alignment) = &format.alignment {
            alignment.validate()?;
        }
        Ok(())
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
    /// Borrow a declared or portable built-in code without per-call allocation.
    /// Unknown locale identities remain None; original numeric IDs stay intact.
    pub fn number_format(&self, id: u32) -> Option<&str> {
        self.declared_number_format(id)
            .or_else(|| crate::builtin_number_format(id))
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
