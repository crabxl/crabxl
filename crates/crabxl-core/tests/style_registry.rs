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
    assert_eq!(
        registry
            .named_style_format_with_limit("Normal", usize::MAX)
            .unwrap()
            .get(),
        0
    );
    let named = registry
        .register_named_style_with_limit(
            "Percent named".into(),
            percent,
            crabxl_core::NamedStyleOptions {
                hidden: Some(true),
                ..Default::default()
            },
            usize::MAX,
        )
        .unwrap();
    let metadata = registry.named_style("Percent named").unwrap();
    assert_eq!(metadata.hidden, Some(true));
    assert_eq!(
        registry
            .catalog()
            .cell_format(named)
            .unwrap()
            .base_format_id,
        Some(metadata.base_format_id)
    );
    assert_eq!(
        registry.catalog().cell_style(named).unwrap().number_format,
        Some("0%")
    );
    let bytes = registry.memory_bytes();
    for _ in 0..1000 {
        assert_eq!(
            registry
                .named_style_format_with_limit("Percent named", bytes)
                .unwrap(),
            named
        );
    }
    assert_eq!(registry.memory_bytes(), bytes);
    let names = registry.catalog().named_styles.len();
    let bases = registry.catalog().base_formats.len();
    assert!(
        registry
            .register_named_style_with_limit(
                "Percent named".into(),
                percent,
                Default::default(),
                usize::MAX
            )
            .is_err()
    );
    assert!(
        registry
            .register_named_style_with_limit(
                "Insufficient".into(),
                percent,
                Default::default(),
                bytes
            )
            .is_err()
    );
    assert_eq!(registry.catalog().named_styles.len(), names);
    assert_eq!(registry.catalog().base_formats.len(), bases);
    assert!(registry.named_style("Insufficient").is_none());
    let reopened =
        StyleRegistry::from_catalog(registry.catalog().clone(), StyleLimits::default()).unwrap();
    assert_eq!(
        reopened
            .named_style("Percent named")
            .unwrap()
            .base_format_id,
        1
    );
    let replacement = registry
        .update_named_style_with_limit("Percent named", id, usize::MAX)
        .unwrap();
    assert_eq!(
        registry.catalog().cell_style(named).unwrap().number_format,
        Some("0%")
    );
    assert_eq!(
        registry
            .catalog()
            .cell_style(replacement)
            .unwrap()
            .number_format,
        Some("0.000")
    );
    registry
        .update_named_metadata_with_limit(
            "Percent named",
            "Decimal named".into(),
            Default::default(),
            usize::MAX,
        )
        .unwrap();
    assert!(registry.named_style("Percent named").is_none());
    assert!(registry.named_style("Decimal named").is_some());
    assert_eq!(
        registry
            .named_style_format_with_limit("Decimal named", usize::MAX)
            .unwrap(),
        replacement
    );
    let unchanged = registry.memory_bytes();
    assert!(
        registry
            .update_named_metadata_with_limit(
                "Decimal named",
                "Normal".into(),
                Default::default(),
                usize::MAX
            )
            .is_err()
    );
    assert_eq!(registry.memory_bytes(), unchanged);
    assert!(
        registry
            .update_named_metadata_with_limit(
                "Decimal named",
                "Much longer replacement name".into(),
                Default::default(),
                unchanged
            )
            .is_err()
    );
    assert!(registry.named_style("Decimal named").is_some());
}
#[test]
fn signed_zero_hashes_follow_numeric_equality_and_optional_identity() {
    let mut registry = StyleRegistry::new(StyleLimits::default()).unwrap();
    let mut first = CellStyle::default();
    first.font.size = Some(0.0);
    first.font.color = Some(Color {
        kind: ColorKind::Theme(2.into()),
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
            charset: Some((-1).into()),
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
    assert_eq!(view.font.charset, Some((-1).into()));
    use crabxl_core::StyleInteger;
    for literal in [
        "9223372036854775808",
        "-9223372036854775809",
        "10000000000000000000000000000000000000000",
    ] {
        let integer = StyleInteger::parse(literal).unwrap();
        assert_eq!(integer.to_string(), literal);
        assert!(integer.as_i64().is_none());
        assert!(integer.heap_bytes() > literal.len());
        let color = Color {
            kind: ColorKind::Theme(integer.clone()),
            tint: None,
        };
        let mut style = CellStyle::default();
        style.font.charset = Some(integer);
        style.font.color = Some(color.clone());
        style.fill = Fill::solid(color.clone());
        style.borders.sides[0].as_mut().unwrap().color = Some(color);
        let before = registry.memory_bytes();
        let id = registry.register(style.clone()).unwrap();
        assert!(registry.memory_bytes() >= before + 4 * literal.len());
        let retained = registry.memory_bytes();
        assert_eq!(registry.register(style).unwrap(), id);
        assert_eq!(registry.memory_bytes(), retained);
        let adopted =
            StyleRegistry::from_catalog(registry.catalog().clone(), StyleLimits::default())
                .unwrap();
        assert_eq!(
            adopted.catalog().cell_style(id).unwrap().font,
            registry.catalog().cell_style(id).unwrap().font
        );
    }
    let exact = StyleInteger::parse(&"9".repeat(2048)).unwrap();
    let mut bounded = StyleRegistry::new(StyleLimits::default()).unwrap();
    let before = bounded.catalog().clone();
    let mut oversized = CellStyle::default();
    oversized.font.charset = Some(exact.clone());
    let allowance = bounded.memory_bytes() + 64;
    assert_eq!(
        bounded
            .register_with_limit(oversized, allowance)
            .unwrap_err()
            .kind(),
        crabxl_core::ErrorKind::MemoryBudgetExceeded
    );
    assert_eq!(bounded.catalog().fonts, before.fonts);
    assert_eq!(bounded.catalog().cell_formats, before.cell_formats);
    assert_eq!(bounded.register(CellStyle::default()).unwrap().get(), 0);
    let color = Color {
        kind: ColorKind::Indexed(exact),
        tint: None,
    };
    let mut catalog = registry.catalog().clone();
    let initial = catalog.memory_bytes();
    catalog.recent_colors.push(color.clone());
    catalog
        .differential_styles
        .push(crabxl_core::DifferentialStyle {
            fill: Some(Box::new(Fill::Gradient(crabxl_core::GradientFill {
                stops: vec![crabxl_core::GradientStop {
                    position: 0.0,
                    color: color.clone(),
                }],
                ..Default::default()
            }))),
            border: Some(Box::new({
                let mut border = crabxl_core::Border::default();
                border.sides[0].as_mut().unwrap().color = Some(color.clone());
                border
            })),
            ..Default::default()
        });
    assert!(catalog.memory_bytes() >= initial + 3 * color.heap_bytes());
    let adopted = StyleRegistry::from_catalog(catalog.clone(), StyleLimits::default()).unwrap();
    assert!(adopted.memory_bytes() >= catalog.memory_bytes());
    assert!(
        StyleRegistry::from_catalog(
            catalog,
            StyleLimits {
                max_bytes: initial + color.heap_bytes(),
                ..Default::default()
            }
        )
        .is_err()
    );
    for (literal, value) in [
        ("+0001", 1),
        ("-0000", 0),
        ("00000000000000000000000000000000000000000000001", 1),
    ] {
        let integer = StyleInteger::parse(literal).unwrap();
        assert_eq!(integer, value.into());
        assert_eq!(integer.heap_bytes(), 0);
    }
    for invalid in ["", "--1", "1e40", "1.0", "１２", "1 2"] {
        assert!(StyleInteger::parse(invalid).is_err());
    }
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

#[test]
fn inserted_custom_codes_keep_sparse_declarations_sorted_and_hash_indices_valid() {
    use crabxl_core::NumberFormat;
    let mut catalog = StyleRegistry::new(StyleLimits::default())
        .unwrap()
        .catalog()
        .clone();
    catalog.number_formats = vec![
        NumberFormat::new(500, "0.00000"),
        NumberFormat::new(u32::MAX, "0.000000"),
    ];
    let mut styles = StyleRegistry::from_catalog(catalog, StyleLimits::default()).unwrap();
    assert_eq!(styles.register_number_format("0.000".into()).unwrap(), 164);
    assert_eq!(styles.register_number_format("0.0000".into()).unwrap(), 165);
    assert_eq!(
        styles.register_number_format("0.00000".into()).unwrap(),
        500
    );
    assert_eq!(
        styles.register_number_format("0.000000".into()).unwrap(),
        u32::MAX
    );
    assert!(
        styles
            .catalog()
            .number_formats
            .windows(2)
            .all(|p| p[0].id() < p[1].id())
    );
    let id = styles
        .register(CellStyle {
            number_format: "0.000000".into(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        styles.catalog().cell_formats[id.get() as usize].number_format_id,
        u32::MAX
    );
}

#[test]
fn finite_font_sizes_gradient_edges_and_empty_formats_follow_public_domains() {
    use crabxl_core::{GradientFill, GradientKind};
    let mut styles = StyleRegistry::new(StyleLimits::default()).unwrap();
    for size in [-1.0, 410.0, 1_000_000.0] {
        let mut style = CellStyle::default();
        style.font.size = Some(size);
        style.number_format = "".into();
        style.fill = Fill::Gradient(GradientFill {
            kind: Some(GradientKind::Path),
            degree: Some(-360.0),
            edges: [Some(-1.0), Some(2.0), Some(0.0), Some(0.0)],
            stops: Vec::new(),
        });
        let id = styles.register(style).unwrap();
        let view = styles.catalog().cell_style(id).unwrap();
        assert_eq!(view.font.size, Some(size));
        assert_eq!(view.number_format, Some(""));
        assert!(
            matches!(view.fill, Fill::Gradient(value) if value.edges[0] == Some(-1.0) && value.edges[1] == Some(2.0))
        );
    }
    let mut invalid = CellStyle::default();
    invalid.font.size = Some(f64::NAN);
    assert!(styles.register(invalid).is_err());
}

#[test]
fn borrowed_number_format_variants_share_components_and_preserve_source_properties() {
    use crabxl_core::{Alignment, CellFormat, StyleId};
    let mut registry = StyleRegistry::new(StyleLimits::default()).unwrap();
    let mut source = registry
        .catalog()
        .cell_format(StyleId::new(0))
        .unwrap()
        .clone();
    source.alignment = Some(Box::new(Alignment {
        indent: Some(2.5),
        ..Default::default()
    }));
    source.quote_prefix = Some(true);
    let base = registry.register_format(source.clone()).unwrap();
    let number = registry
        .register_number_format("yyyy-mm-dd h:mm:ss".into())
        .unwrap();
    let before = registry.catalog().fonts[0]
        .name
        .as_ref()
        .map(|v| v.as_ptr());
    let variant = registry
        .register_format_with_number_format(base, number)
        .unwrap();
    let format = registry.catalog().cell_format(variant).unwrap();
    assert_eq!(format.font_id, source.font_id);
    assert_eq!(format.fill_id, source.fill_id);
    assert_eq!(format.border_id, source.border_id);
    assert_eq!(format.alignment, source.alignment);
    assert_eq!(format.quote_prefix, Some(true));
    assert_eq!(format.number_format_id, number);
    assert_eq!(format.apply_number_format, Some(true));
    assert_eq!(
        registry.catalog().fonts[0]
            .name
            .as_ref()
            .map(|v| v.as_ptr()),
        before
    );
    let bytes = registry.memory_bytes();
    let records = registry.catalog().cell_formats.len();
    for _ in 0..1000 {
        assert_eq!(
            registry
                .find_format_with_number_format(base, number)
                .unwrap(),
            Some(variant)
        );
        assert_eq!(
            registry
                .register_format_with_number_format_limit(base, number, bytes)
                .unwrap(),
            variant
        );
    }
    assert_eq!(registry.memory_bytes(), bytes);
    assert_eq!(registry.catalog().cell_formats.len(), records);
    assert!(
        registry
            .register_format_with_number_format_limit(base, 14, bytes)
            .is_err()
    );
    assert_eq!(registry.catalog().cell_formats.len(), records);
    assert!(
        registry
            .find_format_with_number_format(StyleId::new(u32::MAX), number)
            .is_err()
    );
    assert!(
        registry
            .find_format_with_number_format(base, u32::MAX)
            .is_err()
    );
    let mut expected: CellFormat = source;
    expected.number_format_id = number;
    expected.apply_number_format = Some(true);
    assert_eq!(registry.register_format(expected).unwrap(), variant);
    let large_font = crabxl_core::Font {
        name: Some("x".repeat(1024 * 1024).into()),
        ..Default::default()
    };
    let large = registry
        .derive_component_with_limit(
            variant,
            crabxl_core::StyleComponent::Font(Box::new(large_font)),
            usize::MAX,
        )
        .unwrap();
    let font_id = registry.catalog().cell_format(large).unwrap().font_id;
    let pointer = registry.catalog().fonts[font_id as usize]
        .name
        .as_ref()
        .unwrap()
        .as_ptr();
    let aligned = registry
        .derive_component_with_limit(
            large,
            crabxl_core::StyleComponent::Alignment(Some(Box::new(Alignment {
                wrap_text: Some(true),
                ..Default::default()
            }))),
            usize::MAX,
        )
        .unwrap();
    let derived = registry.catalog().cell_format(aligned).unwrap();
    assert_eq!(derived.font_id, font_id);
    assert_eq!(derived.number_format_id, number);
    assert_eq!(derived.quote_prefix, Some(true));
    assert_eq!(
        registry.catalog().fonts[font_id as usize]
            .name
            .as_ref()
            .unwrap()
            .as_ptr(),
        pointer
    );
    let bytes = registry.memory_bytes();
    let records = registry.catalog().cell_formats.len();
    assert!(
        registry
            .derive_component_with_limit(
                StyleId::new(u32::MAX),
                crabxl_core::StyleComponent::Protection(None),
                usize::MAX,
            )
            .is_err()
    );
    assert!(
        registry
            .derive_component_with_limit(
                aligned,
                crabxl_core::StyleComponent::Alignment(Some(Box::new(Alignment {
                    rotation: Some(254),
                    ..Default::default()
                }))),
                usize::MAX,
            )
            .is_err()
    );
    assert!(
        registry
            .derive_component_with_limit(
                aligned,
                crabxl_core::StyleComponent::Protection(Some(crabxl_core::Protection {
                    locked: Some(false),
                    hidden: Some(true),
                })),
                bytes,
            )
            .is_err()
    );
    assert_eq!(registry.memory_bytes(), bytes);
    assert_eq!(registry.catalog().cell_formats.len(), records);
}

#[test]
fn palette_literal_payloads_are_accounted_during_source_adoption() {
    use crabxl_core::ArgbLiteral;
    let source = StyleRegistry::new(StyleLimits::default()).unwrap();
    let mut catalog = source.catalog().clone();
    let base = StyleRegistry::from_catalog(catalog.clone(), StyleLimits::default())
        .unwrap()
        .memory_bytes();
    catalog.indexed_colors = vec![ArgbLiteral::parse("ff11aa22").unwrap(); 512];
    assert!(
        StyleRegistry::from_catalog(
            catalog.clone(),
            StyleLimits {
                max_bytes: base + 512 * 4,
                max_records: 1000
            }
        )
        .is_err()
    );
    let registry = StyleRegistry::from_catalog(catalog, StyleLimits::default()).unwrap();
    assert_eq!(registry.catalog().indexed_colors[0].to_string(), "ff11aa22");
    assert_eq!(
        ArgbLiteral::from(0x00aabbcc),
        ArgbLiteral::parse("00AABBCC").unwrap()
    );
}
