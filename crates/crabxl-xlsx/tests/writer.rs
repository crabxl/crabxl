//! Generated scalar fixtures and injected I/O failures; no upstream binary copies.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use crabxl_core::{Cell, CellAddress, CellValue, ErrorKind, ExactInteger, Row, RowIndex};
use crabxl_xlsx::{WorkbookReader, WorkbookWriter, WriteOptions};
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
                style: crabxl_core::StyleId::new(0),
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
        CellValue::text(" \u{1f980}\u{e9}\t<&>\r\n\u{6587}\u{5b57} "),
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
        // Query the file directly; Windows directory entries can cache old sizes.
        .map(|entry| std::fs::metadata(entry.unwrap().path()).unwrap().len())
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
    let mut writer = WorkbookWriter::new(WriteOptions {
        non_finite: crabxl_xlsx::NonFiniteWritePolicy::Reject,
        ..options(&directory)
    })
    .unwrap();
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
// Windows does not permit renaming a directory containing an open spool.
#[cfg(unix)]
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
    use crabxl_core::{Formula, ReadOptions};
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
            .map(|cache| match cache {
                Some(CellValue::Text(text)) if text.as_str().is_empty() => CellValue::Empty,
                other => other.unwrap_or(CellValue::Empty),
            })
            .collect::<Vec<_>>()
    );
    assert_eq!(files(&directory), 0);
}
#[test]
fn shared_styles_are_deduplicated_bounded_and_references_are_checked() {
    use crabxl_core::{CellStyle, DateEpoch, DateKind, ExcelDateTime, Formula, StyleId};
    let directory = tempfile::tempdir().unwrap();
    let mut writer = WorkbookWriter::new(WriteOptions {
        max_styles: 6,
        non_finite: crabxl_xlsx::NonFiniteWritePolicy::Reject,
        ..options(&directory)
    })
    .unwrap();
    let mut style = CellStyle::default();
    style.font.bold = Some(true);
    let id = writer.register_style(style.clone()).unwrap();
    assert_eq!(writer.register_style(style).unwrap(), id);
    assert_eq!(
        writer
            .register_style(CellStyle {
                fill: crabxl_core::Fill::solid(crabxl_core::Color {
                    kind: crabxl_core::ColorKind::Argb(0xFF123456),
                    tint: None
                }),
                ..CellStyle::default()
            })
            .unwrap_err()
            .kind(),
        ErrorKind::LimitExceeded
    );
    assert_eq!(
        writer
            .register_style(CellStyle {
                alignment: crabxl_core::Alignment {
                    rotation: Some(181),
                    ..Default::default()
                },
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
    use crabxl_core::{DateEpoch, DateKind, ExcelDateTime, StyleId};
    use std::io::Read;
    // Direct streamed rows can explicitly override automatic date formatting.
    let mut explicit = WorkbookWriter::new(Default::default()).unwrap();
    explicit.start_sheet("General").unwrap();
    let mut cell = Cell {
        address: CellAddress::new(0, 0).unwrap(),
        value: CellValue::DateTime(Box::new(ExcelDateTime::from_ymd(2024, 1, 2).unwrap())),
        style: StyleId::new(0),
    };
    assert!(cell.set_style(StyleId::new(0)));
    assert!(!cell.set_style(StyleId::new(0)));
    explicit
        .write_row(&Row {
            index: RowIndex::new(0).unwrap(),
            cells: vec![cell],
        })
        .unwrap();
    let mut reader =
        WorkbookReader::new(explicit.finish(Cursor::new(Vec::new())).unwrap()).unwrap();
    let data = reader.read_sheet("General").unwrap();
    assert_eq!(data.rows[0].cells[0].style, StyleId::new(0));
    assert_eq!(data.rows[0].cells[0].value, CellValue::Integer(45293));
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
fn assigned_temporal_values_derive_nondate_formats_and_preserve_any_existing_date_format() {
    use crabxl_core::{CellStyle, DateEpoch, DateKind, ExcelDateTime};
    for (format, kind, retained) in [
        ("yyyy-mm-dd", DateKind::DateTime, true),
        ("[Red]hh:mm:ss", DateKind::Time, true),
        ("[hh]:mm:ss", DateKind::Duration, true),
        ("\"days\" 0.00", DateKind::DateTime, false),
        ("0.00\\m", DateKind::DateTime, false),
        ("0.00_m", DateKind::DateTime, false),
        ("0.00*m", DateKind::DateTime, false),
        ("0.00;yyyy-mm-dd", DateKind::DateTime, false),
        ("[h]:mm:ss", DateKind::DateTime, true),
        ("hh:mm:ss", DateKind::Duration, true),
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
        writer.write_row(&values).unwrap();
        let output = writer.finish(Cursor::new(Vec::new())).unwrap();
        let mut reader = WorkbookReader::new(output).unwrap();
        let loaded = reader.read_sheet("Sheet").unwrap();
        let output_id = loaded.rows[0].cells[0].style;
        assert_eq!(output_id == style, retained);
        let catalog = reader.style_catalog().unwrap().unwrap();
        let output_format = catalog.cell_format(output_id).unwrap();
        assert_eq!(
            catalog.number_format(output_format.number_format_id),
            Some(if retained {
                format
            } else {
                kind.default_number_format()
            })
        );
    }
    let mut writer = WorkbookWriter::new(WriteOptions::default()).unwrap();
    let style = CellStyle {
        number_format: "x".repeat(256).into(),
        ..CellStyle::default()
    };
    // Public reference permits long format codes; the configured byte budget bounds them.
    assert!(writer.register_style(style).is_ok());
    assert_eq!(
        writer
            .register_style(CellStyle {
                number_format: "x"
                    .repeat(WriteOptions::default().max_metadata_bytes + 1)
                    .into(),
                ..Default::default()
            })
            .unwrap_err()
            .kind(),
        ErrorKind::LimitExceeded
    );
}

#[test]
fn materialized_sparse_model_exports_with_structural_edits_and_empty_extent() {
    use crabxl_core::{ColumnIndex, EditLimits, Worksheet};
    let mut sheet = Worksheet::new("Model", EditLimits::default()).unwrap();
    sheet.append(vec![]).unwrap();
    sheet
        .append(vec![CellValue::Integer(42), CellValue::text("value")])
        .unwrap();
    sheet
        .insert_columns(ColumnIndex::new(0).unwrap(), 1)
        .unwrap();
    sheet.append(vec![]).unwrap();
    let mut writer = WorkbookWriter::new(WriteOptions::default()).unwrap();
    writer.write_worksheet(&sheet).unwrap();
    let mut book = WorkbookReader::new(writer.finish(Cursor::new(Vec::new())).unwrap()).unwrap();
    let rows = book.read_sheet("Model").unwrap().rows;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].index.get(), 1);
    assert_eq!(rows[0].cells[0].address.to_string(), "B2");
    assert_eq!(rows[0].cells[0].value, CellValue::Integer(42));
    assert_eq!(rows[1].index.get(), 2);
    assert!(rows[1].cells.is_empty());
    assert_eq!(sheet.len(), 2);
    assert_eq!(sheet.row_extent(), 3);
    let mut merged = crabxl_core::Workbook::new(Default::default()).unwrap();
    let id = merged.create_sheet("Sparse merge").unwrap();
    merged
        .merge_cells(id, "A1:XFD1048576".parse().unwrap())
        .unwrap();
    for _ in 0..2 {
        let mut writer = WorkbookWriter::from_canonical_style_catalog(
            WriteOptions::default(),
            merged.style_catalog().cloned().unwrap(),
        )
        .unwrap();
        writer.write_workbook(&merged).unwrap();
        assert_eq!(writer.stats().cells, 1);
        assert!(writer.stats().peak_temp_bytes < 1024);
        let output = writer.finish(Cursor::new(Vec::new())).unwrap();
        let mut reader = WorkbookReader::new(output).unwrap();
        let mut rows = reader.rows("Sparse merge").unwrap();
        rows.capture_merges();
        while rows.next_row().unwrap().is_some() {}
        assert_eq!(rows.merge_ranges(), &["A1:XFD1048576".parse().unwrap()]);
        assert_eq!(rows.take_merge_ranges().len(), 1);
        assert!(rows.merge_ranges().is_empty());
        drop(rows);
        let mut source =
            crabxl_xlsx::LoadedWorkbook::with_options(reader.into_inner(), Default::default())
                .unwrap();
        let id = source.sheet_id("Sparse merge").unwrap();
        source
            .upsert_value(id, "B2".parse().unwrap(), CellValue::Integer(42))
            .unwrap();
        assert_eq!(
            source
                .save(Cursor::new(Vec::new()), Default::default())
                .unwrap_err()
                .kind(),
            crabxl_core::ErrorKind::Unsupported
        );
        source
            .upsert_value(id, "B2".parse().unwrap(), CellValue::Empty)
            .unwrap();
        source
            .unmerge_cells(id, "A1:XFD1048576".parse().unwrap())
            .unwrap();
        source
            .save(Cursor::new(Vec::new()), Default::default())
            .unwrap();
    }
}

#[test]
fn owned_workbook_export_preserves_order_epoch_active_sheet_and_borrowed_values() {
    let mut book = crabxl_core::Workbook::new(crabxl_core::WorkbookLimits::default()).unwrap();
    let first = book.create_sheet("First").unwrap();
    let other = book.create_sheet("Other").unwrap();
    book.sheet_mut(first)
        .unwrap()
        .append(vec![CellValue::Integer(1)])
        .unwrap();
    book.sheet_mut(other)
        .unwrap()
        .append(vec![CellValue::Integer(2)])
        .unwrap();
    book.set_active_sheet(first).unwrap();
    book.move_sheet(other, 0).unwrap();
    book.set_epoch(crabxl_core::DateEpoch::Mac1904);
    let mut writer = WorkbookWriter::new(WriteOptions::default()).unwrap();
    writer.write_workbook(&book).unwrap();
    let output = writer.finish(Cursor::new(Vec::new())).unwrap();
    let mut reader = WorkbookReader::new(output).unwrap();
    assert!(reader.date_1904());
    assert_eq!(reader.active_index(), Some(1));
    assert_eq!(
        reader
            .sheets()
            .iter()
            .map(|sheet| sheet.name())
            .collect::<Vec<_>>(),
        ["Other", "First"]
    );
    assert_eq!(
        reader.read_sheet("First").unwrap().rows[0].cells[0].value,
        CellValue::Integer(1)
    );
    assert_eq!(book.cell_count(), 2);
    use crabxl_core::SheetVisibility::{Hidden, VeryHidden, Visible};
    book.set_sheet_visibility(first, Hidden).unwrap();
    assert!(book.set_active_sheet(first).is_err());
    assert_eq!(book.active_sheet(), Some(first));
    let mut writer = WorkbookWriter::new(WriteOptions::default()).unwrap();
    writer.write_workbook(&book).unwrap();
    let reader = WorkbookReader::new(writer.finish(Cursor::new(Vec::new())).unwrap()).unwrap();
    assert_eq!(reader.active_index(), Some(0));
    assert_eq!(reader.sheets()[0].visibility(), Visible);
    assert_eq!(reader.sheets()[1].visibility(), Hidden);
    // Export normalizes its view without mutating the borrowed owned bank.
    assert_eq!(book.active_sheet(), Some(first));
    book.set_sheet_visibility(other, VeryHidden).unwrap();
    let mut writer = WorkbookWriter::new(WriteOptions::default()).unwrap();
    writer.write_workbook(&book).unwrap();
    let mut target = Cursor::new(b"unchanged".to_vec());
    assert!(writer.finish(&mut target).is_err());
    assert_eq!(target.into_inner(), b"unchanged");
    book.set_sheet_visibility(first, Visible).unwrap();
    let mut writer = WorkbookWriter::new(WriteOptions::default()).unwrap();
    writer.write_workbook(&book).unwrap();
    let reader = WorkbookReader::new(writer.finish(Cursor::new(Vec::new())).unwrap()).unwrap();
    assert_eq!(reader.sheets()[0].visibility(), VeryHidden);
    assert_eq!(reader.active_index(), Some(1));
    for (requested, after, output_index) in [(-3, 1, 1), (-1, -1, 1), (0, 1, 1), (10, 10, 0)] {
        book.set_active_view_index(requested);
        let mut writer = WorkbookWriter::new(WriteOptions::default()).unwrap();
        writer.write_workbook(&book).unwrap();
        writer.set_active_view_index(requested).unwrap();
        assert_eq!(
            writer.active_view_selection().unwrap().requested_index,
            after
        );
        let reader = WorkbookReader::new(writer.finish(Cursor::new(Vec::new())).unwrap()).unwrap();
        assert_eq!(reader.active_index(), Some(output_index));
    }
    let mut invalid = WorkbookWriter::new(WriteOptions {
        active_sheet: 3,
        ..WriteOptions::default()
    })
    .unwrap();
    invalid.start_sheet("Only").unwrap();
    assert!(invalid.finish(Cursor::new(Vec::new())).is_err());
}

#[test]
fn inline_escape_looking_literals_round_trip_without_changing_spelling() {
    let values = [
        "_x0041_",
        "_x005F_x0041_",
        "_x005F__x0041_",
        "_x000D_",
        "_xD83D__xDE00_",
        "😀_x005f_<&>\r\n ",
    ];
    let mut writer = WorkbookWriter::new(WriteOptions::default()).unwrap();
    writer.start_sheet("Sheet").unwrap();
    let cells = values.iter().map(|value| CellValue::text(*value)).collect();
    let expected = row(0, cells);
    writer.write_row(&expected).unwrap();
    let mut read = WorkbookReader::new(writer.finish(Cursor::new(Vec::new())).unwrap()).unwrap();
    assert_eq!(
        read.read_sheet("Sheet").unwrap().rows[0].cells,
        expected.cells
    );
}

fn rich_value() -> CellValue {
    use crabxl_core::{
        Color, ColorKind, FontScheme, PhoneticProperties, PhoneticRun, RichText, RichTextRun,
        RunFont, TextVerticalAlignment, Underline,
    };
    CellValue::RichText(Box::new(RichText {
        runs: vec![
            RichTextRun {
                text: " <&> _x005F_x0041_ \r\n🦀 ".into(),
                font: Some(Box::new(RunFont {
                    name: Some("Quoted \" &\t\n\r".into()),
                    size: Some(12.5),
                    bold: Some(true),
                    italic: Some(false),
                    strike: Some(false),
                    outline: Some(true),
                    shadow: Some(true),
                    condense: Some(false),
                    extend: Some(true),
                    underline: Some(Underline::DoubleAccounting),
                    vertical: Some(TextVerticalAlignment::Superscript),
                    charset: Some(128.into()),
                    family: Some(3.0),
                    scheme: Some(FontScheme::Minor),
                    color: Some(Color {
                        kind: ColorKind::Argb(0x80445566),
                        tint: Some(-0.25),
                    }),
                })),
            },
            RichTextRun {
                text: "tail".into(),
                font: None,
            },
            RichTextRun {
                text: "".into(),
                font: Some(Box::default()),
            },
        ],
        phonetic_runs: vec![PhoneticRun {
            start: 0,
            end: 2,
            text: " pronunciation ".into(),
        }],
        phonetic_properties: Some(Box::new(PhoneticProperties {
            font_id: 0,
            kind: Some("Hiragana".into()),
            alignment: Some("center".into()),
        })),
    }))
}

#[test]
fn typed_rich_runs_fonts_colors_phonetics_and_empty_style_round_trip() {
    use crabxl_core::ReadOptions;
    let mut writer = WorkbookWriter::new(WriteOptions::default()).unwrap();
    writer.start_sheet("Sheet").unwrap();
    let expected = row(0, vec![rich_value()]);
    writer.write_row(&expected).unwrap();
    let mut book = WorkbookReader::new(writer.finish(Cursor::new(Vec::new())).unwrap()).unwrap();
    let typed = book
        .rows_with_options(
            "Sheet",
            ReadOptions {
                rich_text: true,
                ..ReadOptions::default()
            },
        )
        .unwrap()
        .next_row()
        .unwrap()
        .unwrap();
    let mut normalized = expected.cells.clone();
    if let CellValue::RichText(value) = &mut normalized[0].value {
        value.runs[0].text = value.runs[0].text.replace("x005F_", "").into_boxed_str();
    }
    assert_eq!(typed.cells, normalized);
    let CellValue::RichText(value) = &expected.cells[0].value else {
        panic!("Expected rich text")
    };
    assert_eq!(
        book.read_sheet("Sheet").unwrap().rows[0].cells[0].value,
        CellValue::text(value.plain_text().unwrap())
    );
    assert_eq!(size_of::<CellValue>(), 16);
}

#[test]
fn rich_invalid_fonts_budgets_and_phonetic_references_are_atomic() {
    let mut writer = WorkbookWriter::new(WriteOptions::default()).unwrap();
    writer.start_sheet("Sheet").unwrap();
    let before = writer.temporary_bytes();
    for mode in 0..4 {
        let CellValue::RichText(mut value) = rich_value() else {
            unreachable!()
        };
        match mode {
            0 => value.runs[0].font.as_mut().unwrap().size = Some(f64::NAN),
            1 => value.phonetic_properties.as_mut().unwrap().font_id = 100,
            2 => value.phonetic_runs[0].start = 3,
            _ => value.runs[0].text = "bad\0text".into(),
        }
        assert_eq!(
            writer
                .write_row(&row(0, vec![CellValue::RichText(value)]))
                .unwrap_err()
                .kind(),
            ErrorKind::InvalidData
        );
        assert_eq!(writer.temporary_bytes(), before);
    }
    writer.write_row(&row(0, vec![rich_value()])).unwrap();
    assert!(writer.finish(Cursor::new(Vec::new())).is_ok());
    assert!(crabxl_core::Formula::new("1", Some(rich_value())).is_err());
    let mut limited = WorkbookWriter::new(WriteOptions {
        max_cell_bytes: 100,
        ..Default::default()
    })
    .unwrap();
    limited.start_sheet("Sheet").unwrap();
    let before = limited.temporary_bytes();
    assert_eq!(
        limited
            .write_row(&row(0, vec![rich_value()]))
            .unwrap_err()
            .kind(),
        ErrorKind::LimitExceeded
    );
    assert_eq!(limited.temporary_bytes(), before);
    limited
        .write_row(&row(0, vec![CellValue::text("retry")]))
        .unwrap();
    assert!(limited.finish(Cursor::new(Vec::new())).is_ok());
}

#[test]
fn rich_existing_literal_replacement_preserves_other_cells_and_repeats() {
    use crabxl_core::ReadOptions;
    use crabxl_xlsx::WorkbookEditor;
    let CellValue::RichText(mut value) = rich_value() else {
        unreachable!()
    };
    value.phonetic_properties = None;
    value.phonetic_runs.clear();
    let original = CellValue::RichText(value);
    let mut writer = WorkbookWriter::new(WriteOptions::default()).unwrap();
    writer.start_sheet("Sheet").unwrap();
    writer
        .write_row(&row(0, vec![original, CellValue::Integer(42)]))
        .unwrap();
    let mut editor = WorkbookEditor::new(writer.finish(Cursor::new(Vec::new())).unwrap()).unwrap();
    use crabxl_core::{RichText, RichTextRun};
    let replacement = CellValue::RichText(Box::new(RichText {
        runs: vec![RichTextRun {
            text: "edited".into(),
            font: None,
        }],
        ..RichText::default()
    }));
    editor
        .set_value(
            "Sheet",
            CellAddress::new(0, 0).unwrap(),
            replacement.clone(),
        )
        .unwrap();
    for _ in 0..2 {
        let (out, _) = editor
            .save(Cursor::new(Vec::new()), crabxl_xlsx::SaveOptions::default())
            .unwrap();
        let mut book = WorkbookReader::new(out).unwrap();
        let cells = book
            .rows_with_options(
                "Sheet",
                ReadOptions {
                    rich_text: true,
                    ..ReadOptions::default()
                },
            )
            .unwrap()
            .next_row()
            .unwrap()
            .unwrap()
            .cells;
        assert_eq!(cells[0].value, replacement);
        assert_eq!(cells[1].value, CellValue::Integer(42));
    }
}

#[test]
fn rich_color_references_and_absent_or_zero_tints_round_trip() {
    use crabxl_core::{Color, ColorKind, ReadOptions, RichText, RichTextRun, RunFont};
    let values: Vec<_> = [
        ColorKind::Unspecified,
        ColorKind::Argb(0x00112233),
        ColorKind::Theme(7.into()),
        ColorKind::Indexed(64.into()),
        ColorKind::Auto(false),
        ColorKind::Auto(true),
    ]
    .into_iter()
    .flat_map(|kind| {
        [None, Some(0.0), Some(1.0)]
            .into_iter()
            .map(move |tint| (kind.clone(), tint))
    })
    .map(|(kind, tint)| {
        CellValue::RichText(Box::new(RichText {
            runs: vec![RichTextRun {
                text: "color".into(),
                font: Some(Box::new(RunFont {
                    color: Some(Color { kind, tint }),
                    ..Default::default()
                })),
            }],
            ..Default::default()
        }))
    })
    .collect();
    let expected = row(0, values);
    let mut writer = WorkbookWriter::new(WriteOptions::default()).unwrap();
    writer.start_sheet("Sheet").unwrap();
    writer.write_row(&expected).unwrap();
    let mut book = WorkbookReader::new(writer.finish(Cursor::new(Vec::new())).unwrap()).unwrap();
    let actual = book
        .rows_with_options(
            "Sheet",
            ReadOptions {
                rich_text: true,
                ..Default::default()
            },
        )
        .unwrap()
        .next_row()
        .unwrap()
        .unwrap();
    assert_eq!(actual.cells, expected.cells);
}

fn complete_style() -> crabxl_core::CellStyle {
    use crabxl_core::*;
    let mut style = CellStyle {
        number_format: "0.00".into(),
        font: Font {
            name: Some("A".repeat(80).into()),
            size: Some(12.5),
            bold: Some(false),
            italic: Some(true),
            strike: Some(true),
            outline: Some(false),
            shadow: Some(true),
            condense: Some(false),
            extend: Some(true),
            underline: Some(Underline::DoubleAccounting),
            vertical: Some(TextVerticalAlignment::Subscript),
            charset: Some(128.into()),
            family: Some(3.0),
            scheme: Some(FontScheme::Major),
            color: Some(Color {
                kind: ColorKind::Argb(0x80445566),
                tint: Some(-0.25),
            }),
        },
        fill: Fill::Gradient(GradientFill {
            kind: Some(GradientKind::Path),
            degree: Some(35.0),
            edges: [Some(0.1), Some(0.2), Some(0.3), Some(0.4)],
            stops: vec![
                GradientStop {
                    position: 0.0,
                    color: Color {
                        kind: ColorKind::Argb(0x80AABBCC),
                        tint: None,
                    },
                },
                GradientStop {
                    position: 1.0,
                    color: Color {
                        kind: ColorKind::Indexed(64.into()),
                        tint: Some(0.0),
                    },
                },
            ],
        }),
        borders: Border {
            sides: [const { None }; 9],
            diagonal_up: Some(true),
            diagonal_down: Some(false),
            outline: Some(false),
        },
        alignment: Alignment {
            horizontal: Some(HorizontalAlignment::Distributed),
            vertical: Some(VerticalAlignment::Justify),
            rotation: Some(255),
            wrap_text: Some(true),
            shrink_to_fit: Some(true),
            indent: Some(2.5),
            relative_indent: Some(-1.5),
            reading_order: Some(2.0),
            justify_last_line: Some(true),
            merge_cell: None,
        },
        protection: Protection {
            locked: Some(false),
            hidden: Some(true),
        },
    };
    for (i, line) in [
        BorderLine::Thin,
        BorderLine::Medium,
        BorderLine::Thick,
        BorderLine::Dashed,
        BorderLine::SlantDashDot,
        BorderLine::MediumDashDot,
        BorderLine::MediumDashDotDot,
        BorderLine::Hair,
        BorderLine::None,
    ]
    .into_iter()
    .enumerate()
    {
        style.borders.sides[i] = Some(BorderSide {
            line: Some(line),
            color: Some(Color {
                kind: ColorKind::Auto(i % 2 == 0),
                tint: Some(0.25),
            }),
        });
    }
    style
}

#[test]
fn complete_style_components_round_trip_through_shared_catalog_without_flattening() {
    let mut writer = WorkbookWriter::new(WriteOptions::default()).unwrap();
    let expected = complete_style();
    let id = writer.register_style(expected.clone()).unwrap();
    assert_eq!(writer.register_style(expected.clone()).unwrap(), id);
    writer.start_sheet("Sheet").unwrap();
    let mut input = row(0, vec![CellValue::Number(1.5)]);
    input.cells[0].style = id;
    writer.write_row(&input).unwrap();
    let mut book = WorkbookReader::new(writer.finish(Cursor::new(Vec::new())).unwrap()).unwrap();
    let actual = book.rows("Sheet").unwrap().next_row().unwrap().unwrap();
    assert_eq!(actual.cells, input.cells);
    let catalog = book.style_catalog().unwrap().unwrap();
    let record = catalog.cell_format(id).unwrap();
    assert_eq!(&catalog.fonts[record.font_id as usize], &expected.font);
    assert_eq!(&catalog.fills[record.fill_id as usize], &expected.fill);
    assert_eq!(
        &catalog.borders[record.border_id as usize],
        &expected.borders
    );
    assert_eq!(record.alignment.as_deref(), Some(&expected.alignment));
    assert_eq!(record.protection, Some(expected.protection));
    assert_eq!(catalog.number_format(record.number_format_id), Some("0.00"));
}

#[test]
fn malformed_complete_styles_fail_before_registration_or_spooling() {
    let mut writer = WorkbookWriter::new(WriteOptions::default()).unwrap();
    writer.start_sheet("Sheet").unwrap();
    let before = writer.temporary_bytes();
    for kind in 0..5 {
        let mut style = complete_style();
        match kind {
            0 => style.font.size = Some(f64::NAN),
            1 => {
                let crabxl_core::Fill::Gradient(v) = &mut style.fill else {
                    unreachable!()
                };
                v.stops[1].position = 0.0;
            }
            2 => style.alignment.relative_indent = Some(-256.0),
            3 => style.font.family = Some(15.0),
            _ => {
                style.borders.sides[0]
                    .as_mut()
                    .unwrap()
                    .color
                    .as_mut()
                    .unwrap()
                    .tint = Some(2.0)
            }
        }
        assert_eq!(
            writer.register_style(style).unwrap_err().kind(),
            ErrorKind::InvalidData
        );
        assert_eq!(writer.temporary_bytes(), before);
    }
    let id = writer.register_style(complete_style()).unwrap();
    assert_eq!(id.get(), 5);
    let mut input = row(0, vec![CellValue::Integer(1)]);
    input.cells[0].style = id;
    writer.write_row(&input).unwrap();
    assert!(writer.finish(Cursor::new(Vec::new())).is_ok());
}

#[test]
fn iso_creation_preserves_date_kind_calendar_day_and_truncated_fraction() {
    use crabxl_core::{DateKind, ExcelDateTime};
    for mac in [false, true] {
        let mut writer = WorkbookWriter::new(WriteOptions {
            iso_dates: true,
            date_1904: mac,
            ..Default::default()
        })
        .unwrap();
        writer.start_sheet("Sheet").unwrap();
        let input = row(
            0,
            vec![
                CellValue::DateTime(Box::new(ExcelDateTime::from_ymd(1899, 12, 31).unwrap())),
                CellValue::DateTime(Box::new(
                    ExcelDateTime::from_ymd_hms_micro(2024, 2, 29, 12, 3, 4, 123456).unwrap(),
                )),
                CellValue::DateTime(Box::new(
                    ExcelDateTime::from_hms_micro(12, 3, 4, 123456).unwrap(),
                )),
                CellValue::DateTime(Box::new(
                    ExcelDateTime::from_duration_parts(1, 0, 0).unwrap(),
                )),
            ],
        );
        writer.write_row(&input).unwrap();
        let mut book =
            WorkbookReader::new(writer.finish(Cursor::new(Vec::new())).unwrap()).unwrap();
        let loaded = book.read_sheet("Sheet").unwrap();
        let expected = [
            (DateKind::Date, "1899-12-31"),
            (DateKind::DateTime, "2024-02-29T12:03:04.123"),
            (DateKind::Time, "12:03:04.123"),
        ];
        for (cell, (kind, iso)) in loaded.rows[0].cells.iter().zip(expected) {
            let CellValue::DateTime(value) = &cell.value else {
                panic!("Expected calendar/clock")
            };
            assert_eq!(value.kind(), kind);
            assert_eq!(value.to_iso8601().unwrap(), iso);
        }
        let CellValue::DateTime(duration) = &loaded.rows[0].cells[3].value else {
            panic!("Expected duration")
        };
        assert_eq!(duration.kind(), DateKind::Duration);
        assert_eq!(duration.to_duration().unwrap().num_seconds(), 86400);
        assert_eq!(loaded.rows[0].cells[0].style.get(), 4);
    }
}

#[test]
fn iso_payload_limits_fail_before_spooling_and_allow_retry() {
    use crabxl_core::ExcelDateTime;
    let directory = tempfile::tempdir().unwrap();
    let mut writer = WorkbookWriter::new(WriteOptions {
        iso_dates: true,
        max_cell_bytes: 12,
        ..options(&directory)
    })
    .unwrap();
    writer.start_sheet("Sheet").unwrap();
    let before = writer.temporary_bytes();
    let datetime = CellValue::DateTime(Box::new(
        ExcelDateTime::from_ymd_hms_micro(2024, 1, 1, 0, 0, 0, 123456).unwrap(),
    ));
    assert_eq!(
        writer
            .write_row(&row(0, vec![datetime]))
            .unwrap_err()
            .kind(),
        ErrorKind::LimitExceeded
    );
    assert_eq!(writer.temporary_bytes(), before);
    assert_eq!(writer.stats().rows, 0);
    let date = CellValue::DateTime(Box::new(ExcelDateTime::from_ymd(2024, 1, 1).unwrap()));
    writer.write_row(&row(0, vec![date])).unwrap();
    writer.finish(Cursor::new(Vec::new())).unwrap();
    assert_eq!(files(&directory), 0);
}

#[test]
fn array_table_metadata_and_verbatim_source_formulas_round_trip() {
    use crabxl_core::{
        DataTableOptions, Formula, FormulaFlag, FormulaFlags, FormulaMetadata, FormulaRange,
        FormulaType,
    };
    let array = Formula::with_metadata(
        "=SUM(C1:C2)",
        Some(CellValue::Integer(5)),
        FormulaMetadata {
            kind: FormulaType::Array,
            reference: Some(FormulaRange::from_xml("$A$1:$B$2").unwrap()),
            flags: FormulaFlags {
                always_calculate: Some(FormulaFlag::new(false)),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
    let table = Formula::with_metadata(
        "",
        Some(CellValue::Integer(0)),
        FormulaMetadata {
            literal_array_text: false,
            annotations: None,
            kind: FormulaType::DataTable,
            reference: Some(FormulaRange::from_xml("D1:E2").unwrap()),
            flags: FormulaFlags {
                calculate_cell: Some(FormulaFlag::from_xml("false").unwrap()),
                ..Default::default()
            },
            data_table: Some(Box::new(DataTableOptions {
                two_dimensions: Some(true.into()),
                row_table: Some(false.into()),
                input1: Some("$A$1".into()),
                input2: Some("B1".into()),
                deleted1: Some(false.into()),
                deleted2: Some(true.into()),
            })),
        },
    )
    .unwrap();
    let mut writer = WorkbookWriter::new(WriteOptions::default()).unwrap();
    writer.start_sheet("Sheet").unwrap();
    let mut input = row(
        0,
        vec![
            CellValue::Formula(Box::new(array)),
            CellValue::Formula(Box::new(table)),
            CellValue::Formula(Box::new(Formula::from_source("=1", None, None).unwrap())),
            CellValue::Formula(Box::new(Formula::from_source("", None, None).unwrap())),
        ],
    );
    input.cells[1].address = CellAddress::new(0, 3).unwrap();
    input.cells[2].address = CellAddress::new(0, 5).unwrap();
    input.cells[3].address = CellAddress::new(0, 6).unwrap();
    writer.write_row(&input).unwrap();
    let mut book = WorkbookReader::new(writer.finish(Cursor::new(Vec::new())).unwrap()).unwrap();
    let loaded = book.read_sheet("Sheet").unwrap();
    let get = |index: usize| {
        let CellValue::Formula(value) = &loaded.rows[0].cells[index].value else {
            panic!("Expected formula")
        };
        value
    };
    assert_eq!(get(0).formula_type(), FormulaType::Array);
    assert_eq!(get(0).expression(), "SUM(C1:C2)");
    assert_eq!(get(0).cached(), Some(&CellValue::Integer(5)));
    assert_eq!(
        get(0)
            .metadata()
            .unwrap()
            .reference
            .as_ref()
            .unwrap()
            .spelling(),
        "$A$1:$B$2"
    );
    let table = get(1);
    assert_eq!(table.formula_type(), FormulaType::DataTable);
    assert_eq!(table.cached(), Some(&CellValue::Integer(0)));
    assert_eq!(
        table
            .metadata()
            .unwrap()
            .flags
            .calculate_cell
            .as_ref()
            .unwrap()
            .source(),
        Some("false")
    );
    assert_eq!(
        table
            .metadata()
            .unwrap()
            .data_table
            .as_ref()
            .unwrap()
            .input1
            .as_deref(),
        Some("$A$1")
    );
    assert_eq!(get(2).expression(), "=1");
    assert_eq!(get(3).expression(), "");
}

#[test]
fn structured_formula_payload_and_input_validation_are_atomic() {
    use crabxl_core::{DataTableOptions, Formula, FormulaFlag, FormulaMetadata, FormulaType};
    let literal = Formula::with_metadata(
        "",
        None,
        FormulaMetadata {
            kind: FormulaType::DataTable,
            data_table: Some(Box::new(DataTableOptions {
                input1: Some("XFE1".into()),
                ..Default::default()
            })),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        literal
            .metadata()
            .unwrap()
            .data_table
            .as_ref()
            .unwrap()
            .validate_inputs()
            .is_err()
    );
    assert!(FormulaFlag::from_xml("\u{000b}true").is_err());
    let directory = tempfile::tempdir().unwrap();
    let mut writer = WorkbookWriter::new(WriteOptions {
        max_cell_bytes: 64,
        ..options(&directory)
    })
    .unwrap();
    writer.start_sheet("Sheet").unwrap();
    let before = writer.temporary_bytes();
    let large = Formula::with_metadata(
        "1",
        None,
        FormulaMetadata {
            flags: crabxl_core::FormulaFlags {
                calculate_cell: Some(
                    FormulaFlag::from_xml(format!("{}true", " ".repeat(1024))).unwrap(),
                ),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        writer
            .write_row(&row(0, vec![CellValue::Formula(Box::new(large))]))
            .unwrap_err()
            .kind(),
        ErrorKind::LimitExceeded
    );
    assert_eq!(writer.temporary_bytes(), before);
    writer
        .write_row(&row(0, vec![CellValue::Integer(1)]))
        .unwrap();
    writer.finish(Cursor::new(Vec::new())).unwrap();
    assert_eq!(files(&directory), 0);
}

#[test]
fn compatible_nonfinite_numbers_emit_blank_values_without_losing_formulas() {
    use crabxl_core::{Formula, ReadOptions};
    let directory = tempfile::tempdir().unwrap();
    let mut writer = WorkbookWriter::new(options(&directory)).unwrap();
    writer.start_sheet("Sheet").unwrap();
    writer
        .write_row(&row(
            0,
            vec![
                CellValue::Number(f64::NAN),
                CellValue::Number(f64::INFINITY),
                CellValue::Number(f64::NEG_INFINITY),
                CellValue::Formula(Box::new(
                    Formula::new("=1", Some(CellValue::Number(f64::INFINITY))).unwrap(),
                )),
            ],
        ))
        .unwrap();
    let output = writer.finish(Cursor::new(Vec::new())).unwrap();
    let mut archive = zip::ZipArchive::new(output.clone()).unwrap();
    let mut xml = String::new();
    use std::io::Read;
    archive
        .by_name("xl/worksheets/sheet1.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    assert_eq!(xml.matches("<v></v>").count(), 4);
    assert!(!xml.contains("NaN"));
    assert!(!xml.contains("inf"));
    let mut reader = WorkbookReader::new(output).unwrap();
    let sheet = reader.read_sheet("Sheet").unwrap();
    for cell in &sheet.rows[0].cells[..3] {
        assert_eq!(cell.value, CellValue::Empty);
    }
    let CellValue::Formula(formula) = &sheet.rows[0].cells[3].value else {
        panic!("Expected formula");
    };
    assert_eq!(formula.expression(), "1");
    assert_eq!(formula.cached(), Some(&CellValue::Empty));
    let cached = reader
        .read_sheet_with_options(
            "Sheet",
            ReadOptions {
                data_only: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(
        cached.rows[0]
            .cells
            .iter()
            .all(|cell| cell.value == CellValue::Empty)
    );
    assert_eq!(files(&directory), 0);
}

#[test]
fn normalized_writer_components_use_actual_font_ids_and_release_on_abort() {
    let mut writer = WorkbookWriter::new(WriteOptions::default()).unwrap();
    for code in ["0.000", "0.0000"] {
        writer
            .register_style(crabxl_core::CellStyle {
                number_format: code.into(),
                ..Default::default()
            })
            .unwrap();
    }
    let catalog = writer.style_catalog().unwrap();
    assert_eq!(catalog.fonts.len(), 1);
    assert_eq!(catalog.fills.len(), 2);
    assert_eq!(catalog.borders.len(), 1);
    assert_eq!(catalog.cell_formats.len(), 7);
    assert_eq!(catalog.number_formats.len(), 5);
    writer.start_sheet("Sheet").unwrap();
    let before = writer.temporary_bytes();
    let CellValue::RichText(mut value) = rich_value() else {
        unreachable!();
    };
    value.phonetic_properties.as_mut().unwrap().font_id = 6;
    assert_eq!(
        writer
            .write_row(&row(0, vec![CellValue::RichText(value)]))
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidData
    );
    assert_eq!(writer.temporary_bytes(), before);
    writer.write_row(&row(0, vec![rich_value()])).unwrap();
    assert!(writer.style_memory_bytes() > 0);
    writer.abort().unwrap();
    assert!(writer.style_catalog().is_none());
    assert_eq!(writer.style_memory_bytes(), 0);
}

#[test]
fn registered_font_attributes_stream_escaped_payloads_and_recover_original_text() {
    let name = "&<\"'\r\n\t".repeat(2000);
    let mut style = crabxl_core::CellStyle::default();
    style.font.name = Some(name.clone().into());
    let mut writer = WorkbookWriter::new(WriteOptions::default()).unwrap();
    let id = writer.register_style(style).unwrap();
    writer.start_sheet("Sheet").unwrap();
    let mut input = row(0, vec![CellValue::Integer(1)]);
    input.cells[0].style = id;
    writer.write_row(&input).unwrap();
    let output = writer.finish(Cursor::new(Vec::new())).unwrap();
    let mut bounded = WorkbookReader::new(output.clone()).unwrap();
    assert_eq!(
        bounded.style_catalog().unwrap_err().kind(),
        ErrorKind::LimitExceeded
    );
    let mut book = WorkbookReader::with_limits(
        output,
        crabxl_core::ResourceLimits {
            max_xml_event_bytes: 128 * 1024,
            ..Default::default()
        },
    )
    .unwrap();
    let catalog = book.style_catalog().unwrap().unwrap();
    assert_eq!(
        catalog.cell_style(id).unwrap().font.name.as_deref(),
        Some(name.as_str())
    );
}

#[test]
fn alignment_zero_serialization_has_compatible_and_explicit_policies() {
    use crabxl_core::{Alignment, CellStyle};
    for policy in [
        crabxl_xlsx::StyleWritePolicy::Compatible,
        crabxl_xlsx::StyleWritePolicy::RetainExplicit,
    ] {
        let mut writer = WorkbookWriter::new(WriteOptions {
            style_attributes: policy,
            ..Default::default()
        })
        .unwrap();
        let alignment = Alignment {
            rotation: Some(0),
            wrap_text: Some(false),
            shrink_to_fit: Some(false),
            indent: Some(0.0),
            relative_indent: Some(-0.0),
            reading_order: Some(0.0),
            justify_last_line: Some(false),
            ..Default::default()
        };
        let id = writer
            .register_style(CellStyle {
                alignment: alignment.clone(),
                ..Default::default()
            })
            .unwrap();
        writer.start_sheet("Sheet").unwrap();
        let mut input = row(0, vec![CellValue::Integer(1)]);
        input.cells[0].style = id;
        writer.write_row(&input).unwrap();
        let mut reader =
            WorkbookReader::new(writer.finish(Cursor::new(Vec::new())).unwrap()).unwrap();
        let catalog = reader.style_catalog().unwrap().unwrap();
        let loaded = catalog.cell_style(id).unwrap().alignment.unwrap();
        if policy == crabxl_xlsx::StyleWritePolicy::Compatible {
            assert_eq!(loaded, &Alignment::default());
        } else {
            assert_eq!(loaded, &alignment);
        }
    }
}

#[test]
fn public_font_domains_and_case_sensitive_colors_roundtrip() {
    use crabxl_core::{ArgbLiteral, CellStyle, Color, ColorKind};
    let mut writer = WorkbookWriter::new(WriteOptions::default()).unwrap();
    let mut ids = Vec::new();
    let mut cases: Vec<_> = [
        (-1, ColorKind::Theme((-1).into())),
        (256, ColorKind::Indexed((-1).into())),
        (4096, ArgbLiteral::parse("aaBbCcDd").unwrap().into_kind()),
    ]
    .into_iter()
    .map(|(charset, kind)| (crabxl_core::StyleInteger::from(charset), kind))
    .collect();
    for literal in [
        "9223372036854775808",
        "-9223372036854775809",
        "10000000000000000000000000000000000000000",
    ] {
        let value = crabxl_core::StyleInteger::parse(literal).unwrap();
        cases.push((value.clone(), ColorKind::Theme(value.clone())));
        cases.push((value.clone(), ColorKind::Indexed(value)));
    }
    for (charset, kind) in cases {
        let mut style = CellStyle::default();
        style.font.family = Some(2.5);
        style.font.charset = Some(charset.clone());
        let color = Color {
            kind: kind.clone(),
            tint: None,
        };
        style.font.color = Some(color.clone());
        style.fill = crabxl_core::Fill::solid(color.clone());
        style.borders.sides[0].as_mut().unwrap().color = Some(color);
        ids.push((writer.register_style(style).unwrap(), charset, kind));
    }
    writer.start_sheet("Sheet").unwrap();
    for (index, (id, _, _)) in ids.iter().enumerate() {
        let mut value = row(index as u32, vec![CellValue::Integer(1)]);
        value.cells[0].style = *id;
        writer.write_row(&value).unwrap();
    }
    let output = writer.finish(Cursor::new(Vec::new())).unwrap();
    let mut reader = WorkbookReader::new(Cursor::new(output.into_inner())).unwrap();
    let catalog = reader.style_catalog().unwrap().unwrap();
    for (id, charset, kind) in ids {
        let font = catalog.cell_style(id).unwrap().font;
        assert_eq!(font.family, Some(2.5));
        assert_eq!(font.charset, Some(charset.clone()));
        assert_eq!(font.color.as_ref().unwrap().kind, kind);
        let view = catalog.cell_style(id).unwrap();
        let crabxl_core::Fill::Pattern(fill) = view.fill else {
            panic!("Expected pattern fill")
        };
        assert_eq!(fill.foreground.as_ref().unwrap().kind, kind);
        assert_eq!(
            view.border.sides[0]
                .as_ref()
                .unwrap()
                .color
                .as_ref()
                .unwrap()
                .kind,
            kind
        );
    }
}

#[test]
fn themes_default_opaque_validated_omitted_and_abort_ownership() {
    use crabxl_core::Theme;
    use crabxl_xlsx::ThemeWritePolicy;
    let custom = b"<a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" name=\"custom\"><a:extLst/></a:theme>";
    for policy in [
        ThemeWritePolicy::ReferenceDefault,
        ThemeWritePolicy::Custom(Theme::from_bytes(custom.to_vec().into_boxed_slice())),
        ThemeWritePolicy::Validated(Theme::from_bytes(custom.to_vec().into_boxed_slice())),
        ThemeWritePolicy::Omit,
    ] {
        let omitted = matches!(policy, ThemeWritePolicy::Omit);
        let default = matches!(policy, ThemeWritePolicy::ReferenceDefault);
        let mut writer = WorkbookWriter::new(WriteOptions {
            theme: policy,
            ..Default::default()
        })
        .unwrap();
        writer.start_sheet("Sheet").unwrap();
        writer
            .write_row(&row(0, vec![CellValue::Integer(1)]))
            .unwrap();
        let output = writer.finish(Cursor::new(Vec::new())).unwrap();
        let mut book = WorkbookReader::new(Cursor::new(output.into_inner())).unwrap();
        assert_eq!(book.theme_memory_bytes(), 0);
        let theme = book.theme().unwrap();
        if omitted {
            assert!(theme.is_none());
        } else if default {
            assert!(theme.unwrap().bytes().starts_with(b"<?xml"));
        } else {
            assert_eq!(theme.unwrap().bytes(), custom);
        }
        let bytes = book.theme_memory_bytes();
        assert_eq!(book.theme_memory_bytes(), bytes);
        book.validate_theme().unwrap();
        assert_eq!(
            book.rows("Sheet")
                .unwrap()
                .next_row()
                .unwrap()
                .unwrap()
                .cells[0]
                .value,
            CellValue::Integer(1)
        );
    }
    let malformed = Theme::from_bytes(b"not XML".to_vec().into_boxed_slice());
    assert!(
        WorkbookWriter::new(WriteOptions {
            theme: ThemeWritePolicy::Validated(malformed.clone()),
            ..Default::default()
        })
        .is_err()
    );
    let mut writer = WorkbookWriter::new(WriteOptions {
        theme: ThemeWritePolicy::Custom(malformed),
        ..Default::default()
    })
    .unwrap();
    writer.start_sheet("Sheet").unwrap();
    let output = writer.finish(Cursor::new(Vec::new())).unwrap();
    let mut book = WorkbookReader::new(Cursor::new(output.into_inner())).unwrap();
    assert_eq!(book.theme().unwrap().unwrap().bytes(), b"not XML");
    assert!(book.validate_theme().is_err());
    let mut writer = WorkbookWriter::new(WriteOptions {
        theme: ThemeWritePolicy::Custom(Theme::from_bytes(custom.to_vec().into_boxed_slice())),
        ..Default::default()
    })
    .unwrap();
    assert!(writer.theme_memory_bytes() > 0);
    writer.abort().unwrap();
    assert_eq!(writer.theme_memory_bytes(), 0);
    assert_eq!(writer.style_memory_bytes(), 0);
    assert!(writer.start_sheet("after abort").is_err());
}

#[test]
fn theme_and_styles_share_metadata_input_allowance_in_either_order() {
    let mut writer = WorkbookWriter::new(WriteOptions::default()).unwrap();
    writer.start_sheet("Sheet").unwrap();
    let output = writer.finish(Cursor::new(Vec::new())).unwrap().into_inner();
    let mut archive = zip::ZipArchive::new(Cursor::new(&output)).unwrap();
    let total: u64 = (0..archive.len())
        .map(|index| {
            let file = archive.by_index(index).unwrap();
            if file.name().starts_with("xl/worksheets/") {
                0
            } else {
                file.size()
            }
        })
        .sum();
    for theme_first in [true, false] {
        let mut book = WorkbookReader::with_limits(
            Cursor::new(&output),
            crabxl_core::ResourceLimits {
                max_metadata_bytes: total - 1,
                ..Default::default()
            },
        )
        .unwrap();
        if theme_first {
            assert!(book.theme().unwrap().is_some());
            assert_eq!(
                book.style_catalog().unwrap_err().kind(),
                ErrorKind::LimitExceeded
            );
            assert_eq!(book.style_memory_bytes(), 0);
        } else {
            assert!(book.style_catalog().unwrap().is_some());
            assert_eq!(book.theme().unwrap_err().kind(), ErrorKind::LimitExceeded);
            assert_eq!(book.theme_memory_bytes(), 0);
        }
    }
}

#[test]
fn formula_attribute_policies_distinguish_owned_false_from_source_spellings() {
    use crabxl_core::{DataTableOptions, Formula, FormulaFlag, FormulaMetadata, FormulaType};
    use crabxl_xlsx::FormulaWritePolicy;
    use std::io::Read;
    let formula = Formula::with_metadata(
        "",
        None,
        FormulaMetadata {
            kind: FormulaType::DataTable,
            reference: Some(crabxl_core::FormulaRange::from_xml("A1:B2").unwrap()),
            flags: crabxl_core::FormulaFlags {
                calculate_cell: Some(FormulaFlag::from_literal("")),
                ..Default::default()
            },
            data_table: Some(Box::new(DataTableOptions {
                two_dimensions: Some(FormulaFlag::new(false)),
                row_table: Some(FormulaFlag::from_xml("0").unwrap()),
                deleted1: Some(FormulaFlag::from_xml("false").unwrap()),
                deleted2: Some(FormulaFlag::new(true)),
                input1: Some("".into()),
                input2: Some("B1".into()),
            })),
            ..Default::default()
        },
    )
    .unwrap();
    for policy in [
        FormulaWritePolicy::Compatible,
        FormulaWritePolicy::RetainExplicit,
    ] {
        let mut writer = WorkbookWriter::new(WriteOptions {
            formula_attributes: policy,
            ..Default::default()
        })
        .unwrap();
        writer.start_sheet("Sheet").unwrap();
        writer
            .write_row(&row(0, vec![CellValue::Formula(Box::new(formula.clone()))]))
            .unwrap();
        let output = writer.finish(Cursor::new(Vec::new())).unwrap();
        let mut archive = zip::ZipArchive::new(output).unwrap();
        let mut xml = String::new();
        archive
            .by_name("xl/worksheets/sheet1.xml")
            .unwrap()
            .read_to_string(&mut xml)
            .unwrap();
        assert_eq!(
            xml.contains("ca=\"\""),
            policy == FormulaWritePolicy::RetainExplicit
        );
        assert!(xml.contains("dtr=\"0\""));
        assert!(xml.contains("del1=\"false\""));
        assert!(xml.contains("del2=\"1\""));
        assert!(xml.contains("r2=\"B1\""));
        assert_eq!(
            xml.contains("dt2D=\"0\""),
            policy == FormulaWritePolicy::RetainExplicit
        );
        assert_eq!(
            xml.contains("r1=\"\""),
            policy == FormulaWritePolicy::RetainExplicit
        );
    }
}

#[test]
fn owned_bank_theme_exports_without_payload_copy_and_preflights_strict_validation() {
    use crabxl_core::{Theme, Workbook, WorkbookLimits};
    use crabxl_xlsx::ThemeWritePolicy;
    let custom = Theme::from_bytes(b"opaque shared theme".to_vec().into_boxed_slice());
    let mut bank = Workbook::new(WorkbookLimits::default()).unwrap();
    bank.create_sheet("Sheet").unwrap();
    bank.set_theme(Some(custom.clone())).unwrap();
    let mut writer = WorkbookWriter::new(WriteOptions::default()).unwrap();
    writer.write_workbook(&bank).unwrap();
    let output = writer.finish(Cursor::new(Vec::new())).unwrap();
    let mut reader = WorkbookReader::new(output).unwrap();
    assert_eq!(reader.theme().unwrap().unwrap().bytes(), custom.bytes());
    let mut strict = WorkbookWriter::new(WriteOptions {
        theme: ThemeWritePolicy::Validated(Theme::from_bytes(
            b"<theme xmlns=\"http://schemas.openxmlformats.org/drawingml/2006/main\"/>"
                .to_vec()
                .into_boxed_slice(),
        )),
        ..Default::default()
    })
    .unwrap();
    assert!(strict.write_workbook(&bank).is_err());
    assert_eq!(strict.stats().rows, 0);
    assert_eq!(strict.temporary_bytes(), 0);
    bank.set_theme(None).unwrap();
    strict.write_workbook(&bank).unwrap();
    strict.finish(Cursor::new(Vec::new())).unwrap();
}

#[test]
fn imported_style_ids_and_zero_date_formats_survive_export_with_derived_auto_ids() {
    use crabxl_core::{DateEpoch, DateKind, ExcelDateTime, StyleId, StyleLimits, StyleRegistry};
    for zero_is_date in [false, true] {
        let mut catalog = StyleRegistry::new(StyleLimits::default())
            .unwrap()
            .catalog()
            .clone();
        catalog.fonts[0].name = Some("Source font".into());
        let mut second = catalog.fonts[0].clone();
        second.name = Some("Unchanged font".into());
        catalog.fonts.push(second);
        let mut format = catalog.cell_formats[0].clone();
        format.font_id = 1;
        catalog.cell_formats.push(format);
        if zero_is_date {
            catalog.cell_formats[0].number_format_id = 14;
        }
        let prefix = catalog.cell_formats.clone();
        let mut writer =
            WorkbookWriter::from_style_catalog(WriteOptions::default(), catalog).unwrap();
        assert_eq!(writer.style_catalog().unwrap().fonts.len(), 2);
        assert_eq!(
            &writer.style_catalog().unwrap().cell_formats[..2],
            prefix.as_slice()
        );
        writer.start_sheet("Sheet").unwrap();
        let mut values = row(
            0,
            vec![
                CellValue::Integer(1),
                CellValue::text("stable"),
                CellValue::DateTime(Box::new(
                    ExcelDateTime::from_serial(
                        43831.25,
                        DateEpoch::Windows1900,
                        DateKind::DateTime,
                    )
                    .unwrap(),
                )),
                CellValue::DateTime(Box::new(
                    ExcelDateTime::from_serial(0.5, DateEpoch::Windows1900, DateKind::Time)
                        .unwrap(),
                )),
                CellValue::DateTime(Box::new(
                    ExcelDateTime::from_serial(1.25, DateEpoch::Windows1900, DateKind::Duration)
                        .unwrap(),
                )),
            ],
        );
        values.cells[1].style = StyleId::new(1);
        writer.write_row(&values).unwrap();
        let mut reader =
            WorkbookReader::new(writer.finish(Cursor::new(Vec::new())).unwrap()).unwrap();
        assert_eq!(
            &reader.style_catalog().unwrap().unwrap().cell_formats[..2],
            prefix.as_slice()
        );
        let output = reader.read_sheet("Sheet").unwrap();
        assert_eq!(output.rows[0].cells[1].style.get(), 1);
        assert_eq!(output.rows[0].cells[2].style.get() == 0, zero_is_date);
        assert_eq!(output.rows[0].cells[3].style.get() == 0, zero_is_date);
        assert_eq!(output.rows[0].cells[4].style.get() == 0, zero_is_date);
        let CellValue::DateTime(date) = &output.rows[0].cells[2].value else {
            panic!("Missing date")
        };
        assert_eq!(
            date.to_datetime().unwrap().to_string(),
            "2020-01-01 06:00:00"
        );
        let CellValue::DateTime(time) = &output.rows[0].cells[3].value else {
            panic!("Missing time")
        };
        assert_eq!(time.to_time().unwrap().to_string(), "12:00:00");
        let CellValue::DateTime(duration) = &output.rows[0].cells[4].value else {
            panic!("Missing duration")
        };
        if zero_is_date {
            assert_eq!(
                duration.to_datetime().unwrap().to_string(),
                "1900-01-01 06:00:00"
            );
        } else {
            assert_eq!(duration.to_duration().unwrap().num_seconds(), 108000);
        }
    }
}

#[test]
fn source_catalog_creation_rejects_unmodeled_sections_and_accepts_large_configured_counts() {
    use crabxl_core::{StyleLimits, StyleRegistry};
    let mut catalog = StyleRegistry::new(StyleLimits::default())
        .unwrap()
        .catalog()
        .clone();
    catalog.unmodeled_sections.push("dxfs".into());
    assert!(
        matches!(WorkbookWriter::from_style_catalog(WriteOptions::default(), catalog), Err(e) if e.kind() == ErrorKind::Unsupported)
    );
    let writer = WorkbookWriter::new(WriteOptions {
        max_styles: 100000,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(writer.style_catalog().unwrap().cell_formats.len(), 5);
}

#[test]
fn consuming_bank_export_transfers_style_ids_components_and_theme() {
    use crabxl_core::{CellStyle, Theme, Workbook, WorkbookLimits};
    let mut bank = Workbook::new(WorkbookLimits::default()).unwrap();
    let first = bank.create_sheet("First").unwrap();
    let other = bank.create_sheet("Other").unwrap();
    let mut style = CellStyle::default();
    style.font.name = Some("OwnedBank".into());
    style.font.size = Some(410.0);
    style.number_format = "0.000".into();
    let id = bank.register_style(style).unwrap();
    let font_pointer = bank.style_catalog().unwrap().fonts[1]
        .name
        .as_ref()
        .unwrap()
        .as_ptr();
    bank.sheet_mut(first)
        .unwrap()
        .set(crabxl_core::Cell {
            address: crabxl_core::CellAddress::new(0, 0).unwrap(),
            value: CellValue::Number(1.25),
            style: id,
        })
        .unwrap();
    bank.sheet_mut(other)
        .unwrap()
        .append(vec![CellValue::Integer(2)])
        .unwrap();
    bank.set_theme(Some(Theme::from_bytes(
        b"opaque-custom-theme".to_vec().into_boxed_slice(),
    )))
    .unwrap();
    bank.move_sheet(other, 0).unwrap();
    bank.set_active_sheet(first).unwrap();
    bank.set_epoch(crabxl_core::DateEpoch::Mac1904);
    let mut legacy = WorkbookWriter::new(WriteOptions::default()).unwrap();
    assert!(legacy.write_workbook(&bank).is_err());
    assert_eq!(legacy.temporary_bytes(), 0);
    let writer = WorkbookWriter::from_workbook(WriteOptions::default(), bank).unwrap();
    assert_eq!(
        writer.style_catalog().unwrap().fonts[1]
            .name
            .as_ref()
            .unwrap()
            .as_ptr(),
        font_pointer
    );
    let output = writer.finish(Cursor::new(Vec::new())).unwrap();
    let mut reader = WorkbookReader::new(output).unwrap();
    assert!(reader.date_1904());
    assert_eq!(reader.active_index(), Some(1));
    assert_eq!(
        reader.theme().unwrap().unwrap().bytes(),
        b"opaque-custom-theme"
    );
    let cell = reader.read_sheet("First").unwrap().rows[0].cells[0].clone();
    assert_eq!(cell.style, id);
    assert_eq!(cell.value, CellValue::Number(1.25));
    let style = reader
        .style_catalog()
        .unwrap()
        .unwrap()
        .cell_style(id)
        .unwrap();
    assert_eq!(style.font.name.as_deref(), Some("OwnedBank"));
    assert_eq!(style.font.size, Some(410.0));
    assert_eq!(style.number_format, Some("0.000"));
}

#[test]
fn absent_array_expression_and_reference_serialize_as_empty_source_body() {
    use crabxl_core::{Formula, FormulaMetadata, FormulaType};
    let formula = Formula::with_optional_expression(
        None,
        None,
        FormulaMetadata {
            kind: FormulaType::Array,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(formula.optional_expression(), None);
    let mut writer = WorkbookWriter::new(WriteOptions::default()).unwrap();
    writer.start_sheet("Array").unwrap();
    writer
        .write_row(&row(0, vec![CellValue::Formula(Box::new(formula))]))
        .unwrap();
    let output = writer.finish(Cursor::new(Vec::new())).unwrap();
    let mut reader = WorkbookReader::new(output).unwrap();
    let values = reader.read_sheet("Array").unwrap();
    let CellValue::Formula(source) = &values.rows[0].cells[0].value else {
        panic!("Expected array");
    };
    assert_eq!(source.formula_type(), FormulaType::Array);
    assert_eq!(source.optional_expression(), Some(""));
    assert!(source.metadata().unwrap().reference.is_none());
    assert!(source.cached().is_none());
}

#[test]
fn typed_differential_table_catalogs_export_with_original_identity_and_count_properties() {
    use crabxl_core::{
        CellStyle, DifferentialStyle, NumberFormat, StyleLimits, StyleRegistry, TableStyle,
        TableStyleCatalog, TableStyleElement, TableStyleRegion,
    };
    let mut catalog = StyleRegistry::new(StyleLimits::default())
        .unwrap()
        .catalog()
        .clone();
    let mut font = CellStyle::default().font;
    font.name = Some("SparseOverride".into());
    catalog.differential_styles.push(DifferentialStyle {
        font: Some(Box::new(font)),
        number_format: Some(NumberFormat::new(500, "0.000")),
        ..Default::default()
    });
    catalog.table_styles = Some(Box::new(TableStyleCatalog {
        default_table_style: Some("Custom & Named".into()),
        styles: vec![TableStyle {
            name: "Custom & Named".into(),
            pivot: Some(false),
            table: Some(true),
            count: Some(4_000_000_000),
            elements: vec![TableStyleElement {
                region: TableStyleRegion::WholeTable,
                size: Some(0),
                differential_style_id: Some(0),
            }],
        }],
        ..Default::default()
    }));
    let expected_differentials = catalog.differential_styles.clone();
    let expected_tables = catalog.table_styles.clone();
    let pointer = catalog.differential_styles[0]
        .font
        .as_ref()
        .unwrap()
        .name
        .as_ref()
        .unwrap()
        .as_ptr();
    let mut writer = WorkbookWriter::from_style_catalog(WriteOptions::default(), catalog).unwrap();
    assert_eq!(
        writer.style_catalog().unwrap().differential_styles[0]
            .font
            .as_ref()
            .unwrap()
            .name
            .as_ref()
            .unwrap()
            .as_ptr(),
        pointer
    );
    writer.start_sheet("Sheet").unwrap();
    writer
        .write_row(&row(0, vec![CellValue::Integer(1)]))
        .unwrap();
    let mut reader = WorkbookReader::new(writer.finish(Cursor::new(Vec::new())).unwrap()).unwrap();
    let actual = reader.style_catalog().unwrap().unwrap();
    assert_eq!(actual.differential_styles, expected_differentials);
    assert_eq!(actual.table_styles, expected_tables);
    let mut invalid = actual.clone();
    invalid.differential_styles[0].unmodeled_extensions = true;
    assert!(
        matches!(WorkbookWriter::from_style_catalog(WriteOptions::default(), invalid), Err(error) if error.kind() == ErrorKind::Unsupported)
    );
}

#[test]
fn literal_empty_formula_references_follow_compatible_and_retained_policies() {
    use crabxl_core::{Formula, FormulaMetadata, FormulaReference, FormulaType};
    use crabxl_xlsx::FormulaWritePolicy;
    use std::io::Read;
    for kind in [FormulaType::Array, FormulaType::DataTable] {
        for policy in [
            FormulaWritePolicy::Compatible,
            FormulaWritePolicy::RetainExplicit,
        ] {
            let formula = Formula::with_optional_expression(
                None,
                None,
                FormulaMetadata {
                    kind: kind.clone(),
                    reference: Some(FormulaReference::from_literal("")),
                    ..Default::default()
                },
            )
            .unwrap();
            let mut writer = WorkbookWriter::new(WriteOptions {
                formula_attributes: policy,
                ..Default::default()
            })
            .unwrap();
            writer.start_sheet("Sheet").unwrap();
            writer
                .write_row(&row(0, vec![CellValue::Formula(Box::new(formula))]))
                .unwrap();
            let output = writer.finish(Cursor::new(Vec::new())).unwrap();
            let mut archive = zip::ZipArchive::new(output).unwrap();
            let mut xml = String::new();
            archive
                .by_name("xl/worksheets/sheet1.xml")
                .unwrap()
                .read_to_string(&mut xml)
                .unwrap();
            assert_eq!(
                xml.contains("ref=\"\""),
                policy == FormulaWritePolicy::RetainExplicit
            );
        }
    }
}

#[test]
fn literal_formula_metadata_xml_validation_is_atomic_before_spooling() {
    use crabxl_core::{
        DataTableOptions, Formula, FormulaFlag, FormulaMetadata, FormulaReference, FormulaType,
    };
    for property in 0..3 {
        let mut metadata = FormulaMetadata {
            kind: FormulaType::DataTable,
            ..Default::default()
        };
        match property {
            0 => metadata.reference = Some(FormulaReference::from_literal("bad\0reference")),
            1 => metadata.flags.calculate_cell = Some(FormulaFlag::from_literal("bad\0flag")),
            _ => {
                metadata.data_table = Some(Box::new(DataTableOptions {
                    input1: Some("bad\0input".into()),
                    ..Default::default()
                }))
            }
        }
        let formula = Formula::with_optional_expression(None, None, metadata).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let mut writer = WorkbookWriter::new(options(&directory)).unwrap();
        writer.start_sheet("Sheet").unwrap();
        let before = writer.temporary_bytes();
        assert_eq!(
            writer
                .write_row(&row(0, vec![CellValue::Formula(Box::new(formula))]))
                .unwrap_err()
                .kind(),
            ErrorKind::InvalidData
        );
        assert_eq!(writer.temporary_bytes(), before);
        writer
            .write_row(&row(0, vec![CellValue::Integer(1)]))
            .unwrap();
        writer.finish(Cursor::new(Vec::new())).unwrap();
    }
}

#[test]
fn derived_temporal_formats_reuse_components_and_validate_rows_before_interning() {
    use crabxl_core::{Alignment, CellStyle, ExcelDateTime, Font};
    let mut writer = WorkbookWriter::new(WriteOptions::default()).unwrap();
    let source = writer
        .register_style(CellStyle {
            font: Font {
                name: Some("Shared temporal font".into()),
                bold: Some(true),
                ..Default::default()
            },
            alignment: Alignment {
                indent: Some(2.5),
                ..Default::default()
            },
            number_format: "0.00".into(),
            ..Default::default()
        })
        .unwrap();
    writer.start_sheet("Sheet").unwrap();
    let date = CellValue::DateTime(Box::new(
        ExcelDateTime::from_ymd_hms_micro(2024, 1, 2, 3, 4, 5, 678900).unwrap(),
    ));
    let mut invalid = row(0, vec![date.clone(), CellValue::text("bad\0text")]);
    invalid.cells[0].style = source;
    let styles = writer.style_catalog().unwrap().cell_formats.len();
    let bytes = writer.style_memory_bytes();
    let temporary = writer.temporary_bytes();
    assert!(writer.write_row(&invalid).is_err());
    assert_eq!(writer.style_catalog().unwrap().cell_formats.len(), styles);
    assert_eq!(writer.style_memory_bytes(), bytes);
    assert_eq!(writer.temporary_bytes(), temporary);
    let mut first = row(0, vec![date.clone()]);
    first.cells[0].style = source;
    writer.write_row(&first).unwrap();
    assert_eq!(
        writer.style_catalog().unwrap().cell_formats.len(),
        styles + 1
    );
    let derived_bytes = writer.style_memory_bytes();
    let mut next = row(1, vec![date]);
    next.cells[0].style = source;
    writer.write_row(&next).unwrap();
    assert_eq!(writer.style_memory_bytes(), derived_bytes);
    assert_eq!(
        writer.style_catalog().unwrap().cell_formats.len(),
        styles + 1
    );
    let output = writer.finish(Cursor::new(Vec::new())).unwrap();
    let mut reader = WorkbookReader::new(output).unwrap();
    let loaded = reader.read_sheet("Sheet").unwrap();
    let derived = loaded.rows[0].cells[0].style;
    assert_eq!(loaded.rows[1].cells[0].style, derived);
    assert_ne!(derived, source);
    let catalog = reader.style_catalog().unwrap().unwrap();
    let base = catalog.cell_format(source).unwrap();
    let changed = catalog.cell_format(derived).unwrap();
    assert_eq!(changed.font_id, base.font_id);
    assert_eq!(changed.fill_id, base.fill_id);
    assert_eq!(changed.border_id, base.border_id);
    assert_eq!(changed.alignment, base.alignment);
    assert_eq!(catalog.number_format(base.number_format_id), Some("0.00"));
    assert_eq!(
        catalog.number_format(changed.number_format_id),
        Some("yyyy-mm-dd h:mm:ss")
    );
}

#[test]
fn indexed_palette_source_adoption_and_export_preserve_literal_spelling() {
    use crabxl_core::{ArgbLiteral, StyleLimits, StyleRegistry};
    let mut catalog = StyleRegistry::new(StyleLimits::default())
        .unwrap()
        .catalog()
        .clone();
    let literals = ["ff11aa22", "FF11AA22", "abc123", "AbC123", "00ff00FF"];
    catalog.indexed_colors = literals
        .iter()
        .map(|value| ArgbLiteral::parse(value).unwrap())
        .collect();
    let pointer = catalog.indexed_colors.as_ptr();
    let mut writer = WorkbookWriter::from_style_catalog(WriteOptions::default(), catalog).unwrap();
    assert_eq!(
        writer.style_catalog().unwrap().indexed_colors.as_ptr(),
        pointer
    );
    writer.start_sheet("Sheet").unwrap();
    writer
        .write_row(&row(0, vec![CellValue::Integer(1)]))
        .unwrap();
    let mut reader = WorkbookReader::new(writer.finish(Cursor::new(Vec::new())).unwrap()).unwrap();
    assert_eq!(
        reader
            .style_catalog()
            .unwrap()
            .unwrap()
            .indexed_colors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["ff11aa22", "FF11AA22", "00abc123", "00AbC123", "00ff00FF"]
    );
}

#[test]
fn default_owned_temporal_catalog_supports_repeated_borrowed_saves_without_reinterpreting_ids() {
    use crabxl_core::{CellStyle, DateKind, ExcelDateTime, Workbook, WorkbookLimits};
    let mut book = Workbook::new(WorkbookLimits::default()).unwrap();
    let sheet = book.create_sheet("Sheet").unwrap();
    book.sheet_mut(sheet)
        .unwrap()
        .append(vec![
            CellValue::DateTime(Box::new(ExcelDateTime::from_ymd(2024, 1, 2).unwrap())),
            CellValue::DateTime(Box::new(
                ExcelDateTime::from_hms_micro(3, 4, 5, 678900).unwrap(),
            )),
        ])
        .unwrap();
    let date_style = book
        .sheet(sheet)
        .unwrap()
        .get("A1".parse().unwrap())
        .unwrap()
        .style;
    let time_style = book
        .sheet(sheet)
        .unwrap()
        .get("B1".parse().unwrap())
        .unwrap()
        .style;
    assert_eq!(date_style.get(), 4);
    assert_eq!(time_style.get(), 2);
    let charged = book.charged_bytes();
    for _ in 0..2 {
        let mut writer = WorkbookWriter::new(WriteOptions::default()).unwrap();
        assert_eq!(writer.style_catalog(), book.style_catalog());
        writer.write_workbook(&book).unwrap();
        let output = writer.finish(Cursor::new(Vec::new())).unwrap();
        let mut reader = WorkbookReader::new(output).unwrap();
        let loaded = reader.read_sheet("Sheet").unwrap();
        assert_eq!(loaded.rows[0].cells[0].style, date_style);
        assert_eq!(loaded.rows[0].cells[1].style, time_style);
        assert_eq!(
            loaded.rows[0].cells[0]
                .value
                .temporal_value()
                .unwrap()
                .to_date()
                .unwrap()
                .to_string(),
            "2024-01-02"
        );
        assert_eq!(
            loaded.rows[0].cells[1]
                .value
                .temporal_value()
                .unwrap()
                .kind(),
            DateKind::Time
        );
        assert_eq!(book.charged_bytes(), charged);
    }
    let custom = book
        .register_style(CellStyle {
            number_format: "0.00000".into(),
            ..Default::default()
        })
        .unwrap();
    book.sheet_mut(sheet)
        .unwrap()
        .set_style("A1".parse().unwrap(), crabxl_core::StyleId::new(0))
        .unwrap();
    book.sheet_mut(sheet)
        .unwrap()
        .set_style("B1".parse().unwrap(), custom)
        .unwrap();
    for _ in 0..2 {
        let mut writer = WorkbookWriter::from_style_catalog(
            WriteOptions::default(),
            book.style_catalog().unwrap().clone(),
        )
        .unwrap();
        writer.write_workbook(&book).unwrap();
        let output = writer.finish(Cursor::new(Vec::new())).unwrap();
        let mut reader = WorkbookReader::new(output).unwrap();
        let loaded = reader.read_sheet("Sheet").unwrap();
        assert_eq!(loaded.rows[0].cells[0].style, crabxl_core::StyleId::new(0));
        assert_eq!(loaded.rows[0].cells[0].value, CellValue::Integer(45293));
        assert_eq!(loaded.rows[0].cells[1].style, custom);
        assert!(matches!(
            loaded.rows[0].cells[1].value,
            CellValue::Number(_)
        ));
    }
    book.sheet_mut(sheet)
        .unwrap()
        .set(Cell {
            address: "A2".parse().unwrap(),
            value: CellValue::Number(1.25),
            style: custom,
        })
        .unwrap();
    let mut mismatched = WorkbookWriter::new(WriteOptions::default()).unwrap();
    assert!(mismatched.write_workbook(&book).is_err());
    assert_eq!(mismatched.temporary_bytes(), 0);
    let writer = WorkbookWriter::from_workbook(WriteOptions::default(), book).unwrap();
    let output = writer.finish(Cursor::new(Vec::new())).unwrap();
    let mut reader = WorkbookReader::new(output).unwrap();
    let loaded = reader.read_sheet("Sheet").unwrap();
    assert_eq!(loaded.rows[0].cells[0].value, CellValue::Integer(45293));
    assert_eq!(loaded.rows[0].cells[1].style, custom);
}

#[test]
fn unresolved_literal_shared_ids_escape_and_validate_before_spooling() {
    use crabxl_core::{Formula, FormulaMetadata, FormulaType, SharedFormulaIndex, StyleId};
    let mut writer = WorkbookWriter::new(WriteOptions::default()).unwrap();
    writer.start_sheet("Sheet").unwrap();
    let formula = |identifier: &str| {
        CellValue::Formula(Box::new(
            Formula::with_metadata(
                "",
                None,
                FormulaMetadata {
                    kind: FormulaType::Shared {
                        index: SharedFormulaIndex::from_literal(identifier),
                        master: false,
                    },
                    ..Default::default()
                },
            )
            .unwrap(),
        ))
    };
    let mut row = Row {
        index: RowIndex::new(0).unwrap(),
        cells: vec![Cell {
            address: "A1".parse().unwrap(),
            value: formula("invalid\0id"),
            style: StyleId::new(0),
        }],
    };
    let temporary = writer.temporary_bytes();
    assert!(writer.write_row(&row).is_err());
    assert_eq!(writer.temporary_bytes(), temporary);
    row.cells[0].value = formula("quoted & value");
    writer.write_row(&row).unwrap();
    let output = writer.finish(Cursor::new(Vec::new())).unwrap();
    let mut reader = WorkbookReader::new(output).unwrap();
    let read = reader
        .read_sheet_with_options(
            "Sheet",
            crabxl_core::ReadOptions {
                formula_metadata: true,
                ..Default::default()
            },
        )
        .unwrap();
    let CellValue::Formula(value) = &read.rows[0].cells[0].value else {
        panic!("Expected shared formula");
    };
    assert!(
        matches!(value.formula_type(), FormulaType::Shared { index, .. } if index.literal() == Some("quoted & value"))
    );
    assert_eq!(value.expression(), "");
}
