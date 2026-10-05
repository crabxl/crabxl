// SPDX-License-Identifier: MIT
// Printing/page-break layouts selected from rust_xlsxwriter worksheet.rs,
// Copyright 2022-2026 John McNamara. Shared models and public descriptor semantics
// are adapted to CrabXL; see third_party/ports.json.
//! Runtime-independent printing settings, retaining explicit optional values.
use crate::{Error, ErrorKind, Result};

macro_rules! tokens {
    ($name:ident, $description:literal, {$($variant:ident => $text:literal),+ $(,)?}) => {
        #[doc = $description]
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum $name { $(#[doc = $text] $variant),+ }
        impl $name {
            /// Public/OOXML token spelling.
            pub const fn as_str(self) -> &'static str { match self { $(Self::$variant => $text),+ } }
            /// Validate a public/OOXML token.
            pub fn parse(value: &str) -> Result<Self> { match value { $($text => Ok(Self::$variant)),+, _ => Err(Error::new(ErrorKind::InvalidData, concat!("Invalid ", $description))) } }
        }
    };
}
tokens!(PageOrientation, "page orientation", { Portrait => "portrait", Landscape => "landscape", Default => "default" });
tokens!(PageOrder, "page traversal order", { DownThenOver => "downThenOver", OverThenDown => "overThenDown" });
tokens!(PrintedComments, "printed comment placement", { AsDisplayed => "asDisplayed", AtEnd => "atEnd" });
tokens!(PrintedErrors, "printed error representation", { Displayed => "displayed", Blank => "blank", Dash => "dash", NotAvailable => "NA" });

/// A literal paper dimension validated against the baseline public prefix pattern.
/// Trailing source spelling is retained, matching the reference descriptor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaperDimension(Box<str>);
impl PaperDimension {
    /// Accept a decimal prefix followed by mm/cm/in/pt/pc/pi; retain its spelling.
    pub fn parse(value: &str) -> Result<Self> {
        let bytes = value.as_bytes();
        let mut end = 0;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        if end == 0 {
            return Err(invalid("Invalid paper dimension"));
        }
        if bytes.get(end) == Some(&b'.') {
            end += 1;
            let start = end;
            while bytes.get(end).is_some_and(u8::is_ascii_digit) {
                end += 1;
            }
            if start == end {
                return Err(invalid("Invalid paper dimension"));
            }
        }
        if !matches!(
            bytes.get(end..end + 2),
            Some(b"mm" | b"cm" | b"in" | b"pt" | b"pc" | b"pi")
        ) {
            return Err(invalid("Invalid paper dimension"));
        }
        Ok(Self(value.into()))
    }
    /// Borrow the validated literal, without another allocation.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
/// Six page margins in inches; finite negative descriptor values are retained.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageMargins {
    /// Left margin.
    pub left: f64,
    /// Right margin.
    pub right: f64,
    /// Top margin.
    pub top: f64,
    /// Bottom margin.
    pub bottom: f64,
    /// Header margin.
    pub header: f64,
    /// Footer margin.
    pub footer: f64,
}
impl Default for PageMargins {
    fn default() -> Self {
        Self {
            left: 0.75,
            right: 0.75,
            top: 1.0,
            bottom: 1.0,
            header: 0.5,
            footer: 0.5,
        }
    }
}
impl PageMargins {
    /// Reject nonfinite values rather than emitting invalid empty reference attributes.
    pub fn validate(&self) -> Result<()> {
        if [
            self.left,
            self.right,
            self.top,
            self.bottom,
            self.header,
            self.footer,
        ]
        .iter()
        .any(|value| !value.is_finite())
        {
            return Err(invalid("Page margins must be finite"));
        }
        Ok(())
    }
}
/// Optional print flags; explicit false remains distinct from absent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PrintOptions {
    /// Center horizontally.
    pub horizontal_centered: Option<bool>,
    /// Center vertically.
    pub vertical_centered: Option<bool>,
    /// Print row/column headings.
    pub headings: Option<bool>,
    /// Print grid lines.
    pub grid_lines: Option<bool>,
    /// Explicit grid-line setting indicator.
    pub grid_lines_set: Option<bool>,
}
/// Optional baseline paper/scaling/printer settings; integer source values use i64.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PageSetup {
    /// Paper orientation.
    pub orientation: Option<PageOrientation>,
    /// Paper code.
    pub paper_size: Option<i64>,
    /// Print scale.
    pub scale: Option<i64>,
    /// Fit height in pages.
    pub fit_to_height: Option<i64>,
    /// Fit width in pages.
    pub fit_to_width: Option<i64>,
    /// First page number.
    pub first_page_number: Option<i64>,
    /// Use the first page number.
    pub use_first_page_number: Option<bool>,
    /// Literal paper height.
    pub paper_height: Option<PaperDimension>,
    /// Literal paper width.
    pub paper_width: Option<PaperDimension>,
    /// Page traversal order.
    pub page_order: Option<PageOrder>,
    /// Use printer defaults.
    pub use_printer_defaults: Option<bool>,
    /// Print in black and white.
    pub black_and_white: Option<bool>,
    /// Draft printing.
    pub draft: Option<bool>,
    /// Printed comment placement.
    pub cell_comments: Option<PrintedComments>,
    /// Printed error representation.
    pub errors: Option<PrintedErrors>,
    /// Horizontal resolution.
    pub horizontal_dpi: Option<i64>,
    /// Vertical resolution.
    pub vertical_dpi: Option<i64>,
    /// Copy count.
    pub copies: Option<i64>,
    /// Original printer relationship identity. New-package graph creation is separate.
    pub printer_relationship: Option<Box<str>>,
}
impl PageSetup {
    /// Owned literal payloads outside this fixed record.
    pub fn heap_bytes(&self) -> usize {
        self.paper_height
            .as_ref()
            .map_or(0, |value| value.as_str().len())
            + self
                .paper_width
                .as_ref()
                .map_or(0, |value| value.as_str().len())
            + self
                .printer_relationship
                .as_ref()
                .map_or(0, |value| value.len())
    }
}
/// An explicit page break, stored sparsely without expanding row/column ranges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageBreak {
    /// Break identity/position.
    pub id: Option<i64>,
    /// Minimum affected position.
    pub minimum: Option<i64>,
    /// Maximum affected position.
    pub maximum: Option<i64>,
    /// Manual break indicator.
    pub manual: Option<bool>,
    /// Pivot-table break indicator.
    pub pivot: Option<bool>,
}
impl Default for PageBreak {
    fn default() -> Self {
        Self {
            id: Some(0),
            minimum: Some(0),
            maximum: Some(16383),
            manual: Some(true),
            pivot: None,
        }
    }
}
/// Canonical printing metadata. Absence of this entire model costs no sheet allocation.
#[derive(Clone, Debug, PartialEq)]
pub struct PrintSettings {
    /// Automatic page-break setting from sheetPr/pageSetUpPr.
    pub auto_page_breaks: Option<bool>,
    /// Fit-to-page setting from sheetPr/pageSetUpPr.
    pub fit_to_page: Option<bool>,
    /// Optional page margins; reference loading defaults missing margins.
    pub margins: Option<PageMargins>,
    /// Optional print flags.
    pub options: PrintOptions,
    /// Optional paper/scaling settings.
    pub setup: PageSetup,
    /// Source-ordered horizontal breaks.
    pub row_breaks: Vec<PageBreak>,
    /// Source-ordered vertical breaks.
    pub column_breaks: Vec<PageBreak>,
}
impl Default for PrintSettings {
    fn default() -> Self {
        Self {
            auto_page_breaks: None,
            fit_to_page: None,
            margins: Some(PageMargins::default()),
            options: PrintOptions::default(),
            setup: PageSetup::default(),
            row_breaks: Vec::new(),
            column_breaks: Vec::new(),
        }
    }
}
impl PrintSettings {
    /// Fixed model, actual break vector capacities and string payloads.
    pub fn memory_bytes(&self) -> usize {
        size_of::<Self>()
            + self.setup.heap_bytes()
            + (self.row_breaks.capacity() + self.column_breaks.capacity()) * size_of::<PageBreak>()
    }
    /// Validate canonical numeric invariants; XLSX validates XML/relationships.
    pub fn validate(&self) -> Result<()> {
        if let Some(margins) = &self.margins {
            margins.validate()?;
        }
        Ok(())
    }
}
fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    #[test]
    fn public_paper_prefix_validation_retains_literal_spelling() {
        for value in ["1in", "1.5mm", "11.5in tail", "1intrash", "0pc", "02.00pi"] {
            assert_eq!(PaperDimension::parse(value).unwrap().as_str(), value);
        }
        for value in ["-2cm", "opaque", "1e2pt", "1.", "2PI", "1.5.3in", "1.in"] {
            assert!(PaperDimension::parse(value).is_err());
        }
    }
    #[test]
    fn finite_negative_margins_and_actual_break_capacities_are_charged() {
        assert!(
            PageMargins {
                left: -1.5,
                ..PageMargins::default()
            }
            .validate()
            .is_ok()
        );
        let mut settings = PrintSettings::default();
        let before = settings.memory_bytes();
        settings.row_breaks.reserve_exact(20);
        settings.row_breaks.push(PageBreak::default());
        assert_eq!(
            settings.memory_bytes(),
            before + settings.row_breaks.capacity() * size_of::<PageBreak>()
        );
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(
                PageMargins {
                    left: value,
                    ..PageMargins::default()
                }
                .validate()
                .is_err()
            );
        }
    }
}
