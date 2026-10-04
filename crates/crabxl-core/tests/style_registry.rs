//! Canonical style component sharing, hash equality and failed registrations.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use crabxl_core::{CellStyle, Color, ColorKind, Fill, StyleLimits, StyleRegistry};
#[test]
fn independent_styles_share_components_and_exact_builtin_codes() {
    let mut registry = StyleRegistry::new(StyleLimits::default()).unwrap();
    assert_eq!(registry.catalog().fonts.len(), 1);
    assert_eq!(registry.catalog().fills.len(), 2);
    assert_eq!(registry.catalog().borders.len(), 1);
    let custom = CellStyle {
        number_format: "0.000".into(),
        ..Default::default()
    };
    let id = registry.register(custom.clone()).unwrap();
    assert_eq!(id.get(), 1);
    assert_eq!(registry.register(custom).unwrap(), id);
    let percent = registry
        .register(CellStyle {
            number_format: "0%".into(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        registry
            .catalog()
            .cell_format(percent)
            .unwrap()
            .number_format_id,
        9
    );
    assert_eq!(registry.catalog().number_formats.len(), 1);
    assert_eq!(registry.catalog().fonts.len(), 1);
    assert_eq!(registry.catalog().borders.len(), 1);
    assert_eq!(registry.catalog().cell_formats.len(), 3);
    let first = registry.catalog().cell_style(id).unwrap();
    let second = registry.catalog().cell_style(percent).unwrap();
    assert!(std::ptr::eq(first.font, second.font));
    assert_eq!(first.number_format, Some("0.000"));
}
#[test]
fn signed_zero_hashes_follow_numeric_equality_and_optional_identity() {
    let mut registry = StyleRegistry::new(StyleLimits::default()).unwrap();
    let mut first = CellStyle::default();
    first.font.size = Some(0.0);
    first.font.color = Some(Color {
        kind: ColorKind::Theme(2),
        tint: Some(0.0),
    });
    first.alignment.indent = Some(0.0);
    let id = registry.register(first.clone()).unwrap();
    first.font.size = Some(-0.0);
    first.font.color.as_mut().unwrap().tint = Some(-0.0);
    first.alignment.indent = Some(-0.0);
    assert_eq!(registry.register(first.clone()).unwrap(), id);
    first.font.color.as_mut().unwrap().tint = None;
    assert_ne!(registry.register(first).unwrap(), id);
}
#[test]
fn failed_registration_preserves_logical_tables_and_reusable_indices() {
    let mut registry = StyleRegistry::new(StyleLimits {
        max_records: 2,
        ..Default::default()
    })
    .unwrap();
    let before = registry.catalog().clone();
    let mut style = CellStyle::default();
    style.font.bold = Some(true);
    style.fill = Fill::solid(Color {
        kind: ColorKind::Argb(0xFF123456),
        tint: None,
    });
    assert!(registry.register(style).is_err());
    assert_eq!(registry.catalog(), &before);
    assert_eq!(
        registry
            .register(CellStyle {
                number_format: "0%".into(),
                ..Default::default()
            })
            .unwrap()
            .get(),
        1
    );
    let before = registry.catalog().clone();
    let bytes = registry.memory_bytes();
    assert!(
        registry
            .register_with_limit(
                CellStyle {
                    number_format: "0.0000".into(),
                    ..Default::default()
                },
                bytes + 1
            )
            .is_err()
    );
    assert_eq!(registry.catalog(), &before);
    assert!(registry.memory_bytes() <= StyleLimits::default().max_bytes);
}
#[test]
fn invalid_numeric_components_reject_before_interning() {
    let mut registry = StyleRegistry::new(StyleLimits::default()).unwrap();
    let before = registry.catalog().clone();
    let mut style = CellStyle::default();
    style.font.size = Some(f64::NAN);
    assert!(registry.register(style).is_err());
    assert_eq!(registry.catalog(), &before);
}

#[test]
fn tight_allowances_fall_back_to_small_capacity_growth_without_losing_defaults() {
    let wide = StyleRegistry::new(StyleLimits::default()).unwrap();
    let limit = wide.memory_bytes() - 1;
    let narrow = StyleRegistry::new(StyleLimits {
        max_bytes: limit,
        ..Default::default()
    })
    .unwrap();
    assert!(narrow.memory_bytes() <= limit);
    let style = narrow
        .catalog()
        .cell_style(crabxl_core::StyleId::new(0))
        .unwrap();
    assert_eq!(style.font.name.as_deref(), Some("Calibri"));
    assert_eq!(style.font.family, Some(2));
    assert_eq!(style.number_format, Some("General"));
}
