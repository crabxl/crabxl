// SPDX-License-Identifier: MIT
// Run/font composition adapted from umya-spreadsheet, Copyright (c) 2020 MathNya.
// Provenance and refactoring: third_party/ports.json.

/// Workbook-independent color reference. Theme/indexed colors retain identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorKind {
    /// A present color element without an explicit identity.
    Unspecified,
    /// Eight-digit ARGB channels, including alpha.
    Argb(u32),
    /// Theme slot; resolve through the workbook theme when needed.
    Theme(u32),
    /// Indexed palette slot.
    Indexed(u32),
    /// Automatic color setting, including explicit false.
    Auto(bool),
}
/// Color identity and optional tint without eager RGB conversion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    /// Underlying color reference.
    pub kind: ColorKind,
    /// Optional signed tint; absent and explicit zero retain their encoding distinction.
    pub tint: Option<f64>,
}
/// Underline shape, distinct from a missing run property.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Underline {
    /// Explicitly no underline.
    None,
    /// Single underline.
    Single,
    /// Double underline.
    Double,
    /// Single accounting underline.
    SingleAccounting,
    /// Double accounting underline.
    DoubleAccounting,
}
/// Run baseline position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextVerticalAlignment {
    /// Normal baseline.
    Baseline,
    /// Raised text.
    Superscript,
    /// Lowered text.
    Subscript,
}
/// Theme font scheme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontScheme {
    /// Explicitly no theme scheme.
    None,
    /// Major theme font.
    Major,
    /// Minor theme font.
    Minor,
}
/// Optional run font overrides. Absent properties retain inheritance; explicit
/// false properties must not be confused with absent ones.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RunFont {
    /// Optional typeface name.
    pub name: Option<Box<str>>,
    /// Optional point size.
    pub size: Option<f64>,
    /// Bold override.
    pub bold: Option<bool>,
    /// Italic override.
    pub italic: Option<bool>,
    /// Strike-through override.
    pub strike: Option<bool>,
    /// Outline override.
    pub outline: Option<bool>,
    /// Shadow override.
    pub shadow: Option<bool>,
    /// Condensed text override.
    pub condense: Option<bool>,
    /// Extended text override.
    pub extend: Option<bool>,
    /// Underline override.
    pub underline: Option<Underline>,
    /// Vertical baseline override.
    pub vertical: Option<TextVerticalAlignment>,
    /// Charset number.
    pub charset: Option<u8>,
    /// Font family number.
    pub family: Option<u8>,
    /// Theme font scheme.
    pub scheme: Option<FontScheme>,
    /// Color reference with optional tint.
    pub color: Option<Color>,
}
/// A contiguous run; None means ordinary text with no explicit run properties.
#[derive(Clone, Debug, PartialEq)]
pub struct RichTextRun {
    /// Owned text, including meaningful whitespace and empty styled runs.
    pub text: Box<str>,
    /// Explicit formatting, distinct from absent formatting.
    pub font: Option<Box<RunFont>>,
}
/// Pronunciation text attached to a source character range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhoneticRun {
    /// Source start offset retained in its format-defined units.
    pub start: u32,
    /// Source exclusive end offset.
    pub end: u32,
    /// Pronunciation text, separate from the main displayed text.
    pub text: Box<str>,
}
/// Phonetic rendering settings; font identity belongs to the workbook font catalog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhoneticProperties {
    /// Workbook font index, not a cell StyleId.
    pub font_id: u32,
    /// Optional phonetic type token.
    pub kind: Option<Box<str>>,
    /// Optional alignment token.
    pub alignment: Option<Box<str>>,
}
/// Owned rich text without a redundant flattened-string allocation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RichText {
    /// Ordered display runs, including unformatted runs.
    pub runs: Vec<RichTextRun>,
    /// Separate pronunciation annotations.
    pub phonetic_runs: Vec<PhoneticRun>,
    /// Optional phonetic rendering settings.
    pub phonetic_properties: Option<Box<PhoneticProperties>>,
}
impl RichText {
    /// Iterator over displayed text slices; pronunciation is excluded.
    pub fn text_parts(&self) -> impl Iterator<Item = &str> {
        self.runs.iter().map(|run| run.text.as_ref())
    }
    /// Allocate one flattened display string using checked exact capacity.
    pub fn plain_text(&self) -> crate::Result<Box<str>> {
        let length = self
            .runs
            .iter()
            .try_fold(0usize, |n, run| n.checked_add(run.text.len()))
            .ok_or_else(|| {
                crate::Error::new(
                    crate::ErrorKind::LimitExceeded,
                    "Rich-text length overflows",
                )
            })?;
        let mut text = String::new();
        text.try_reserve_exact(length).map_err(|e| {
            crate::Error::caused_by(
                crate::ErrorKind::MemoryBudgetExceeded,
                "Cannot allocate plain rich-text projection",
                e,
            )
        })?;
        for part in self.text_parts() {
            text.push_str(part);
        }
        Ok(text.into_boxed_str())
    }
    /// Retained heap allocation including this boxed model and vector capacities.
    pub fn memory_bytes(&self) -> usize {
        size_of::<Self>()
            + self.runs.capacity() * size_of::<RichTextRun>()
            + self
                .runs
                .iter()
                .map(|r| {
                    r.text.len()
                        + r.font.as_ref().map_or(0, |f| {
                            size_of::<RunFont>() + f.name.as_ref().map_or(0, |n| n.len())
                        })
                })
                .sum::<usize>()
            + self.phonetic_runs.capacity() * size_of::<PhoneticRun>()
            + self
                .phonetic_runs
                .iter()
                .map(|r| r.text.len())
                .sum::<usize>()
            + self.phonetic_properties.as_ref().map_or(0, |p| {
                size_of::<PhoneticProperties>()
                    + p.kind.as_ref().map_or(0, |s| s.len())
                    + p.alignment.as_ref().map_or(0, |s| s.len())
            })
    }
}
