//! Builder methods for the style models. Each `with_*` method consumes and
//! returns the value, so styles compose in one expression.

use crate::{
    Alignment, Border, BorderLine, BorderSide, CellStyle, Color, ColorKind, Error, ErrorKind, Fill,
    FillPattern, Font, HorizontalAlignment, PatternFill, Protection, Result, Underline,
    VerticalAlignment,
};

impl Color {
    /// An opaque RGB color.
    pub const fn rgb(red: u8, green: u8, blue: u8) -> Self {
        Self {
            kind: ColorKind::Argb(
                0xFF00_0000 | (red as u32) << 16 | (green as u32) << 8 | blue as u32,
            ),
            tint: None,
        }
    }
    /// Parse `RRGGBB` or `AARRGGBB` hexadecimal text, with an optional `#`.
    pub fn hex(text: &str) -> Result<Self> {
        let digits = text.strip_prefix('#').unwrap_or(text);
        let invalid = || {
            Error::new(
                ErrorKind::InvalidData,
                format!("Invalid hex color '{text}'"),
            )
        };
        if !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(invalid());
        }
        let value = u32::from_str_radix(digits, 16).map_err(|_| invalid())?;
        let argb = match digits.len() {
            6 => 0xFF00_0000 | value,
            8 => value,
            _ => return Err(invalid()),
        };
        Ok(Self {
            kind: ColorKind::Argb(argb),
            tint: None,
        })
    }
    /// A theme color by index (0 to 11), with a tint from -1.0 (darker) to
    /// 1.0 (lighter).
    pub fn theme(index: u32, tint: f64) -> Result<Self> {
        if index > 11 || !(-1.0..=1.0).contains(&tint) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "A theme color needs an index from 0 to 11 and a tint from -1 to 1",
            ));
        }
        Ok(Self {
            kind: ColorKind::Theme(i64::from(index).into()),
            tint: (tint != 0.0).then_some(tint),
        })
    }
    /// The RGB channels of an explicit color, or `None` for theme, indexed
    /// and automatic colors.
    pub fn to_rgb(&self) -> Option<(u8, u8, u8)> {
        let argb = match &self.kind {
            ColorKind::Argb(argb) => *argb,
            ColorKind::ArgbLiteral(literal) => literal.channels(),
            _ => return None,
        };
        Some(((argb >> 16) as u8, (argb >> 8) as u8, argb as u8))
    }
}

impl Font {
    /// A font that inherits every attribute from the workbook default.
    pub fn new() -> Self {
        Self::default()
    }
    /// Set the typeface name.
    pub fn with_name(mut self, name: impl Into<Box<str>>) -> Self {
        self.name = Some(name.into());
        self
    }
    /// Set the size in points.
    pub fn with_size(mut self, points: f64) -> Self {
        self.size = Some(points);
        self
    }
    /// Set or clear bold.
    pub fn with_bold(mut self, bold: bool) -> Self {
        self.bold = Some(bold);
        self
    }
    /// Set or clear italic.
    pub fn with_italic(mut self, italic: bool) -> Self {
        self.italic = Some(italic);
        self
    }
    /// Set or clear strikethrough.
    pub fn with_strike(mut self, strike: bool) -> Self {
        self.strike = Some(strike);
        self
    }
    /// Set the underline.
    pub fn with_underline(mut self, underline: Underline) -> Self {
        self.underline = Some(underline);
        self
    }
    /// Set the color.
    pub fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
}

impl Fill {
    /// A two-color pattern fill.
    pub fn pattern(pattern: FillPattern, foreground: Color, background: Color) -> Self {
        Self::Pattern(PatternFill {
            pattern: Some(pattern),
            foreground: Some(foreground),
            background: Some(background),
        })
    }
}

/// Positions in [`Border::sides`].
const LEFT: usize = 0;
const RIGHT: usize = 1;
const TOP: usize = 2;
const BOTTOM: usize = 3;

impl BorderSide {
    /// A line of one style and color.
    pub fn new(line: BorderLine, color: Color) -> Self {
        Self {
            line: Some(line),
            color: Some(color),
        }
    }
}

impl Border {
    /// No border lines.
    pub fn new() -> Self {
        Self::default()
    }
    /// The same line on all four outer edges.
    pub fn all(side: BorderSide) -> Self {
        Self::new()
            .with_left(side.clone())
            .with_right(side.clone())
            .with_top(side.clone())
            .with_bottom(side)
    }
    /// Set the left edge.
    pub fn with_left(mut self, side: BorderSide) -> Self {
        self.sides[LEFT] = Some(side);
        self
    }
    /// Set the right edge.
    pub fn with_right(mut self, side: BorderSide) -> Self {
        self.sides[RIGHT] = Some(side);
        self
    }
    /// Set the top edge.
    pub fn with_top(mut self, side: BorderSide) -> Self {
        self.sides[TOP] = Some(side);
        self
    }
    /// Set the bottom edge.
    pub fn with_bottom(mut self, side: BorderSide) -> Self {
        self.sides[BOTTOM] = Some(side);
        self
    }
    /// The left edge.
    pub fn left(&self) -> Option<&BorderSide> {
        self.sides[LEFT].as_ref()
    }
    /// The right edge.
    pub fn right(&self) -> Option<&BorderSide> {
        self.sides[RIGHT].as_ref()
    }
    /// The top edge.
    pub fn top(&self) -> Option<&BorderSide> {
        self.sides[TOP].as_ref()
    }
    /// The bottom edge.
    pub fn bottom(&self) -> Option<&BorderSide> {
        self.sides[BOTTOM].as_ref()
    }
}

impl Alignment {
    /// No alignment overrides.
    pub fn new() -> Self {
        Self::default()
    }
    /// Set the horizontal alignment.
    pub fn with_horizontal(mut self, horizontal: HorizontalAlignment) -> Self {
        self.horizontal = Some(horizontal);
        self
    }
    /// Set the vertical alignment.
    pub fn with_vertical(mut self, vertical: VerticalAlignment) -> Self {
        self.vertical = Some(vertical);
        self
    }
    /// Wrap long text onto several lines.
    pub fn with_wrap_text(mut self, wrap: bool) -> Self {
        self.wrap_text = Some(wrap);
        self
    }
    /// Shrink text to fit the cell width.
    pub fn with_shrink_to_fit(mut self, shrink: bool) -> Self {
        self.shrink_to_fit = Some(shrink);
        self
    }
    /// Set the indent level.
    pub fn with_indent(mut self, indent: u8) -> Self {
        self.indent = Some(f64::from(indent));
        self
    }
    /// Rotate text: 0 to 90 degrees counterclockwise, 91 to 180 for 1 to 90
    /// degrees clockwise, or 255 for vertical text.
    pub fn with_rotation(mut self, rotation: u8) -> Self {
        self.rotation = Some(rotation);
        self
    }
}

impl Protection {
    /// Cell protection flags. They take effect when the sheet is protected.
    pub fn new(locked: bool, hidden: bool) -> Self {
        Self {
            locked: Some(locked),
            hidden: Some(hidden),
        }
    }
}

impl CellStyle {
    /// Set the number format code, such as `"0.00"` or `"yyyy-mm-dd"`.
    pub fn with_number_format(mut self, code: impl Into<Box<str>>) -> Self {
        self.number_format = code.into();
        self
    }
    /// Set the font.
    pub fn with_font(mut self, font: Font) -> Self {
        self.font = merge_font(&CellStyle::default().font, font);
        self
    }
    /// Set the fill.
    pub fn with_fill(mut self, fill: Fill) -> Self {
        self.fill = fill;
        self
    }
    /// Set the border.
    pub fn with_border(mut self, border: Border) -> Self {
        self.borders = border;
        self
    }
    /// Set the alignment.
    pub fn with_alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self
    }
    /// Set the protection flags.
    pub fn with_protection(mut self, protection: Protection) -> Self {
        self.protection = protection;
        self
    }
}

/// Attributes the caller did not set come from `base`, so that a font such
/// as `Font::new().with_bold(true)` keeps the workbook's typeface and size.
fn merge_font(base: &Font, font: Font) -> Font {
    let named = font.name.is_some();
    Font {
        name: font.name.or_else(|| base.name.clone()),
        size: font.size.or(base.size),
        family: if named {
            font.family
        } else {
            font.family.or(base.family)
        },
        scheme: if named {
            font.scheme
        } else {
            font.scheme.or(base.scheme)
        },
        color: font.color.or_else(|| base.color.clone()),
        ..font
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn builders_compose_and_inherit_font_defaults() {
        let style = CellStyle::default()
            .with_font(Font::new().with_bold(true))
            .with_fill(Fill::solid(Color::hex("#1F4E79").unwrap()))
            .with_border(Border::all(BorderSide::new(
                BorderLine::Thin,
                Color::rgb(0, 0, 0),
            )))
            .with_number_format("0.00");
        assert_eq!(style.font.bold, Some(true));
        assert_eq!(style.font.name.as_deref(), Some("Calibri"));
        assert_eq!(style.font.size, Some(11.0));
        assert_eq!(
            Color::hex("1F4E79").unwrap().to_rgb(),
            Some((0x1F, 0x4E, 0x79))
        );
        assert!(Color::hex("12345").is_err());
        assert!(Color::theme(12, 0.0).is_err());
        assert_eq!(style.borders.left().unwrap().line, Some(BorderLine::Thin));
        assert_eq!(&*style.number_format, "0.00");
    }
}
