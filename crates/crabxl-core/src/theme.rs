//! Owned opaque theme data shared by format readers, writers and future bindings.
use std::sync::Arc;
/// Exact theme-part bytes, including unknown drawing sections and typeface names.
/// This core container does not interpret or validate an XML serialization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Theme {
    bytes: Arc<[u8]>,
}
impl Theme {
    /// Transfer a theme buffer into immutable shared ownership.
    /// Conversion may transiently retain the input buffer while allocating the shared block.
    pub fn from_bytes(bytes: Box<[u8]>) -> Self {
        Self {
            bytes: bytes.into(),
        }
    }
    /// Borrow the original serialization without cloning.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Conservative full payload/container/reference-count charge per holder.
    /// Clones share bytes without copying; allocator overhead is additional.
    pub fn memory_bytes(&self) -> usize {
        size_of::<Self>()
            .saturating_add(2 * size_of::<usize>())
            .saturating_add(self.bytes.len())
    }
}

/// A DrawingML palette color, preserving system-color identity separately from
/// its portable last-known RGB value. Color transforms require explicit support.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThemeColor {
    /// Six-digit RGB spelling, retained without changing hexadecimal case.
    Rgb(crate::ArgbLiteral),
    /// Named system color and optional last-known RGB fallback.
    System {
        /// System color token.
        name: Box<str>,
        /// Last-known RGB supplied by the document, not queried from the host.
        last_color: Option<crate::ArgbLiteral>,
    },
}
impl ThemeColor {
    /// RGB value without tint or DrawingML transforms. A system color without a
    /// last-known value cannot be resolved portably.
    pub fn rgb(&self) -> Option<u32> {
        match self {
            Self::Rgb(value) => Some(value.channels()),
            Self::System { last_color, .. } => last_color.map(crate::ArgbLiteral::channels),
        }
    }
    fn heap_bytes(&self) -> usize {
        match self {
            Self::System { name, .. } => name.len(),
            Self::Rgb(_) => 0,
        }
    }
}

/// Typeface properties in a theme font collection. Empty typeface names are
/// meaningful and remain distinct from absent elements.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThemeTypeface {
    /// Typeface name.
    pub typeface: Box<str>,
    /// Optional PANOSE classification spelling.
    pub panose: Option<Box<str>>,
    /// Optional pitch/family byte.
    pub pitch_family: Option<u8>,
    /// Optional character-set byte.
    pub charset: Option<u8>,
}
impl ThemeTypeface {
    fn heap_bytes(&self) -> usize {
        self.typeface.len() + self.panose.as_ref().map_or(0, |value| value.len())
    }
}

/// Supplemental typeface for a script, in document order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeScriptFont {
    /// Script identity such as Hant or Arab.
    pub script: Box<str>,
    /// Typeface name.
    pub typeface: Box<str>,
}

/// Major or minor theme font collection.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThemeFontCollection {
    /// Latin typeface.
    pub latin: Option<ThemeTypeface>,
    /// East Asian typeface.
    pub east_asian: Option<ThemeTypeface>,
    /// Complex-script typeface.
    pub complex_script: Option<ThemeTypeface>,
    /// Supplemental script mappings, retaining source order.
    pub supplemental: Vec<ThemeScriptFont>,
}
impl ThemeFontCollection {
    fn heap_bytes(&self) -> usize {
        [&self.latin, &self.east_asian, &self.complex_script]
            .into_iter()
            .flatten()
            .map(ThemeTypeface::heap_bytes)
            .sum::<usize>()
            .saturating_add(self.supplemental.capacity() * size_of::<ThemeScriptFont>())
            .saturating_add(
                self.supplemental
                    .iter()
                    .map(|font| font.script.len() + font.typeface.len())
                    .sum::<usize>(),
            )
    }
}

/// Typed read-side palette and font scheme. Original opaque theme bytes remain
/// separately available; this is not a drawing/effect graph or editing model.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThemeCatalog {
    /// Theme name.
    pub name: Option<Box<str>>,
    /// Palette name.
    pub color_scheme_name: Option<Box<str>>,
    /// Font scheme name.
    pub font_scheme_name: Option<Box<str>>,
    /// Spreadsheet theme-index order: lt1, dk1, lt2, dk2, accent1..6,
    /// hyperlink, followed hyperlink. This differs from XML element order.
    pub colors: [Option<ThemeColor>; 12],
    /// Major font collection.
    pub major_fonts: ThemeFontCollection,
    /// Minor font collection.
    pub minor_fonts: ThemeFontCollection,
}
impl ThemeCatalog {
    /// Actual owned payload and container charge; allocator overhead is additional.
    pub fn memory_bytes(&self) -> usize {
        size_of::<Self>()
            .saturating_add(
                [&self.name, &self.color_scheme_name, &self.font_scheme_name]
                    .into_iter()
                    .flatten()
                    .map(|name| name.len())
                    .sum::<usize>(),
            )
            .saturating_add(
                self.colors
                    .iter()
                    .flatten()
                    .map(ThemeColor::heap_bytes)
                    .sum::<usize>(),
            )
            .saturating_add(self.major_fonts.heap_bytes())
            .saturating_add(self.minor_fonts.heap_bytes())
    }
}
