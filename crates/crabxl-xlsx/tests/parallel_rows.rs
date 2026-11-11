//! Worksheets large enough to be parsed on worker threads read exactly like
//! small ones, including when a block falls back to the event reader.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use crabxl_core::CellAddress;
use crabxl_core::{CellValue, ReadOptions, RowIndex};
use crabxl_xlsx::{WorkbookEditor, WorkbookReader};
use std::io::{Cursor, Write};
use zip::{ZipWriter, write::SimpleFileOptions};

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PKG: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const ROWS: u32 = 60_000;

fn book(sheet_data: &str) -> WorkbookReader<Cursor<Vec<u8>>> {
    book_with(sheet_data, &[])
}

/// A workbook with extra parts, each also listed in the workbook relationships.
fn book_with(sheet_data: &str, extra: &[(&str, &str, String)]) -> WorkbookReader<Cursor<Vec<u8>>> {
    WorkbookReader::new(Cursor::new(package(sheet_data, extra))).unwrap()
}

/// The bytes of a workbook with one sheet and the given extra parts.
fn package(sheet_data: &str, extra: &[(&str, &str, String)]) -> Vec<u8> {
    let links: String = extra
        .iter()
        .enumerate()
        .map(|(i, (name, kind, _))| {
            format!(
                "<Relationship Id=\"x{i}\" Type=\"{REL}/{kind}\" Target=\"{}\"/>",
                name.trim_start_matches("xl/")
            )
        })
        .collect();
    let parts = [
        ("[Content_Types].xml", "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/><Override PartName=\"/xl/worksheets/sheet1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/></Types>".to_owned()),
        ("_rels/.rels", format!("<Relationships xmlns=\"{PKG}\"><Relationship Id=\"rId1\" Type=\"{REL}/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>")),
        ("xl/workbook.xml", format!("<workbook xmlns=\"{MAIN}\" xmlns:r=\"{REL}\"><sheets><sheet name=\"S\" sheetId=\"1\" r:id=\"rId1\"/></sheets></workbook>")),
        ("xl/_rels/workbook.xml.rels", format!("<Relationships xmlns=\"{PKG}\"><Relationship Id=\"rId1\" Type=\"{REL}/worksheet\" Target=\"worksheets/sheet1.xml\"/>{links}</Relationships>")),
        ("xl/worksheets/sheet1.xml", format!("<worksheet xmlns=\"{MAIN}\"><sheetData>{sheet_data}</sheetData><mergeCells count=\"1\"><mergeCell ref=\"A1:B1\"/></mergeCells></worksheet>")),
    ];
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default();
    for (name, content) in parts.into_iter().chain(
        extra
            .iter()
            .map(|(name, _, content)| (*name, content.clone())),
    ) {
        zip.start_file(name, options).unwrap();
        zip.write_all(content.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

/// Rows 1..=ROWS with three plain cells each, row `n` replaced by `special(n)`.
fn sheet(special: impl Fn(u32) -> Option<String>) -> String {
    (1..=ROWS)
        .map(|n| {
            special(n).unwrap_or_else(|| {
                format!(
                    "<row r=\"{n}\" spans=\"1:3\"><c r=\"A{n}\"><v>{n}</v></c><c r=\"B{n}\"><v>{n}.5</v></c><c r=\"C{n}\" t=\"b\"><v>{}</v></c></row>\n",
                    n % 2
                )
            })
        })
        .collect()
}

fn read(
    book: &mut WorkbookReader<Cursor<Vec<u8>>>,
    options: ReadOptions,
) -> crabxl_core::Result<Vec<crabxl_core::Row>> {
    let mut rows = book.rows_with_options("S", options)?;
    let mut out = Vec::new();
    while let Some(row) = rows.next_row()? {
        out.push(row);
    }
    Ok(out)
}

fn assert_plain(row: &crabxl_core::Row) {
    let n = row.index.get() + 1;
    let values: Vec<_> = row.cells.iter().map(|cell| cell.value.clone()).collect();
    assert_eq!(
        values,
        [
            CellValue::Integer(i64::from(n)),
            CellValue::Number(f64::from(n) + 0.5),
            CellValue::Boolean(n % 2 == 1),
        ],
        "row {n}"
    );
}

#[test]
fn large_sheets_read_all_rows_in_order() {
    let rows = read(&mut book(&sheet(|_| None)), ReadOptions::default()).unwrap();
    assert_eq!(rows.len(), ROWS as usize);
    rows.iter().for_each(assert_plain);
}

#[test]
fn a_formula_midway_hands_the_rest_to_the_event_reader() {
    let middle = ROWS / 2;
    let xml = sheet(|n| {
        (n == middle).then(|| format!("<row r=\"{n}\"><c r=\"A{n}\"><f>1+1</f><v>2</v></c></row>"))
    });
    let rows = read(&mut book(&xml), ReadOptions::default()).unwrap();
    assert_eq!(rows.len(), ROWS as usize);
    for row in &rows {
        if row.index.get() + 1 == middle {
            assert!(matches!(row.cells[0].value, CellValue::Formula(_)));
        } else {
            assert_plain(row);
        }
    }
}

#[test]
fn errors_match_sequential_reading() {
    let xml = sheet(|n| (n == 40_000).then(|| "<row r=\"39999\"></row>".to_owned()));
    let error = read(&mut book(&xml), ReadOptions::default()).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Rows must be in strictly increasing order"),
        "{error}"
    );

    let xml = sheet(|n| {
        (n == 50_000).then(|| format!("<row r=\"{n}\"><c r=\"A{n}\"><v>x</v></c></row>"))
    });
    let error = read(&mut book(&xml), ReadOptions::default()).unwrap_err();
    assert!(
        error.to_string().contains("Invalid exact integer literal"),
        "{error}"
    );
}

#[test]
fn a_row_range_stops_before_an_invalid_tail() {
    let xml = sheet(|n| {
        (n == 50_000).then(|| format!("<row r=\"{n}\"><c r=\"A{n}\"><v>x</v></c></row>"))
    });
    let options = ReadOptions {
        rows: Some(RowIndex::new(0).unwrap()..=RowIndex::new(9).unwrap()),
        stop_after_last_row: true,
        ..ReadOptions::default()
    };
    let rows = read(&mut book(&xml), options).unwrap();
    assert_eq!(rows.len(), 10);
    rows.iter().for_each(assert_plain);
}

#[test]
fn captured_row_formatting_matches_each_row() {
    let xml = sheet(|n| {
        (n % 1000 == 0).then(|| {
            format!(
                "<row r=\"{n}\" ht=\"{}\" customHeight=\"1\" x14ac:dyDescent=\"0.3\" xmlns:x14ac=\"urn:x14ac\"><c r=\"A{n}\"><v>{n}</v></c><c r=\"B{n}\"><v>{n}.5</v></c><c r=\"C{n}\" t=\"b\"><v>{}</v></c></row>",
                n / 1000,
                n % 2
            )
        })
    });
    // Formatted rows declare a namespace, so they go to the event reader;
    // the same rows without the declaration stay on the worker threads.
    for xml in [xml.clone(), xml.replace(" xmlns:x14ac=\"urn:x14ac\"", "")] {
        let mut book = book(&xml);
        let mut rows = book.rows_with_options("S", ReadOptions::default()).unwrap();
        rows.capture_dimensions();
        let mut count = 0;
        while let Some(row) = rows.next_row().unwrap() {
            assert_plain(&row);
            let n = row.index.get() + 1;
            let height = rows.row_dimension().and_then(|dimension| dimension.height);
            assert_eq!(
                height,
                (n % 1000 == 0).then(|| f64::from(n / 1000)),
                "row {n}"
            );
            count += 1;
        }
        assert_eq!(count, ROWS);
    }
}

#[test]
fn shared_strings_and_dates_resolve_on_workers() {
    let strings: String = (0..100)
        .map(|i| format!("<si><t>text {i}</t></si>"))
        .collect();
    let extra = [
        (
            "xl/sharedStrings.xml",
            "sharedStrings",
            format!("<sst xmlns=\"{MAIN}\" count=\"100\" uniqueCount=\"100\">{strings}</sst>"),
        ),
        (
            "xl/styles.xml",
            "styles",
            format!(
                "<styleSheet xmlns=\"{MAIN}\"><fonts count=\"1\"><font/></fonts><fills count=\"1\"><fill/></fills><borders count=\"1\"><border/></borders><cellXfs count=\"2\"><xf numFmtId=\"0\"/><xf numFmtId=\"14\" applyNumberFormat=\"1\"/></cellXfs></styleSheet>"
            ),
        ),
    ];
    let xml = sheet(|n| {
        Some(format!(
            "<row r=\"{n}\"><c r=\"A{n}\" t=\"s\"><v>{}</v></c><c r=\"B{n}\" s=\"1\"><v>45000</v></c><c r=\"C{n}\"><v>{n}</v></c></row>",
            n % 100
        ))
    });
    let rows = read(&mut book_with(&xml, &extra), ReadOptions::default()).unwrap();
    assert_eq!(rows.len(), ROWS as usize);
    for row in &rows {
        let n = row.index.get() + 1;
        assert_eq!(
            row.cells[0].value,
            CellValue::text(format!("text {}", n % 100)),
            "row {n}"
        );
        let CellValue::DateTime(date) = &row.cells[1].value else {
            panic!("row {n}: {:?}", row.cells[1].value);
        };
        assert_eq!(date.to_date().unwrap().to_string(), "2023-03-15");
        assert_eq!(row.cells[2].value, CellValue::Integer(i64::from(n)));
    }
}

/// Edit cells at the start, middle and end and past the last row, save, and
/// read the result back.
fn edited(sheet_data: &str) -> Vec<crabxl_core::Row> {
    let mut editor = WorkbookEditor::new(Cursor::new(package(sheet_data, &[]))).unwrap();
    for (address, value) in [
        ("A1", 100),
        ("B30000", 200),
        ("C59999", 300),
        ("D60005", 400),
    ] {
        editor
            .upsert_value(
                "S",
                address.parse::<CellAddress>().unwrap(),
                CellValue::Integer(value),
            )
            .unwrap();
    }
    let (saved, _) = editor
        .save(Cursor::new(Vec::new()), Default::default())
        .unwrap();
    let mut reader = WorkbookReader::new(Cursor::new(saved.into_inner())).unwrap();
    read(&mut reader, ReadOptions::default()).unwrap()
}

fn assert_edited(rows: &[crabxl_core::Row], special: impl Fn(&crabxl_core::Row) -> bool) {
    assert_eq!(rows.len(), ROWS as usize + 1);
    for row in rows {
        let n = row.index.get() + 1;
        let values: Vec<_> = row.cells.iter().map(|cell| cell.value.clone()).collect();
        match n {
            1 => assert_eq!(values[0], CellValue::Integer(100)),
            30_000 => assert_eq!(values[1], CellValue::Integer(200)),
            59_999 => assert_eq!(values[2], CellValue::Integer(300)),
            60_005 => assert_eq!(values, [CellValue::Integer(400)]),
            _ if special(row) => {}
            _ => assert_plain(row),
        }
    }
}

#[test]
fn edits_copy_untouched_rows_and_rewrite_edited_ones() {
    assert_edited(&edited(&sheet(|_| None)), |_| false);
    // A formula midway sends the rest of the sheet through the event writer.
    let middle = ROWS / 2 + 7;
    let xml = sheet(|n| {
        (n == middle).then(|| format!("<row r=\"{n}\"><c r=\"A{n}\"><f>1+1</f><v>2</v></c></row>"))
    });
    assert_edited(&edited(&xml), |row| {
        row.index.get() + 1 == middle && matches!(row.cells[0].value, CellValue::Formula(_))
    });
}
