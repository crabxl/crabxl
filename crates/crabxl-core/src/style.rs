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

/// Shared optional font properties, identical to rich-run overrides.
/// Missing properties retain their identity rather than installing defaults.
pub type Font = crate::RunFont;
/// Complete appearance components for registering a new shared cell format.
/// Imported component/base-format IDs and named-style metadata live in StyleCatalog.
#[derive(Clone, Debug, PartialEq, Hash)]
pub struct CellStyle {
    /// Number-format code (General by default).
    pub number_format: Box<str>,
    /// Font attributes.
    pub font: Font,
    /// Complete pattern/gradient fill.
    pub fill: crate::Fill,
    /// Complete border edges and diagonal settings.
    pub borders: crate::Border,
    /// Optional alignment overrides.
    pub alignment: crate::Alignment,
    /// Optional protection overrides.
    pub protection: crate::Protection,
}
impl Default for CellStyle {
    fn default() -> Self {
        Self {
            number_format: "General".into(),
            font: Font {
                name: Some("Calibri".into()),
                size: Some(11.0),
                family: Some(2.0),
                scheme: Some(crate::FontScheme::Minor),
                color: Some(crate::Color {
                    kind: crate::ColorKind::Theme(1),
                    tint: None,
                }),
                ..Default::default()
            },
            fill: crate::Fill::default(),
            borders: crate::Border::default(),
            alignment: crate::Alignment::default(),
            protection: crate::Protection {
                locked: Some(true),
                hidden: Some(false),
            },
        }
    }
}
impl CellStyle {
    /// Payload bytes owned by the shared format, excluding allocator overhead.
    pub fn heap_bytes(&self) -> usize {
        self.number_format.len()
            + self.font.name.as_ref().map_or(0, |name| name.len())
            + self.fill.heap_bytes()
    }
}
