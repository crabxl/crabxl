//! Generated scalar fixtures and injected I/O failures; no upstream binary copies.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use openrsxl_core::{Cell, CellAddress, CellValue, ErrorKind, ExactInteger, Row, RowIndex};
use openrsxl_xlsx::{WorkbookReader, WorkbookWriter, WriteOptions};
use std::io::{Cursor, Seek, Write};

fn row(index: u32, values: Vec<CellValue>) -> Row {
    Row {
        index: RowIndex::new(index).unwrap(),
        cells: values
            .into_iter()
            .enumerate()
            .map(|(column, value)| Cell {
                address: CellAddress::new(index, column as u32).unwrap(),
                value,
                style: openrsxl_core::StyleId::new(0),
            })
            .collect(),
    }
}
fn options(directory: &tempfile::TempDir) -> WriteOptions {
    WriteOptions {
        temp_directory: Some(directory.path().to_owned()),
        ..WriteOptions::default()
    }
}
fn files(directory: &tempfile::TempDir) -> usize {
    std::fs::read_dir(directory.path()).unwrap().count()
}

#[test]
fn scalar_round_trip_sparse_rows_empty_sheet_and_epoch() {
    let directory = tempfile::tempdir().unwrap();
    let mut writer = WorkbookWriter::new(WriteOptions {
        date_1904: true,
        ..options(&directory)
    })
    .unwrap();
    writer.start_sheet("A & \"B\"").unwrap();
    let values = vec![
        CellValue::Integer(9007199254740993),
        CellValue::Integer(i64::MIN),
        CellValue::BigInteger(Box::new(
            ExactInteger::parse("999999999999999999999999999999").unwrap(),
        )),
        CellValue::Number(1.0),
        CellValue::Number(-0.0),
        CellValue::Boolean(false),
        CellValue::Boolean(true),
        CellValue::error("#DIV/0!"),
        CellValue::text(" \t<&>\r\n "),
        CellValue::text(""),
        CellValue::Empty,
    ];
    let expected = row(7, values);
    writer.write_row(&expected).unwrap();
    writer.start_sheet("Empty").unwrap();
    writer.close_sheet().unwrap();
    writer.close_sheet().unwrap();
    assert_eq!(writer.stats().rows, 1);
    assert_eq!(writer.stats().cells, 11);
    let temp_bytes = std::fs::read_dir(directory.path())
        .unwrap()
        .map(|entry| entry.unwrap().metadata().unwrap().len())
        .sum::<u64>();
    assert_eq!(writer.temporary_bytes(), temp_bytes);
    assert_eq!(writer.stats().peak_temp_bytes, temp_bytes);
    let output = writer.finish(Cursor::new(Vec::new())).unwrap();
    assert_eq!(files(&directory), 0);
    let mut book = WorkbookReader::new(output).unwrap();
    assert!(book.date_1904());
    assert_eq!(book.sheets()[0].name(), "A & \"B\"");
    let actual = book.read_sheet("A & \"B\"").unwrap();
    assert!(
        matches!(actual.rows[0].cells[4].value, CellValue::Number(number) if number == 0.0 && number.is_sign_negative())
    );
    assert_eq!(actual.rows, vec![expected]);
    assert!(book.read_sheet("Empty").unwrap().rows.is_empty());
}
#[test]
fn rejected_rows_do_not_commit_and_sequential_state_is_explicit() {
    let directory = tempfile::tempdir().unwrap();
    let mut writer = WorkbookWriter::new(options(&directory)).unwrap();
    assert_eq!(
        writer.write_row(&row(0, vec![])).unwrap_err().kind(),
        ErrorKind::InvalidState
    );
    writer.start_sheet("Sheet").unwrap();
    let before = writer.temporary_bytes();
    for value in [
        CellValue::Number(f64::NAN),
        CellValue::Number(f64::INFINITY),
        CellValue::text("bad\0text"),
        CellValue::error(""),
    ] {
        let error = writer.write_row(&row(0, vec![value])).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
        assert_eq!(error.cell().unwrap().to_string(), "A1");
        assert_eq!(error.part(), Some("xl/worksheets/sheet1.xml"));
        assert_eq!(writer.temporary_bytes(), before);
    }
    assert_eq!(
        writer
            .write_row(&row(0, vec![CellValue::text("literal_x0041_")]))
            .unwrap_err()
            .kind(),
        ErrorKind::Unsupported
    );
    let mut invalid = row(0, vec![CellValue::Integer(1), CellValue::Integer(2)]);
    invalid.cells.reverse();
    assert_eq!(
        writer.write_row(&invalid).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    writer
        .write_row(&row(0, vec![CellValue::Integer(3)]))
        .unwrap();
    assert_eq!(
        writer
            .write_row(&row(0, vec![CellValue::Integer(4)]))
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidState
    );
    let output = writer.finish(Cursor::new(Vec::new())).unwrap();
    assert_eq!(
        WorkbookReader::new(output)
            .unwrap()
            .read_sheet("Sheet")
            .unwrap()
            .rows[0]
            .cells[0]
            .value,
        CellValue::Integer(3)
    );
}
#[test]
fn abort_and_drop_clean_every_owned_file_without_saving() {
    let directory = tempfile::tempdir().unwrap();
    let mut writer = WorkbookWriter::new(options(&directory)).unwrap();
    writer.start_sheet("First").unwrap();
    writer.start_sheet("Second").unwrap();
    assert_eq!(files(&directory), 2);
    writer.abort().unwrap();
    writer.abort().unwrap();
    assert_eq!(files(&directory), 0);
    assert_eq!(writer.temporary_bytes(), 0);
    assert_eq!(
        writer.start_sheet("Third").unwrap_err().kind(),
        ErrorKind::InvalidState
    );
    assert_eq!(
        writer.finish(Cursor::new(Vec::new())).unwrap_err().kind(),
        ErrorKind::InvalidState
    );
    {
        let mut writer = WorkbookWriter::new(options(&directory)).unwrap();
        writer.start_sheet("First").unwrap();
        writer
            .write_row(&row(0, vec![CellValue::text("unfinished")]))
            .unwrap();
        writer.start_sheet("Second").unwrap();
        writer
            .write_row(&row(0, vec![CellValue::Integer(1)]))
            .unwrap();
    }
    assert_eq!(files(&directory), 0);
}
#[test]
fn name_sheet_row_cell_metadata_and_temp_limits_are_enforced() {
    let directory = tempfile::tempdir().unwrap();
    let mut writer = WorkbookWriter::new(options(&directory)).unwrap();
    for name in [
        "",
        "Bad/Name",
        "'quoted",
        "quoted'",
        "bad\nname",
        "12345678901234567890123456789012",
    ] {
        assert_eq!(
            writer.start_sheet(name).unwrap_err().kind(),
            ErrorKind::InvalidData
        );
    }
    writer.start_sheet("Sheet").unwrap();
    assert_eq!(
        writer.start_sheet("SHEET").unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    writer
        .write_row(&row(0, vec![CellValue::Integer(1)]))
        .unwrap();
    drop(writer);
    for configured in [
        WriteOptions {
            max_sheets: 1,
            ..options(&directory)
        },
        WriteOptions {
            max_row_bytes: 20,
            ..options(&directory)
        },
        WriteOptions {
            max_cell_bytes: 2,
            ..options(&directory)
        },
        WriteOptions {
            max_row_cells: 1,
            ..options(&directory)
        },
        WriteOptions {
            max_temp_bytes: 1,
            ..options(&directory)
        },
        WriteOptions {
            max_sheet_bytes: 1,
            ..options(&directory)
        },
        WriteOptions {
            buffer_bytes: 1,
            max_metadata_bytes: 1,
            ..options(&directory)
        },
    ] {
        let mut writer = match WorkbookWriter::new(configured.clone()) {
            Ok(writer) => writer,
            Err(error) => {
                assert_eq!(error.kind(), ErrorKind::LimitExceeded);
                assert_eq!(files(&directory), 0);
                continue;
            }
        };
        match writer.start_sheet("Sheet") {
            Err(error) => assert_eq!(error.kind(), ErrorKind::LimitExceeded),
            Ok(()) => {
                let result = if configured.max_sheets == 1 {
                    writer.start_sheet("Another")
                } else {
                    writer.write_row(&row(
                        0,
                        vec![CellValue::text("long"), CellValue::Integer(1)],
                    ))
                };
                assert_eq!(result.unwrap_err().kind(), ErrorKind::LimitExceeded);
            }
        }
        writer.abort().unwrap();
        assert_eq!(files(&directory), 0);
    }
}
#[test]
fn failed_new_temp_file_keeps_previous_sheet_usable() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("spool");
    std::fs::create_dir(&path).unwrap();
    let mut writer = WorkbookWriter::new(WriteOptions {
        temp_directory: Some(path.clone()),
        ..WriteOptions::default()
    })
    .unwrap();
    writer.start_sheet("First").unwrap();
    let moved = directory.path().join("moved");
    std::fs::rename(&path, &moved).unwrap();
    assert_eq!(
        writer.start_sheet("Second").unwrap_err().kind(),
        ErrorKind::Io
    );
    std::fs::rename(&moved, &path).unwrap();
    writer
        .write_row(&row(0, vec![CellValue::Integer(7)]))
        .unwrap();
    writer.finish(Cursor::new(Vec::new())).unwrap();
    assert_eq!(std::fs::read_dir(path).unwrap().count(), 0);
}
struct FailingSink {
    cursor: Cursor<Vec<u8>>,
    remaining: usize,
}
impl Write for FailingSink {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        if self.remaining == 0 {
            return Err(std::io::Error::other("Injected output failure"));
        }
        let count = data.len().min(self.remaining);
        self.remaining -= count;
        self.cursor.write(&data[..count])
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl Seek for FailingSink {
    fn seek(&mut self, position: std::io::SeekFrom) -> std::io::Result<u64> {
        self.cursor.seek(position)
    }
}
#[test]
fn finish_io_failure_cleans_spools_and_preserves_borrowed_sink() {
    use std::error::Error as _;
    let directory = tempfile::tempdir().unwrap();
    for bytes in [0, 64, 400] {
        let mut writer = WorkbookWriter::new(options(&directory)).unwrap();
        writer.start_sheet("First").unwrap();
        writer
            .write_row(&row(0, vec![CellValue::text("value")]))
            .unwrap();
        writer.start_sheet("Second").unwrap();
        let mut sink = FailingSink {
            cursor: Cursor::new(Vec::new()),
            remaining: bytes,
        };
        let error = writer.finish(&mut sink).err().unwrap();
        assert_eq!(error.kind(), ErrorKind::Io);
        assert!(error.source().is_some());
        assert_eq!(files(&directory), 0);
        sink.remaining = 10;
        sink.write_all(b"usable").unwrap();
    }
}

#[test]
fn abort_attempts_remaining_cleanup_after_one_unlink_failure() {
    let directory = tempfile::tempdir().unwrap();
    let mut writer = WorkbookWriter::new(options(&directory)).unwrap();
    writer.start_sheet("First").unwrap();
    writer.start_sheet("Second").unwrap();
    let obstructed = std::fs::read_dir(directory.path())
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    std::fs::remove_file(&obstructed).unwrap();
    std::fs::create_dir(&obstructed).unwrap();
    assert_eq!(writer.abort().unwrap_err().kind(), ErrorKind::Io);
    assert_eq!(files(&directory), 1);
    assert!(obstructed.is_dir());
    assert_eq!(writer.abort().unwrap_err().kind(), ErrorKind::Io);
    std::fs::remove_dir(obstructed).unwrap();
    writer.abort().unwrap();
}

#[test]
fn normal_formula_round_trip_and_data_only_caches() {
    use openrsxl_core::{Formula, ReadOptions};
    let directory = tempfile::tempdir().unwrap();
    let caches = [
        None,
        Some(CellValue::Integer(0)),
        Some(CellValue::Boolean(false)),
        Some(CellValue::text(" cached & < \r\n")),
        Some(CellValue::text("")),
        Some(CellValue::error("#N/A")),
    ];
    let values = caches
        .iter()
        .map(|cache| {
            CellValue::Formula(Box::new(
                Formula::new("=IF(A1<2,0,1)", cache.clone()).unwrap(),
            ))
        })
        .collect();
    let expected = row(0, values);
    let mut writer = WorkbookWriter::new(options(&directory)).unwrap();
    writer.start_sheet("Sheet").unwrap();
    writer.write_row(&expected).unwrap();
    let output = writer.finish(Cursor::new(Vec::new())).unwrap();
    let mut book = WorkbookReader::new(output).unwrap();
    assert_eq!(book.read_sheet("Sheet").unwrap().rows, vec![expected]);
    let mut rows = book
        .rows_with_options(
            "Sheet",
            ReadOptions {
                data_only: true,
                ..ReadOptions::default()
            },
        )
        .unwrap();
    let actual: Vec<_> = rows
        .next_row()
        .unwrap()
        .unwrap()
        .cells
        .into_iter()
        .map(|cell| cell.value)
        .collect();
    assert_eq!(
        actual,
        caches
            .into_iter()
            .map(|cache| cache.unwrap_or(CellValue::Empty))
            .collect::<Vec<_>>()
    );
    assert_eq!(files(&directory), 0);
}
#[test]
fn shared_styles_are_deduplicated_bounded_and_references_are_checked() {
    use openrsxl_core::{CellStyle, DateEpoch, DateKind, ExcelDateTime, Formula, StyleId};
    let directory = tempfile::tempdir().unwrap();
    let mut writer = WorkbookWriter::new(WriteOptions {
        max_styles: 5,
        ..options(&directory)
    })
    .unwrap();
    let mut style = CellStyle::default();
    style.font.bold = true;
    let id = writer.register_style(style.clone()).unwrap();
    assert_eq!(writer.register_style(style).unwrap(), id);
    assert_eq!(
        writer
            .register_style(CellStyle {
                fill: Some(0x123456),
                ..CellStyle::default()
            })
            .unwrap_err()
            .kind(),
        ErrorKind::LimitExceeded
    );
    assert_eq!(
        writer
            .register_style(CellStyle {
                rotation: 181,
                ..CellStyle::default()
            })
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidData
    );
    writer.start_sheet("Sheet").unwrap();
    let mut invalid = row(0, vec![CellValue::Integer(1)]);
    invalid.cells[0].style = StyleId::new(999);
    assert_eq!(
        writer.write_row(&invalid).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    let nan_cache = row(
        0,
        vec![CellValue::Formula(Box::new(
            Formula::new("1", Some(CellValue::Number(f64::NAN))).unwrap(),
        ))],
    );
    assert_eq!(
        writer.write_row(&nan_cache).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    writer
        .write_row(&row(0, vec![CellValue::Integer(1)]))
        .unwrap();
    writer.finish(Cursor::new(Vec::new())).unwrap();
    let mut writer = WorkbookWriter::new(WriteOptions {
        date_1904: true,
        ..options(&directory)
    })
    .unwrap();
    writer.start_sheet("Sheet").unwrap();
    let invalid = row(
        0,
        vec![CellValue::DateTime(Box::new(
            ExcelDateTime::from_serial(60.0, DateEpoch::Windows1900, DateKind::DateTime).unwrap(),
        ))],
    );
    assert_eq!(
        writer.write_row(&invalid).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    writer
        .write_row(&row(0, vec![CellValue::Integer(1)]))
        .unwrap();
    writer.finish(Cursor::new(Vec::new())).unwrap();
    assert_eq!(files(&directory), 0);
}

#[test]
fn date_serials_formats_and_epoch_flags_are_encoded_consistently() {
    use openrsxl_core::{DateEpoch, DateKind, ExcelDateTime};
    use std::io::Read;
    for date_1904 in [false, true] {
        let mut writer = WorkbookWriter::new(WriteOptions {
            date_1904,
            ..WriteOptions::default()
        })
        .unwrap();
        writer.start_sheet("Sheet").unwrap();
        let values = [(1900, 2, 28), (1900, 3, 1), (1904, 1, 1), (2024, 2, 29)]
            .into_iter()
            .map(|(y, m, d)| {
                CellValue::DateTime(Box::new(
                    ExcelDateTime::from_ymd_hms_milli(y, m, d, 12, 0, 0, 0).unwrap(),
                ))
            })
            .chain([
                CellValue::DateTime(Box::new(
                    ExcelDateTime::from_serial(0.5, DateEpoch::Windows1900, DateKind::Time)
                        .unwrap(),
                )),
                CellValue::DateTime(Box::new(
                    ExcelDateTime::from_serial(-1.5, DateEpoch::Windows1900, DateKind::Duration)
                        .unwrap(),
                )),
            ])
            .collect();
        writer.write_row(&row(0, values)).unwrap();
        let mut zip =
            zip::ZipArchive::new(writer.finish(Cursor::new(Vec::new())).unwrap()).unwrap();
        let mut xml = String::new();
        zip.by_name("xl/worksheets/sheet1.xml")
            .unwrap()
            .read_to_string(&mut xml)
            .unwrap();
        let serials = if date_1904 {
            [-1401.5, -1400.5, 0.5, 43889.5]
        } else {
            [59.5, 61.5, 1462.5, 45351.5]
        };
        for (column, serial) in serials.into_iter().enumerate() {
            assert!(xml.contains(&format!(
                "<c r=\"{}1\" s=\"1\"><v>{serial:?}</v></c>",
                (b'A' + column as u8) as char
            )));
        }
        assert!(xml.contains("<c r=\"E1\" s=\"2\"><v>0.5</v></c>"));
        assert!(xml.contains("<c r=\"F1\" s=\"3\"><v>-1.5</v></c>"));
        xml.clear();
        zip.by_name("xl/workbook.xml")
            .unwrap()
            .read_to_string(&mut xml)
            .unwrap();
        assert!(xml.contains(&format!("date1904=\"{}\"", u8::from(date_1904))));
    }
}

#[test]
fn explicit_date_formats_validate_literals_escapes_and_duration_kind() {
    use openrsxl_core::{CellStyle, DateEpoch, DateKind, ExcelDateTime};
    for (format, kind, valid) in [
        ("yyyy-mm-dd", DateKind::DateTime, true),
        ("[Red]hh:mm:ss", DateKind::Time, true),
        ("[hh]:mm:ss", DateKind::Duration, true),
        ("\"days\" 0.00", DateKind::DateTime, false),
        ("0.00\\m", DateKind::DateTime, false),
        ("0.00_m", DateKind::DateTime, false),
        ("0.00*m", DateKind::DateTime, false),
        ("0.00;yyyy-mm-dd", DateKind::DateTime, false),
        ("[h]:mm:ss", DateKind::DateTime, false),
        ("hh:mm:ss", DateKind::Duration, false),
    ] {
        let mut writer = WorkbookWriter::new(WriteOptions::default()).unwrap();
        let style = writer
            .register_style(CellStyle {
                number_format: format.into(),
                ..CellStyle::default()
            })
            .unwrap();
        writer.start_sheet("Sheet").unwrap();
        let mut values = row(
            0,
            vec![CellValue::DateTime(Box::new(
                ExcelDateTime::from_serial(
                    if kind == DateKind::Time { 0.5 } else { 1.5 },
                    DateEpoch::Windows1900,
                    kind,
                )
                .unwrap(),
            ))],
        );
        values.cells[0].style = style;
        let result = writer.write_row(&values);
        if valid {
            result.unwrap();
        } else {
            assert_eq!(result.unwrap_err().kind(), ErrorKind::InvalidData);
        }
        writer.abort().unwrap();
    }
    let mut writer = WorkbookWriter::new(WriteOptions::default()).unwrap();
    let style = CellStyle {
        number_format: "x".repeat(256).into(),
        ..CellStyle::default()
    };
    assert_eq!(
        writer.register_style(style).unwrap_err().kind(),
        ErrorKind::LimitExceeded
    );
}
