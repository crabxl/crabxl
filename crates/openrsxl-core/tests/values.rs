//! Generated shared date/formula boundary and payload tests.
#![allow(clippy::unwrap_used)]
use openrsxl_core::{CellValue, DateEpoch::*, DateKind::*, ExcelDateTime as Date, Formula};
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
