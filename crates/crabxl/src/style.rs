//! Cell appearance.
//!
//! A [`Style`] is an immutable, cheap-to-clone value that describes the
//! complete appearance of a cell: number format, font, fill, border,
//! alignment and protection. A workbook stores each distinct style once.
//!
//! ```
//! use crabxl::style::{Color, Fill, Font, Style};
//! # fn main() -> crabxl::Result<()> {
//! let header = Style::new()
//!     .with_font(Font::new().with_bold(true))
//!     .with_fill(Fill::solid(Color::hex("DDEBF7")?));
//! let money = header.clone().with_number_format("#,##0.00");
//! assert_ne!(header, money);
//! # Ok(())
//! # }
//! ```

use crabxl_core::CellStyle;
use std::sync::Arc;

pub use crabxl_core::{
    Alignment, Border, BorderLine, BorderSide, Color, Fill, FillPattern, Font, HorizontalAlignment,
    Protection, Underline, VerticalAlignment,
};

/// The complete appearance of a cell.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Style(Arc<CellStyle>);

impl Style {
    /// The default appearance: General number format and the workbook's
    /// default font, with no fill, border or alignment overrides.
    pub fn new() -> Self {
        Self::default()
    }
    fn edit(self, change: impl FnOnce(CellStyle) -> CellStyle) -> Self {
        Self(Arc::new(change(Arc::unwrap_or_clone(self.0))))
    }
    /// Set the number format code, such as `"0.00"`, `"0%"` or `"yyyy-mm-dd"`.
    pub fn with_number_format(self, code: impl Into<Box<str>>) -> Self {
        let code = code.into();
        self.edit(|style| style.with_number_format(code))
    }
    /// Set the font. Attributes the font leaves unset keep their defaults.
    pub fn with_font(self, font: Font) -> Self {
        self.edit(|style| style.with_font(font))
    }
    /// Set the fill.
    pub fn with_fill(self, fill: Fill) -> Self {
        self.edit(|style| style.with_fill(fill))
    }
    /// Set the border.
    pub fn with_border(self, border: Border) -> Self {
        self.edit(|style| style.with_border(border))
    }
    /// Set the alignment.
    pub fn with_alignment(self, alignment: Alignment) -> Self {
        self.edit(|style| style.with_alignment(alignment))
    }
    /// Set the protection flags.
    pub fn with_protection(self, protection: Protection) -> Self {
        self.edit(|style| style.with_protection(protection))
    }
    /// The number format code.
    pub fn number_format(&self) -> &str {
        &self.0.number_format
    }
    /// The font.
    pub fn font(&self) -> &Font {
        &self.0.font
    }
    /// The fill.
    pub fn fill(&self) -> &Fill {
        &self.0.fill
    }
    /// The border.
    pub fn border(&self) -> &Border {
        &self.0.borders
    }
    /// The alignment.
    pub fn alignment(&self) -> &Alignment {
        &self.0.alignment
    }
    /// The protection flags.
    pub fn protection(&self) -> &Protection {
        &self.0.protection
    }
    /// Identity of the shared appearance, stable while a clone is alive.
    pub(crate) fn identity(&self) -> usize {
        Arc::as_ptr(&self.0) as usize
    }
    pub(crate) fn from_core(style: CellStyle) -> Self {
        Self(Arc::new(style))
    }
    pub(crate) fn to_core(&self) -> CellStyle {
        (*self.0).clone()
    }
}

/// Formatting that a conditional format applies on top of a cell's style.
/// Only the parts that are set change.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PartialStyle {
    pub(crate) inner: crabxl_core::DifferentialStyle,
}

impl PartialStyle {
    /// Formatting that changes nothing.
    pub fn new() -> Self {
        Self::default()
    }
    /// Change the font.
    pub fn with_font(mut self, font: Font) -> Self {
        self.inner.font = Some(Box::new(font));
        self
    }
    /// Change the fill.
    pub fn with_fill(mut self, fill: Fill) -> Self {
        self.inner.fill = Some(Box::new(fill));
        self
    }
    /// Change the border.
    pub fn with_border(mut self, border: Border) -> Self {
        self.inner.border = Some(Box::new(border));
        self
    }
    /// The font it sets.
    pub fn font(&self) -> Option<&Font> {
        self.inner.font.as_deref()
    }
    /// The fill it sets.
    pub fn fill(&self) -> Option<&Fill> {
        self.inner.fill.as_deref()
    }
    /// The border it sets.
    pub fn border(&self) -> Option<&Border> {
        self.inner.border.as_deref()
    }
}
