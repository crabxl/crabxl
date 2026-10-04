//! Generated shared date/formula boundary and payload tests.
#![allow(clippy::unwrap_used)]
use crabxl_core::{CellValue, DateEpoch::*, DateKind::*, ExcelDateTime as Date, Formula};
#[test]
fn date_boundaries_epochs_and_millisecond_rounding() {
    for (y, m, d, serial) in [
        (1899, 12, 30, 0.0),
        (1899, 12, 31, 0.0),
        (1900, 1, 1, 1.0),
        (1900, 2, 28, 59.0),
        (1900, 3, 1, 61.0),
        (1904, 1, 1, 1462.0),
        (2024, 2, 29, 45351.0),
    ] {
        let date = Date::from_ymd_hms_milli(y, m, d, 0, 0, 0, 0).unwrap();
        assert_eq!(date.serial(), serial);
        if y >= 1900 {
            assert_eq!(
                date.to_datetime().unwrap().date().to_string(),
                format!("{y:04}-{m:02}-{d:02}")
            );
        }
    }
    for hour in [0, 12, 23] {
        let win = Date::from_ymd_hms_milli(1900, 2, 28, hour, 30, 1, 123).unwrap();
        let mac = Date::from_serial(win.serial_in(Mac1904).unwrap(), Mac1904, DateTime).unwrap();
        assert!((mac.serial_in(Windows1900).unwrap() - win.serial()).abs() < 1e-10);
        assert_eq!(mac.to_datetime().unwrap(), win.to_datetime().unwrap());
    }
    let fictitious = Date::from_serial(60.5, Windows1900, DateTime).unwrap();
    assert_eq!(
        fictitious.to_datetime().unwrap().to_string(),
        "1900-02-28 12:00:00"
    );
    assert!(fictitious.serial_in(Mac1904).is_err());
    assert!(Date::from_ymd_hms_milli(1900, 2, 29, 0, 0, 0, 0).is_err());
    assert!(Date::from_ymd_hms_milli(2024, 1, 1, 0, 0, 60, 0).is_err());
    assert!(Date::from_serial(f64::NAN, Windows1900, DateTime).is_err());
    for serial in [f64::MAX, f64::MIN, i64::MIN as f64 / 86_400_000.0] {
        assert!(
            Date::from_serial(serial, Windows1900, DateTime)
                .unwrap()
                .to_datetime()
                .is_err()
        );
    }
    assert!(Date::from_serial(1.0, Windows1900, Time).is_err());
    for serial in [-1.5, 1.5] {
        let date = Date::from_serial(serial, Windows1900, Duration).unwrap();
        assert_eq!(date.serial_in(Mac1904).unwrap(), serial);
        assert!(date.to_datetime().is_err());
    }
}
#[test]
fn formulas_distinguish_missing_cache_and_typed_zero_and_count_payloads() {
    let formula = Formula::new("=SUM(A1:A2)", Some(CellValue::text(" cached "))).unwrap();
    assert_eq!(formula.expression(), "SUM(A1:A2)");
    let payload = CellValue::Formula(Box::new(formula));
    assert!(payload.heap_bytes() > "SUM(A1:A2) cached ".len());
    assert!(Formula::new("=", None).is_err());
    assert!(Formula::new("1", Some(payload)).is_err());
    assert_eq!(Formula::new("0", None).unwrap().cached(), None);
    assert_eq!(
        Formula::new("0", Some(CellValue::Integer(0)))
            .unwrap()
            .cached(),
        Some(&CellValue::Integer(0))
    );
    assert_eq!(size_of::<CellValue>(), 16);
}

#[test]
fn distant_calendar_rounding_uses_fractional_day_precision() {
    // Public openpyxl 3.1.5 from_excel probe: absolute-day scaling rounds this
    // source to two milliseconds, while the baseline rounds its fraction to one.
    let date = Date::from_serial(2958465.000000017, Windows1900, DateTime).unwrap();
    assert_eq!(
        date.to_datetime().unwrap().to_string(),
        "9999-12-31 00:00:00.001"
    );
    assert!(date.is_reference_representable());
    let time = Date::from_serial(0.5, Windows1900, Time).unwrap();
    assert_eq!(time.to_time().unwrap().to_string(), "12:00:00");
    let elapsed = Date::from_serial(-1.25, Mac1904, Duration).unwrap();
    assert_eq!(elapsed.to_duration().unwrap().num_seconds(), -108000);
}

#[test]
fn literal_calendar_clock_and_duration_keep_microseconds_and_ambiguous_dates() {
    let literal = Date::from_ymd_hms_micro(1899, 12, 31, 0, 0, 0, 123456).unwrap();
    assert_eq!(
        literal.to_datetime().unwrap().to_string(),
        "1899-12-31 00:00:00.123456"
    );
    // The literal Gregorian day disambiguates equal early Windows serials.
    assert!((literal.serial_in(Mac1904).unwrap() + 1461.0 - 0.123456 / 86400.0).abs() < 1e-10);
    let loaded = Date::from_serial(literal.serial(), Windows1900, DateTime).unwrap();
    assert_eq!(
        loaded.to_datetime().unwrap().to_string(),
        "1899-12-31 00:00:00.123"
    );
    let clock = Date::from_hms_micro(2, 57, 46, 666570).unwrap();
    assert_eq!(clock.to_time().unwrap().to_string(), "02:57:46.666570");
    let loaded_clock = Date::from_serial(clock.serial(), Windows1900, Time).unwrap();
    assert_eq!(loaded_clock.to_time().unwrap().to_string(), "02:57:46.667");
    let elapsed = Date::from_duration_parts(-1, 86399, 999999).unwrap();
    assert_eq!(elapsed.to_duration().unwrap().num_microseconds(), Some(-1));
    let loaded_elapsed = Date::from_serial(elapsed.serial(), Windows1900, Duration).unwrap();
    assert_eq!(
        loaded_elapsed.to_duration().unwrap().num_microseconds(),
        Some(0)
    );
    let huge = Date::from_duration_parts(999999999, 86399, 999999).unwrap();
    assert!(huge.is_reference_representable());
    assert!(Date::from_hms_micro(0, 0, 59, 1000000).is_err());
    assert!(Date::from_ymd_hms_micro(10000, 1, 1, 0, 0, 0, 0).is_err());
    assert!(Date::from_duration_parts(1000000000, 0, 0).is_err());
    assert!(Date::from_duration_parts(0, 86400, 0).is_err());
}

#[test]
fn iso_prefix_fraction_and_duration_behavior_matches_recorded_public_probe() {
    use crabxl_core::parse_iso8601;
    for (text, kind, iso) in [
        ("2024-02-29", crabxl_core::DateKind::Date, "2024-02-29"),
        (
            "2024-02-29garbage",
            crabxl_core::DateKind::Date,
            "2024-02-29",
        ),
        ("2024-02-29T12", crabxl_core::DateKind::Date, "2024-02-29"),
        (
            "2024-02-29T12:34:56.123456",
            DateTime,
            "2024-02-29T12:34:56.123",
        ),
        ("2024-02-29T12:34:56+02:00", DateTime, "2024-02-29T12:34:56"),
        ("12:34:56.12", Time, "12:34:56.120"),
        ("12:34:", Time, "12:34:00"),
        ("12:34:56.123456", Time, "12:34:56.123"),
    ] {
        let value = parse_iso8601(text).unwrap().unwrap();
        assert_eq!(value.kind(), kind, "{text}");
        assert_eq!(value.to_iso8601().unwrap(), iso, "{text}");
    }
    for (text, milliseconds) in [
        ("PT1H2M3.123S", 3723123),
        ("PT1H2M3.123456S", 3720000),
        ("PT100H", 360000000),
        ("PT1.2S", 1200),
        ("PT0S", 0),
    ] {
        let value = parse_iso8601(text).unwrap().unwrap();
        assert_eq!(value.kind(), Duration);
        assert_eq!(
            value.to_duration().unwrap().num_milliseconds(),
            milliseconds
        );
    }
    assert!(parse_iso8601("").unwrap().is_none());
    assert!(Date::from_serial(1.5, Windows1900, crabxl_core::DateKind::Date).is_err());
    for text in [
        " 2024-02-29",
        "2024-02",
        "2024-02-30",
        "0000-01-01",
        "12",
        "12:99",
        "PT",
        "P1DT2H",
        "PT0.123456S",
        "-PT1S",
        "PT999999999999999999999H",
    ] {
        assert!(parse_iso8601(text).is_err(), "{text}");
    }
    let literal = Date::from_ymd_hms_micro(2024, 1, 1, 0, 0, 0, 123).unwrap();
    assert_eq!(literal.to_iso8601().unwrap(), "2024-01-01T00:00:00.000");
    assert_eq!(
        Date::from_duration_parts(-1, 86399, 999999)
            .unwrap()
            .serial(),
        -0.000001 / 86400.0
    );
}

#[test]
fn formula_ranges_and_literal_source_prefixes_keep_distinct_identities() {
    use crabxl_core::{CellAddress, CellRange, FormulaRange};
    let literal = Formula::new("==1", None).unwrap();
    assert_eq!(literal.expression(), "=1");
    let source = Formula::from_source("=1", None, None).unwrap();
    assert_eq!(source.expression(), "=1");
    assert!(Formula::new("", None).is_err());
    assert_eq!(
        Formula::from_source("", None, None).unwrap().expression(),
        ""
    );
    let mut range = FormulaRange::from_xml("$A$1:$B$2").unwrap();
    assert_eq!(range.spelling(), "$A$1:$B$2");
    assert_eq!(range.range().start, CellAddress::new(0, 0).unwrap());
    range
        .set_range(
            CellRange::new(
                CellAddress::new(1, 1).unwrap(),
                CellAddress::new(2, 2).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(range.spelling(), "B2:C3");
    assert!("B2:A1".parse::<CellRange>().is_err());
    assert!("A1:B2:C3".parse::<CellRange>().is_err());
}

#[test]
fn builtin_format_resolution_keeps_declared_overrides_and_unknown_identities() {
    use crabxl_core::{
        BUILTIN_NUMBER_FORMATS, NumberFormat, StyleCatalog, builtin_number_format,
        builtin_number_format_id,
    };
    for (id, code) in BUILTIN_NUMBER_FORMATS {
        assert_eq!(builtin_number_format(*id), Some(*code));
        assert_eq!(builtin_number_format_id(code), Some(*id));
    }
    assert_eq!(builtin_number_format(14), Some("mm-dd-yy"));
    assert_eq!(builtin_number_format(47), Some("mmss.0"));
    assert_eq!(builtin_number_format(23), None);
    assert_eq!(builtin_number_format(u32::MAX), None);
    assert_eq!(builtin_number_format_id("m/d/yy"), None);
    let catalog = StyleCatalog {
        number_formats: vec![
            NumberFormat::new(14, "yyyy-mm-dd"),
            NumberFormat::new(u32::MAX, "0.000"),
        ],
        ..Default::default()
    };
    assert_eq!(catalog.number_format(14), Some("yyyy-mm-dd"));
    assert_eq!(catalog.number_format(49), Some("@"));
    assert_eq!(catalog.number_format(u32::MAX), Some("0.000"));
    assert_eq!(catalog.number_format(27), None);
}

#[test]
fn borrowed_style_views_share_components_and_do_not_install_inheritance() {
    use crabxl_core::{CellFormat, Font, StyleCatalog, StyleId};
    let mut catalog = StyleCatalog {
        fonts: vec![Font {
            name: Some("Shared face".into()),
            ..Default::default()
        }],
        fills: vec![Default::default()],
        borders: vec![Default::default()],
        cell_formats: vec![
            CellFormat {
                number_format_id: 14,
                apply_font: Some(false),
                ..Default::default()
            },
            CellFormat {
                number_format_id: 27,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let first = catalog.cell_style(StyleId::new(0)).unwrap();
    let second = catalog.cell_style(StyleId::new(1)).unwrap();
    assert!(std::ptr::eq(first.font, second.font));
    assert!(std::ptr::eq(first.fill, second.fill));
    assert_eq!(first.number_format, Some("mm-dd-yy"));
    assert_eq!(second.number_format, None);
    assert_eq!(first.format.apply_font, Some(false));
    assert!(first.alignment.is_none());
    assert!(first.protection.is_none());
    catalog.cell_formats[0].font_id = 100;
    assert!(catalog.cell_style(StyleId::new(0)).is_err());
    assert!(catalog.cell_style(StyleId::new(99)).is_err());
}

#[test]
fn number_format_classification_stays_consistent_when_the_owned_code_changes() {
    use crabxl_core::{DateKind, NumberFormat, classify_number_format};
    let mut format = NumberFormat::new(14, "0.000");
    assert_eq!(format.date_kind(), None);
    format.set_code("[h]:mm:ss.000");
    assert_eq!(format.date_kind(), Some(DateKind::Duration));
    assert_eq!(format.code(), "[h]:mm:ss.000");
    assert_eq!(format.id(), 14);
    let snapshot = format.clone();
    format.set_code("yyyy-mm-dd");
    assert_eq!(format.date_kind(), Some(DateKind::DateTime));
    assert_eq!(snapshot.date_kind(), Some(DateKind::Duration));
    for code in ["0.00", "\"day\"0", "\\d0", "[Red]0.00", "0;yyyy"] {
        assert_eq!(classify_number_format(code), None, "{code}");
    }
}

#[test]
fn optional_structured_expression_preserves_absence_without_a_dummy_literal() {
    use crabxl_core::{FormulaMetadata, FormulaType};
    for kind in [FormulaType::Array, FormulaType::DataTable] {
        let metadata = FormulaMetadata {
            kind,
            ..Default::default()
        };
        let absent = Formula::with_optional_expression(None, None, metadata.clone()).unwrap();
        let empty =
            Formula::with_optional_expression(Some("".into()), None, metadata.clone()).unwrap();
        assert_eq!(absent.expression(), "");
        assert_eq!(absent.optional_expression(), None);
        assert_eq!(absent.clone().optional_expression(), None);
        assert_eq!(empty.optional_expression(), Some(""));
        assert_ne!(absent, empty);
        let source = Formula::from_source("", None, Some(metadata)).unwrap();
        assert_eq!(source.optional_expression(), Some(""));
    }
    assert!(Formula::with_optional_expression(None, None, FormulaMetadata::default()).is_err());
}

#[test]
fn literal_array_text_borrows_original_property_and_slices_one_unicode_character() {
    use crabxl_core::{FormulaMetadata, FormulaType};
    use std::borrow::Cow;
    for (literal, body) in [
        ("", ""),
        ("=", ""),
        ("=1", "1"),
        ("1", ""),
        ("abc", "bc"),
        ("==1", "=1"),
        ("\u{3b1}x", "x"),
    ] {
        let formula = Formula::from_array_text(
            Some(literal.into()),
            None,
            FormulaMetadata {
                kind: FormulaType::Array,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(formula.expression(), body);
        assert_eq!(formula.optional_expression(), Some(body));
        assert!(matches!(formula.array_text(), Some(Cow::Borrowed(value)) if value == literal));
        assert_eq!(formula.clone().array_text().as_deref(), Some(literal));
    }
    let source = Formula::from_source(
        "abc",
        None,
        Some(FormulaMetadata {
            kind: FormulaType::Array,
            literal_array_text: true,
            ..Default::default()
        }),
    )
    .unwrap();
    assert_eq!(source.expression(), "abc");
    assert_eq!(source.array_text().as_deref(), Some("=abc"));
    let absent = Formula::from_array_text(
        None,
        None,
        FormulaMetadata {
            kind: FormulaType::Array,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(absent.array_text().is_none());
    assert!(Formula::from_array_text(None, None, FormulaMetadata::default()).is_err());
}
