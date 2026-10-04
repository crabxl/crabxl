//! Shared generated style combinations for streaming and owned-bank comparisons.
use crabxl::{CellStyle, Color, ColorKind, Fill};
pub(crate) fn style(index: u32) -> CellStyle {
    let mut style = CellStyle {
        number_format: "0.000".into(),
        ..Default::default()
    };
    style.font.family = Some(2.0);
    style.font.scheme = Some(crabxl::FontScheme::Minor);
    style.alignment.horizontal = Some(crabxl::HorizontalAlignment::General);
    style.alignment.vertical = Some(crabxl::VerticalAlignment::Bottom);
    style.font.color = Some(Color {
        kind: ColorKind::Argb(0xFF000000 | (index % 16)),
        tint: None,
    });
    style.fill = Fill::solid(Color {
        kind: ColorKind::Argb(0xFF100000 | ((index / 16) % 16)),
        tint: None,
    });
    style.alignment.wrap_text = Some(true);
    style.alignment.shrink_to_fit = Some(true);
    style.alignment.rotation = Some(((index / 256) % 32) as u8);
    style
}
