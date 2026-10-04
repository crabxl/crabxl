/// A workbook-local index into the shared cell-format table.
///
/// Readers, editable models, and writers use the same identity. Index zero is
/// the default record, not a guarantee that its formatting is General. Styles
/// and full read-side format interpretation are M2/M5 work; constructing an ID does not prove
/// that a referenced record exists in a particular workbook.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StyleId(u32);
impl StyleId {
    /// Construct a workbook-local format identity.
    pub const fn new(index: u32) -> Self {
        Self(index)
    }
    /// Return the zero-based table index.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Initial shared font model. Colors are opaque RGB (0xRRGGBB).
#[derive(Clone, Debug, PartialEq)]
pub struct Font {
    /// Typeface name.
    pub name: Box<str>,
    /// Point size.
    pub size: f64,
    /// Bold face.
    pub bold: bool,
    /// Italic face.
    pub italic: bool,
    /// Single underline.
    pub underline: bool,
    /// Optional RGB text color.
    pub color: Option<u32>,
}
impl Default for Font {
    fn default() -> Self {
        Self {
            name: "Calibri".into(),
            size: 11.0,
            bold: false,
            italic: false,
            underline: false,
            color: None,
        }
    }
}
/// Basic border line style.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BorderLine {
    /// Thin line.
    Thin,
    /// Medium line.
    Medium,
    /// Thick line.
    Thick,
    /// Dashed line.
    Dashed,
    /// Dotted line.
    Dotted,
    /// Double line.
    Double,
}
impl BorderLine {
    /// SpreadsheetML token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Thin => "thin",
            Self::Medium => "medium",
            Self::Thick => "thick",
            Self::Dashed => "dashed",
            Self::Dotted => "dotted",
            Self::Double => "double",
        }
    }
}
/// One border side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BorderSide {
    /// Line shape.
    pub line: BorderLine,
    /// Optional RGB color.
    pub color: Option<u32>,
}
/// Horizontal cell alignment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HorizontalAlignment {
    /// Spreadsheet default.
    #[default]
    General,
    /// Left aligned.
    Left,
    /// Centered.
    Center,
    /// Right aligned.
    Right,
}
impl HorizontalAlignment {
    /// SpreadsheetML token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
        }
    }
}
/// Vertical cell alignment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VerticalAlignment {
    /// Bottom aligned.
    #[default]
    Bottom,
    /// Centered.
    Center,
    /// Top aligned.
    Top,
}
impl VerticalAlignment {
    /// SpreadsheetML token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bottom => "bottom",
            Self::Center => "center",
            Self::Top => "top",
        }
    }
}
/// Basic shared cell format. Named styles, themes, gradient/pattern fills,
/// diagonal borders and advanced typography remain explicit M5 extensions.
#[derive(Clone, Debug, PartialEq)]
pub struct CellStyle {
    /// Number-format code (General by default).
    pub number_format: Box<str>,
    /// Font attributes.
    pub font: Font,
    /// Optional solid background RGB.
    pub fill: Option<u32>,
    /// Left, right, top and bottom border sides, in that order.
    pub borders: [Option<BorderSide>; 4],
    /// Horizontal alignment.
    pub horizontal: HorizontalAlignment,
    /// Vertical alignment.
    pub vertical: VerticalAlignment,
    /// Wrap long text.
    pub wrap_text: bool,
    /// Shrink text to fit (mutually exclusive with wrap_text).
    pub shrink_to_fit: bool,
    /// Clockwise text rotation, 0..=180 in SpreadsheetML encoding.
    pub rotation: u8,
    /// Lock cells when sheet protection is enabled.
    pub locked: bool,
    /// Hide formulas when sheet protection is enabled.
    pub hidden: bool,
}
impl Default for CellStyle {
    fn default() -> Self {
        Self {
            number_format: "General".into(),
            font: Font::default(),
            fill: None,
            borders: [None; 4],
            horizontal: HorizontalAlignment::General,
            vertical: VerticalAlignment::Bottom,
            wrap_text: false,
            shrink_to_fit: false,
            rotation: 0,
            locked: true,
            hidden: false,
        }
    }
}
impl CellStyle {
    /// Payload bytes owned by the shared format, excluding allocator overhead.
    pub fn heap_bytes(&self) -> usize {
        self.number_format.len() + self.font.name.len()
    }
}
