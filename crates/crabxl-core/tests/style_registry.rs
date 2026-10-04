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
    assert_eq!(style.font.family, Some(2.0));
    assert_eq!(style.number_format, Some("General"));
}

#[test]
fn public_style_domains_and_literal_color_casing_are_retained() {
    use crabxl_core::{ArgbLiteral, Font};
    let rgb = ArgbLiteral::parse("aAbBcC").unwrap();
    assert_eq!(rgb.to_string(), "00aAbBcC");
    assert_eq!(rgb.channels(), 0x00AABBCC);
    assert_eq!(
        ArgbLiteral::parse("AABBCC").unwrap().into_kind(),
        ColorKind::Argb(0x00AABBCC)
    );
    for invalid in ["abc", "GG001100", "123456789"] {
        assert!(ArgbLiteral::parse(invalid).is_err());
    }
    let mut registry = StyleRegistry::new(StyleLimits::default()).unwrap();
    let style = CellStyle {
        font: Font {
            family: Some(2.5),
            charset: Some(-1),
            color: Some(Color {
                kind: rgb.into_kind(),
                tint: None,
            }),
            ..Font::default()
        },
        ..CellStyle::default()
    };
    let id = registry.register(style.clone()).unwrap();
    assert_eq!(registry.register(style).unwrap(), id);
    let view = registry.catalog().cell_style(id).unwrap();
    assert_eq!(view.font.family, Some(2.5));
    assert_eq!(view.font.charset, Some(-1));
    for value in [-1.0, 14.5, f64::NAN, f64::INFINITY] {
        assert!(
            Font {
                family: Some(value),
                ..Font::default()
            }
            .validate()
            .is_err()
        );
    }
}

#[test]
fn imported_catalogs_preserve_ids_duplicates_and_sparse_number_format_identity() {
    use crabxl_core::{NumberFormat, StyleId};
    let mut original = StyleRegistry::new(StyleLimits::default()).unwrap();
    let existing = original
        .register(CellStyle {
            number_format: "0.000".into(),
            ..Default::default()
        })
        .unwrap();
    let mut catalog = original.catalog().clone();
    catalog.fonts.push(catalog.fonts[0].clone());
    catalog.cell_formats.push(catalog.cell_formats[0].clone());
    let duplicate_id = catalog.cell_formats.len() - 1;
    catalog.cell_formats[duplicate_id].font_id = 1;
    catalog
        .number_formats
        .push(NumberFormat::new(u32::MAX, "0.00000"));
    catalog
        .number_formats
        .push(NumberFormat::new(165, "0.0000"));
    let mut registry =
        StyleRegistry::from_catalog(catalog.clone(), StyleLimits::default()).unwrap();
    assert_eq!(registry.catalog().cell_formats, catalog.cell_formats);
    assert_eq!(registry.catalog().fonts, catalog.fonts);
    assert_eq!(
        registry
            .catalog()
            .cell_style(StyleId::new(duplicate_id as u32))
            .unwrap()
            .font,
        &catalog.fonts[1]
    );
    assert_eq!(
        registry
            .register(CellStyle {
                number_format: "0.000".into(),
                ..Default::default()
            })
            .unwrap(),
        existing
    );
    let id = registry
        .register(CellStyle {
            number_format: "0.000000".into(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        registry.catalog().cell_formats[id.get() as usize].number_format_id,
        166
    );
    assert_eq!(registry.catalog().number_format(u32::MAX), Some("0.00000"));
    assert_eq!(registry.catalog().fonts.len(), 2);
}

#[test]
fn imported_builtin_overrides_do_not_change_new_literal_format_meaning() {
    use crabxl_core::NumberFormat;
    let mut catalog = StyleRegistry::new(StyleLimits::default())
        .unwrap()
        .catalog()
        .clone();
    catalog.number_formats.push(NumberFormat::new(14, "0.000"));
    let mut registry = StyleRegistry::from_catalog(catalog, StyleLimits::default()).unwrap();
    let id = registry
        .register(CellStyle {
            number_format: "mm-dd-yy".into(),
            ..Default::default()
        })
        .unwrap();
    let format = &registry.catalog().cell_formats[id.get() as usize];
    assert_ne!(format.number_format_id, 14);
    assert_eq!(
        registry.catalog().number_format(format.number_format_id),
        Some("mm-dd-yy")
    );
    assert_eq!(registry.catalog().number_format(14), Some("0.000"));
}

#[test]
fn imported_catalogs_reject_missing_links_duplicates_and_index_budget_overflow() {
    use crabxl_core::{ErrorKind, NumberFormat};
    let source = StyleRegistry::new(StyleLimits::default())
        .unwrap()
        .catalog()
        .clone();
    let mut invalid = source.clone();
    invalid.cell_formats[0].font_id = 99;
    assert!(
        matches!(StyleRegistry::from_catalog(invalid, StyleLimits::default()), Err(e) if e.kind() == ErrorKind::InvalidData)
    );
    let mut duplicate = source.clone();
    duplicate.number_formats = vec![
        NumberFormat::new(164, "0.00"),
        NumberFormat::new(164, "0.000"),
    ];
    assert!(
        matches!(StyleRegistry::from_catalog(duplicate, StyleLimits::default()), Err(e) if e.kind() == ErrorKind::InvalidData)
    );
    let imported = StyleRegistry::from_catalog(source.clone(), StyleLimits::default()).unwrap();
    let limits = StyleLimits {
        max_bytes: imported.memory_bytes() - 1,
        ..Default::default()
    };
    assert!(StyleRegistry::from_catalog(source, limits).is_err());
}

#[test]
fn raw_format_edits_reuse_components_and_retain_absence_and_application_flags() {
    let source = StyleRegistry::new(StyleLimits::default())
        .unwrap()
        .catalog()
        .clone();
    let mut registry = StyleRegistry::from_catalog(source, StyleLimits::default()).unwrap();
    let mut format = registry.catalog().cell_formats[0].clone();
    format.number_format_id = 14;
    format.apply_number_format = None;
    format.alignment = None;
    format.protection = None;
    let id = registry.register_format(format.clone()).unwrap();
    assert_eq!(registry.register_format(format.clone()).unwrap(), id);
    assert_eq!(registry.catalog().cell_formats[id.get() as usize], format);
    assert_eq!(registry.catalog().fonts.len(), 1);
    assert_eq!(registry.catalog().fills.len(), 2);
    let count = registry.catalog().cell_formats.len();
    format.font_id = 99;
    assert!(registry.register_format(format).is_err());
    assert_eq!(registry.catalog().cell_formats.len(), count);
}
