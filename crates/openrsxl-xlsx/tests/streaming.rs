//! Small OOXML fixtures generated in memory; no copied upstream binary fixtures.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use openrsxl_core::{
    AccessPattern, AutoMemory, CellAddress, CellValue, ColumnIndex, DecisionReason, ErrorKind,
    MemoryPolicy, MemorySource, ReadMode, ReadOptions, ResourceLimits, Row, RowIndex,
};
use openrsxl_xlsx::{ReadData, WorkbookReader};
use std::io::{Cursor, Write};
use zip::{ZipWriter, write::SimpleFileOptions};

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PKG: &str = "http://schemas.openxmlformats.org/package/2006/relationships";

fn fixture(parts: &[(&str, &str)]) -> Vec<u8> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, content) in parts {
        zip.start_file(*name, options).unwrap();
        zip.write_all(content.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}
fn entries(sheet: &str) -> Vec<(String, String)> {
    vec![
        ("[Content_Types].xml".into(), "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Override PartName=\"/book/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/><Override PartName=\"/data/values.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/></Types>".into()),
        ("_rels/.rels".into(),format!("<Relationships xmlns=\"{PKG}\"><Relationship Id=\"doc\" Type=\"{REL}/officeDocument\" Target=\"book/workbook.xml\"/></Relationships>")),
        ("book/workbook.xml".into(),format!("<workbook xmlns=\"{MAIN}\" xmlns:link=\"{REL}\"><workbookPr date1904=\"1\"/><sheets><sheet name=\"A &amp; B\" sheetId=\"1\" link:id=\"r42\"/></sheets></workbook>")),
        ("book/_rels/workbook.xml.rels".into(),format!("<Relationships xmlns=\"{PKG}\"><Relationship Id=\"r42\" Type=\"{REL}/worksheet\" Target=\"../data/values.xml\"/></Relationships>")),
        ("data/values.xml".into(),sheet.into()),
    ]
}
fn from_entries(entries: &[(String, String)]) -> WorkbookReader<Cursor<Vec<u8>>> {
    let parts: Vec<_> = entries
        .iter()
        .map(|(n, v)| (n.as_str(), v.as_str()))
        .collect();
    WorkbookReader::new(Cursor::new(fixture(&parts))).unwrap()
}
fn open(sheet_data: &str) -> WorkbookReader<Cursor<Vec<u8>>> {
    from_entries(&entries(&format!(
        "<worksheet xmlns=\"{MAIN}\"><dimension ref=\"A1\"/><sheetData>{sheet_data}</sheetData></worksheet>"
    )))
}
fn columns(start: u32, end: u32) -> ReadOptions {
    ReadOptions {
        columns: Some(ColumnIndex::new(start).unwrap()..=ColumnIndex::new(end).unwrap()),
        ..ReadOptions::default()
    }
}

#[test]
fn resolves_nonstandard_parts_and_unescapes_names() {
    let mut book = open("<row r=\"2\"><c r=\"B2\"><v>-2.5e2</v></c><c r=\"D2\"><v>0</v></c></row>");
    assert_eq!(book.sheets()[0].name(), "A & B");
    assert_eq!(book.sheets()[0].part(), "data/values.xml");
    assert!(book.date_1904());
    let rows = book
        .rows("A & B")
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].index.get(), 1);
    assert_eq!(rows[0].cells[0].address.to_string(), "B2");
    assert_eq!(rows[0].cells[0].value, CellValue::Number(-250.0));
    assert_eq!(rows[0].cells[1].address.to_string(), "D2");
    assert_eq!(rows[0].cells[1].value, CellValue::Number(0.0));
}

#[test]
fn multiple_sheets_follow_relationship_ids_and_can_be_reopened() {
    let mut parts = entries(&format!(
        "<worksheet xmlns=\"{MAIN}\"><sheetData><row><c><v>1</v></c></row></sheetData></worksheet>"
    ));
    parts[0].1 = parts[0].1.replace("</Types>", "<Override PartName=\"/data/second.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/></Types>");
    parts[2].1 = parts[2].1.replace(
        "</sheets>",
        "<sheet name=\"Second\" sheetId=\"99\" link:id=\"other\"/></sheets>",
    );
    parts[3].1 = parts[3].1.replace("</Relationships>", &format!("<Relationship Id=\"other\" Type=\"{REL}/worksheet\" Target=\"../data/second.xml\"/></Relationships>"));
    parts.push(("data/second.xml".into(), format!("<worksheet xmlns=\"{MAIN}\"><sheetData><row><c><v>9</v></c></row></sheetData></worksheet>")));
    let mut book = from_entries(&parts);
    assert_eq!(book.sheets().len(), 2);
    assert_eq!(book.sheets()[1].part(), "data/second.xml");
    assert_eq!(
        book.rows("Second")
            .unwrap()
            .next_row()
            .unwrap()
            .unwrap()
            .cells[0]
            .value,
        CellValue::Number(9.0)
    );
    assert_eq!(
        book.rows("A & B")
            .unwrap()
            .next_row()
            .unwrap()
            .unwrap()
            .cells[0]
            .value,
        CellValue::Number(1.0)
    );
    assert_eq!(book.rows("Second").unwrap().count(), 1);
    assert!(matches!(book.rows("missing"), Err(error) if error.kind() == ErrorKind::SheetNotFound));
}

#[test]
fn invalid_row_attributes_retain_part_context() {
    let mut book = open("<row r=\"0\"/>");
    let mut rows = book.rows("A & B").unwrap();
    let error = rows.next_row().unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidData);
    assert_eq!(error.part(), Some("data/values.xml"));
    assert!(rows.next_row().unwrap().is_none());
}

#[test]
fn materialized_sheet_matches_streaming_and_outlives_workbook() {
    let content = "<row r=\"4\"><c r=\"C4\"><v>7</v></c><c/></row><row/><row><c><v>3</v></c></row>";
    let mut book = open(content);
    let streamed = book
        .rows("A & B")
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let snapshot = book.read_sheet("A & B").unwrap();
    assert_eq!(snapshot.rows, streamed);
    assert!(snapshot.memory_bytes() <= ResourceLimits::default().max_materialized_bytes);
    drop(book);
    assert_eq!(snapshot.rows[0].cells[0].value, CellValue::Number(7.0));
}

#[test]
fn materialization_budget_fails_and_releases_reader() {
    let document = format!(
        "<worksheet xmlns=\"{MAIN}\"><sheetData><row><c><v>1</v></c></row></sheetData></worksheet>"
    );
    let parts = entries(&document);
    let refs: Vec<_> = parts
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    let limits = ResourceLimits {
        max_materialized_bytes: 100,
        ..ResourceLimits::default()
    };
    let mut book = WorkbookReader::with_limits(Cursor::new(fixture(&refs)), limits).unwrap();
    let error = book.read_sheet("A & B").unwrap_err();
    assert_eq!(error.kind(), ErrorKind::MemoryBudgetExceeded);
    assert_eq!(error.part(), Some("data/values.xml"));
    assert_eq!(
        book.rows("A & B")
            .unwrap()
            .next_row()
            .unwrap()
            .unwrap()
            .cells[0]
            .value,
        CellValue::Number(1.0)
    );
}

#[test]
fn materialization_budget_counts_outer_capacity_and_empty_sheets() {
    let limits = ResourceLimits {
        max_materialized_bytes: 1,
        ..ResourceLimits::default()
    };
    let parts = entries(&format!(
        "<worksheet xmlns=\"{MAIN}\"><sheetData/></worksheet>"
    ));
    let refs: Vec<_> = parts
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    let mut book = WorkbookReader::with_limits(Cursor::new(fixture(&refs)), limits).unwrap();
    assert_eq!(
        book.read_sheet("A & B").unwrap_err().kind(),
        ErrorKind::MemoryBudgetExceeded
    );
    let parts = entries(&format!(
        "<worksheet xmlns=\"{MAIN}\"><sheetData>{}</sheetData></worksheet>",
        "<row/>".repeat(50)
    ));
    let refs: Vec<_> = parts
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    let limits = ResourceLimits {
        max_materialized_bytes: 500,
        ..ResourceLimits::default()
    };
    let mut book = WorkbookReader::with_limits(Cursor::new(fixture(&refs)), limits).unwrap();
    assert_eq!(
        book.read_sheet("A & B").unwrap_err().kind(),
        ErrorKind::MemoryBudgetExceeded
    );
    let mut book = open(&"<row/>".repeat(50));
    assert_eq!(book.read_sheet("A & B").unwrap().rows.len(), 50);
}

#[test]
fn configured_input_buffers_work_and_zero_is_rejected() {
    let parts = entries(&format!(
        "<worksheet xmlns=\"{MAIN}\"><sheetData><row><c><v>42</v></c></row></sheetData></worksheet>"
    ));
    let refs: Vec<_> = parts
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    let data = fixture(&refs);
    for size in [1, 256 * 1024] {
        let limits = ResourceLimits {
            input_buffer_bytes: size,
            ..ResourceLimits::default()
        };
        let mut book = WorkbookReader::with_limits(Cursor::new(data.clone()), limits).unwrap();
        assert_eq!(
            book.read_sheet("A & B").unwrap().rows[0].cells[0].value,
            CellValue::Number(42.0)
        );
    }
    let limits = ResourceLimits {
        input_buffer_bytes: 0,
        ..ResourceLimits::default()
    };
    assert!(
        matches!(WorkbookReader::with_limits(Cursor::new(data), limits), Err(error) if error.kind() == ErrorKind::InvalidData)
    );
}

#[test]
fn adaptive_scan_streams_without_sampling_and_respects_fixed_budget() {
    let mut book = open("<row><c><v>3</v></c></row>");
    let output = book
        .read_with_policy(
            "A & B",
            AccessPattern::Scan,
            MemoryPolicy::Budget(16 * 1024 * 1024),
        )
        .unwrap();
    assert_eq!(output.decision.mode, ReadMode::Streaming);
    assert_eq!(output.decision.reason, DecisionReason::SequentialAccess);
    assert_eq!(output.decision.memory_source, MemorySource::ExplicitBudget);
    assert_eq!(output.decision.estimated_data_bytes, None);
    assert_eq!(output.decision.budget_bytes, 16 * 1024 * 1024);
    match output.data {
        ReadData::Streaming(mut rows) => assert_eq!(
            rows.next_row().unwrap().unwrap().cells[0].value,
            CellValue::Number(3.0)
        ),
        ReadData::Materialized(_) => panic!("Scan must stream"),
    }
}

#[test]
fn adaptive_repeated_access_materializes_using_available_memory() {
    let mut book = open("<row><c><v>3</v></c></row><row><c><v>4</v></c></row>");
    let policy = MemoryPolicy::Auto(AutoMemory {
        available_bytes: Some(32 * 1024 * 1024 * 1024),
        ..AutoMemory::default()
    });
    let snapshot = {
        let output = book
            .read_with_policy("A & B", AccessPattern::RepeatedAccess, policy)
            .unwrap();
        assert_eq!(output.decision.mode, ReadMode::Materialized);
        assert_eq!(output.decision.reason, DecisionReason::SampleFits);
        assert!(output.decision.budget_bytes > 1024 * 1024 * 1024);
        match output.data {
            ReadData::Materialized(sheet) => sheet,
            ReadData::Streaming(_) => panic!("Small repeated-access input should fit"),
        }
    };
    drop(book);
    assert_eq!(snapshot.rows.len(), 2);
    assert_eq!(snapshot.rows[1].cells[0].value, CellValue::Number(4.0));
}

#[test]
fn adaptive_estimate_rejects_materialization_before_loading_whole_sheet() {
    let mut book = open(&"<row><c><v>1</v></c></row>".repeat(4000));
    let output = book
        .read_with_policy(
            "A & B",
            AccessPattern::RepeatedAccess,
            MemoryPolicy::Budget(2 * 1024 * 1024),
        )
        .unwrap();
    assert_eq!(
        output.decision.reason,
        DecisionReason::EstimateExceedsBudget
    );
    assert_eq!(output.decision.mode, ReadMode::Streaming);
    match output.data {
        ReadData::Streaming(mut rows) => {
            assert_eq!(rows.decoded_cells(), 0);
            assert_eq!(rows.next_row().unwrap().unwrap().index.get(), 0);
        }
        ReadData::Materialized(_) => panic!("Data should not fit"),
    }
}

#[test]
fn adaptive_heterogeneous_input_falls_back_at_actual_budget() {
    let long = format!("<row><c><v>{}</v></c></row>", "0".repeat(5000));
    let content = long.repeat(128) + &"<row><c><v>1</v></c></row>".repeat(3000);
    let mut book = open(&content);
    let output = book
        .read_with_policy(
            "A & B",
            AccessPattern::RepeatedAccess,
            MemoryPolicy::Budget(2 * 1024 * 1024),
        )
        .unwrap();
    assert_eq!(
        output.decision.reason,
        DecisionReason::ActualDataExceedsBudget
    );
    assert_eq!(output.decision.mode, ReadMode::Streaming);
    match output.data {
        ReadData::Streaming(rows) => {
            assert_eq!(rows.collect::<Result<Vec<_>, _>>().unwrap().len(), 3128)
        }
        ReadData::Materialized(_) => panic!("Later rows must exceed the allowance"),
    }
}

#[test]
fn adaptive_errors_are_not_hidden_by_streaming_fallback() {
    let mut book = open("<row><c t=\"s\"><v>0</v></c></row>");
    assert!(
        matches!(book.read_with_policy("A & B", AccessPattern::RepeatedAccess, MemoryPolicy::default()), Err(error) if error.kind() == ErrorKind::Unsupported)
    );
    let mut book = open("<row/>");
    assert!(
        matches!(book.read_with_policy("A & B", AccessPattern::Scan, MemoryPolicy::Budget(1)), Err(error) if error.kind() == ErrorKind::MemoryBudgetExceeded)
    );
    assert!(
        matches!(book.read_with_policy("missing", AccessPattern::Scan, MemoryPolicy::Budget(16 * 1024 * 1024)), Err(error) if error.kind() == ErrorKind::SheetNotFound)
    );
    let content = "<row><c><v>1</v></c></row>".repeat(128) + "<row><c t=\"s\"><v>0</v></c></row>";
    let mut book = open(&content);
    assert!(
        matches!(book.read_with_policy("A & B", AccessPattern::RepeatedAccess, MemoryPolicy::Budget(16 * 1024 * 1024)), Err(error) if error.kind() == ErrorKind::Unsupported)
    );
}
#[test]
fn inferred_coordinates_and_empty_rows_preserve_sparsity() {
    let mut book = open(
        "<row r=\"4\"><c r=\"C4\"><v>1</v></c><c/><c><v>2</v></c></row><row/><row><c><v/></c></row>",
    );
    let rows = book
        .rows("A & B")
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        rows.iter().map(|r| r.index.get()).collect::<Vec<_>>(),
        vec![3, 4, 5]
    );
    assert_eq!(
        rows[0]
            .cells
            .iter()
            .map(|c| c.address.to_string())
            .collect::<Vec<_>>(),
        vec!["C4", "D4", "E4"]
    );
    assert_eq!(rows[0].cells[1].value, CellValue::Empty);
    assert!(rows[1].cells.is_empty());
    assert_eq!(rows[2].cells[0].value, CellValue::Empty);
}
#[test]
fn projection_uses_current_sparse_coordinate_before_decoding() {
    let mut book = open(
        "<row><c r=\"A1\"><v>1</v></c><c r=\"B1\"><v>2</v></c><c r=\"XFD1\" t=\"s\"><v>broken</v></c></row>",
    );
    let mut rows = book.rows_with_options("A & B", columns(0, 1)).unwrap();
    assert_eq!(rows.next_row().unwrap().unwrap().cells.len(), 2);
    assert!(rows.next_row().unwrap().is_none());
    assert_eq!(rows.decoded_cells(), 2);
}
#[test]
fn excluded_rows_do_not_decode_unsupported_values() {
    let mut book =
        open("<row><c t=\"inlineStr\"><is><t>ignored</t></is></c></row><row><c><v>3</v></c></row>");
    let options = ReadOptions {
        rows: Some(RowIndex::new(1).unwrap()..=RowIndex::new(1).unwrap()),
        ..ReadOptions::default()
    };
    let mut rows = book.rows_with_options("A & B", options).unwrap();
    assert_eq!(
        rows.next_row().unwrap().unwrap().cells[0].value,
        CellValue::Number(3.0)
    );
    assert!(rows.next_row().unwrap().is_none());
    assert_eq!(rows.decoded_cells(), 1);
}
#[test]
fn reuse_buffer_and_release_early_reader() {
    let mut book = open("<row><c><v>1</v></c></row><row><c><v>2</v></c></row>");
    let mut row = Row::new(RowIndex::new(0).unwrap());
    {
        let mut rows = book.rows("A & B").unwrap();
        assert!(rows.read_row_into(&mut row).unwrap());
        let capacity = row.cells.capacity();
        assert!(rows.read_row_into(&mut row).unwrap());
        assert_eq!(row.cells.capacity(), capacity);
    }
    assert_eq!(book.rows("A & B").unwrap().count(), 2);
    assert_eq!(row.cells[0].value, CellValue::Number(2.0));
}
#[test]
fn owned_batches_obey_byte_budget_and_outlive_workbook() {
    let mut parts = entries(&format!(
        "<worksheet xmlns=\"{MAIN}\"><sheetData><row><c><v>1</v></c></row><row><c><v>2</v></c></row><row><c><v>3</v></c></row></sheetData></worksheet>"
    ));
    let refs: Vec<_> = parts
        .iter_mut()
        .map(|(n, v)| (n.as_str(), v.as_str()))
        .collect();
    let limits = ResourceLimits {
        max_batch_rows: 2,
        max_batch_bytes: 500,
        ..ResourceLimits::default()
    };
    let mut book = WorkbookReader::with_limits(Cursor::new(fixture(&refs)), limits).unwrap();
    let mut rows = book.rows("A & B").unwrap();
    let first = rows.read_batch().unwrap().unwrap();
    assert!(first.memory_bytes() <= 500);
    let mut count = first.rows.len();
    while let Some(batch) = rows.read_batch().unwrap() {
        assert!(batch.memory_bytes() <= 500);
        count += batch.rows.len();
    }
    assert_eq!(count, 3);
    drop(rows);
    drop(book);
    assert_eq!(first.rows[0].cells[0].value, CellValue::Number(1.0));
}
#[test]
fn tiny_batch_limit_fails_without_partial_delivery() {
    let mut parts = entries(&format!(
        "<worksheet xmlns=\"{MAIN}\"><sheetData><row><c><v>1</v></c></row></sheetData></worksheet>"
    ));
    let refs: Vec<_> = parts
        .iter_mut()
        .map(|(n, v)| (n.as_str(), v.as_str()))
        .collect();
    let limits = ResourceLimits {
        max_batch_bytes: 100,
        ..ResourceLimits::default()
    };
    let mut book = WorkbookReader::with_limits(Cursor::new(fixture(&refs)), limits).unwrap();
    assert_eq!(
        book.rows("A & B").unwrap().read_batch().unwrap_err().kind(),
        ErrorKind::LimitExceeded
    );
}
#[test]
fn prefixes_and_strict_spreadsheet_namespaces_work() {
    let sheet = "<s:worksheet xmlns:s=\"http://purl.oclc.org/ooxml/spreadsheetml/main\"><s:sheetData><s:row><s:c><s:v>4</s:v></s:c></s:row></s:sheetData></s:worksheet>";
    let mut book = from_entries(&entries(sheet));
    assert_eq!(
        book.rows("A & B")
            .unwrap()
            .next_row()
            .unwrap()
            .unwrap()
            .cells[0]
            .value,
        CellValue::Number(4.0)
    );
}
#[test]
fn ignores_nested_extension_sheet_lookalikes_in_metadata() {
    let mut parts = entries(&format!(
        "<worksheet xmlns=\"{MAIN}\"><sheetData/></worksheet>"
    ));
    parts[2].1=parts[2].1.replace("</workbook>","<extLst><ext uri=\"x\"><sheet name=\"NotASheet\" xmlns=\"urn:foreign\"/></ext></extLst></workbook>");
    let book = from_entries(&parts);
    assert_eq!(book.sheets().len(), 1);
}
#[test]
fn foreign_cell_namespace_is_not_decoded_as_spreadsheet_data() {
    let mut book = open("<row><x:c xmlns:x=\"urn:foreign\"><x:v>1</x:v></x:c></row>");
    assert_eq!(
        book.rows("A & B").unwrap().next_row().unwrap_err().kind(),
        ErrorKind::InvalidData
    );
}
#[test]
fn selected_unsupported_features_fail_with_cell_context() {
    for content in [
        "<c t=\"s\"><v>0</v></c>",
        "<c s=\"1\"><v>1</v></c>",
        "<c><f>1+1</f><v>2</v></c>",
        "<c vm=\"1\"><v>1</v></c>",
    ] {
        let mut book = open(&format!("<row>{content}</row>"));
        let mut rows = book.rows("A & B").unwrap();
        let error = rows.next_row().unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Unsupported);
        assert_eq!(error.cell().unwrap().to_string(), "A1");
        assert_eq!(error.part(), Some("data/values.xml"));
        assert!(rows.next().is_none());
        assert!(rows.next().is_none());
    }
}
#[test]
fn malformed_numeric_values_and_coordinates_fail() {
    for content in [
        "<c><v>NaN</v></c>",
        "<c><v>1e999</v></c>",
        "<c><v>broken</v></c>",
        "<c r=\"XFE1\"><v>1</v></c>",
        "<c r=\"A0\"><v>1</v></c>",
        "<c r=\"A2\"><v>1</v></c>",
        "<c><v>1</v><v>2</v></c>",
    ] {
        let mut book = open(&format!("<row>{content}</row>"));
        assert_eq!(
            book.rows("A & B").unwrap().next_row().unwrap_err().kind(),
            ErrorKind::InvalidData
        );
    }
}
#[test]
fn numeric_entities_and_zero_style_are_supported() {
    let mut book = open("<row><c s=\"00\" t=\"&#110;\"><v>&#49;<![CDATA[2]]></v></c></row>");
    assert_eq!(
        book.rows("A & B")
            .unwrap()
            .next_row()
            .unwrap()
            .unwrap()
            .cells[0]
            .value,
        CellValue::Number(12.0)
    );
}
#[test]
fn rejects_duplicate_or_unsorted_cells_and_rows() {
    for data in [
        "<row><c r=\"B1\"/><c r=\"A1\"/></row>",
        "<row><c r=\"A1\"/><c r=\"A1\"/></row>",
        "<row r=\"2\"/><row r=\"1\"/>",
    ] {
        let mut book = open(data);
        assert!(
            book.rows("A & B")
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .is_err()
        );
    }
}
#[test]
fn event_cell_row_depth_and_metadata_limits_are_enforced() {
    let document = format!(
        "<worksheet xmlns=\"{MAIN}\"><sheetData><row><c><v>{}</v></c></row></sheetData></worksheet>",
        "0".repeat(5000)
    );
    let parts = entries(&document);
    let refs: Vec<_> = parts
        .iter()
        .map(|(n, v)| (n.as_str(), v.as_str()))
        .collect();
    let data = fixture(&refs);
    for limits in [
        ResourceLimits {
            max_xml_event_bytes: 1024,
            ..ResourceLimits::default()
        },
        ResourceLimits {
            max_cell_bytes: 8,
            ..ResourceLimits::default()
        },
        ResourceLimits {
            max_row_bytes: 1,
            ..ResourceLimits::default()
        },
        ResourceLimits {
            max_xml_depth: 4,
            ..ResourceLimits::default()
        },
    ] {
        let mut book = WorkbookReader::with_limits(Cursor::new(data.clone()), limits).unwrap();
        let error = book.rows("A & B").unwrap().next_row().unwrap_err();
        assert_eq!(error.kind(), ErrorKind::LimitExceeded);
    }
    assert!(
        WorkbookReader::with_limits(
            Cursor::new(data),
            ResourceLimits {
                max_metadata_bytes: 10,
                ..ResourceLimits::default()
            }
        )
        .is_err()
    );
}
#[test]
fn compressed_archive_and_entry_count_limits_are_checked() {
    let parts = entries(&format!(
        "<worksheet xmlns=\"{MAIN}\"><sheetData/></worksheet>"
    ));
    let refs: Vec<_> = parts
        .iter()
        .map(|(n, v)| (n.as_str(), v.as_str()))
        .collect();
    let data = fixture(&refs);
    for limits in [
        ResourceLimits {
            max_archive_bytes: 10,
            ..ResourceLimits::default()
        },
        ResourceLimits {
            max_archive_entries: 2,
            ..ResourceLimits::default()
        },
        ResourceLimits {
            max_total_uncompressed_bytes: 10,
            ..ResourceLimits::default()
        },
    ] {
        assert!(WorkbookReader::with_limits(Cursor::new(data.clone()), limits).is_err());
    }
}
#[test]
fn rejects_truncated_mismatched_and_duplicate_root_xml() {
    for sheet in [
        format!("<worksheet xmlns=\"{MAIN}\"><sheetData><row><c><v>1</v></c></row>"),
        format!(
            "<worksheet xmlns=\"{MAIN}\"><sheetData><row><c><v>1</v></c></oops></sheetData></worksheet>"
        ),
        format!("<worksheet xmlns=\"{MAIN}\"><sheetData/></worksheet><extra/>"),
    ] {
        let mut book = from_entries(&entries(&sheet));
        assert!(
            book.rows("A & B")
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .is_err()
        );
    }
}
#[test]
fn rejects_doctype_even_without_entity_use() {
    let sheet = format!(
        "<!DOCTYPE worksheet [<!ENTITY a \"1\">]><worksheet xmlns=\"{MAIN}\"><sheetData/></worksheet>"
    );
    let mut book = from_entries(&entries(&sheet));
    let error = match book.rows("A & B") {
        Ok(_) => panic!("DTD unexpectedly accepted"),
        Err(e) => e,
    };
    assert_eq!(error.kind(), ErrorKind::Unsupported);
}
#[test]
fn normalized_uri_paths_and_package_escape_rejection() {
    let mut parts = entries(&format!(
        "<worksheet xmlns=\"{MAIN}\"><sheetData/></worksheet>"
    ));
    parts[3].1 = parts[3]
        .1
        .replace("../data/values.xml", "/data/./values.xml");
    let book = from_entries(&parts);
    assert_eq!(book.sheets()[0].part(), "data/values.xml");
    parts[3].1 = parts[3]
        .1
        .replace("/data/./values.xml", "../../outside.xml");
    let refs: Vec<_> = parts
        .iter()
        .map(|(n, v)| (n.as_str(), v.as_str()))
        .collect();
    assert!(WorkbookReader::new(Cursor::new(fixture(&refs))).is_err());
}
#[test]
fn validates_crc_after_consuming_entire_worksheet() {
    let parts = entries(&format!(
        "<worksheet xmlns=\"{MAIN}\"><sheetData><row><c><v>1</v></c></row></sheetData></worksheet>"
    ));
    let refs: Vec<_> = parts
        .iter()
        .map(|(n, v)| (n.as_str(), v.as_str()))
        .collect();
    let mut data = fixture(&refs);
    let position = data.windows(8).position(|v| v == b"<v>1</v>").unwrap();
    data[position + 3] = b'2';
    let mut book = WorkbookReader::new(Cursor::new(data)).unwrap();
    assert!(
        book.rows("A & B")
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .is_err()
    );
}
#[test]
fn address_overflow_and_absolute_a1_are_checked() {
    for reference in [
        "A0",
        "XFE1",
        "A1048577",
        "AAAAAAA99999999999999999999",
        "0",
        "",
        "A01",
    ] {
        assert!(reference.parse::<CellAddress>().is_err());
    }
    assert_eq!(
        "$xfd$1048576".parse::<CellAddress>().unwrap().to_string(),
        "XFD1048576"
    );
}

struct TrackedSource {
    cursor: Cursor<Vec<u8>>,
    drops: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}
impl std::io::Read for TrackedSource {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        std::io::Read::read(&mut self.cursor, buffer)
    }
}
impl std::io::Seek for TrackedSource {
    fn seek(&mut self, position: std::io::SeekFrom) -> std::io::Result<u64> {
        std::io::Seek::seek(&mut self.cursor, position)
    }
}
impl Drop for TrackedSource {
    fn drop(&mut self) {
        self.drops.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}
#[test]
fn early_stream_drop_reopens_and_into_inner_transfers_source_ownership() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let document = format!(
        "<worksheet xmlns=\"{MAIN}\"><sheetData><row><c><v>1</v></c></row><row><c><v>2</v></c></row></sheetData></worksheet>"
    );
    let parts = entries(&document);
    let refs: Vec<_> = parts
        .iter()
        .map(|(n, v)| (n.as_str(), v.as_str()))
        .collect();
    let drops = Arc::new(AtomicUsize::new(0));
    let source = TrackedSource {
        cursor: Cursor::new(fixture(&refs)),
        drops: Arc::clone(&drops),
    };
    let mut book = WorkbookReader::new(source).unwrap();
    {
        let mut rows = book.rows("A & B").unwrap();
        assert!(rows.next_row().unwrap().is_some());
    }
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    assert_eq!(
        book.rows("A & B")
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .len(),
        2
    );
    let source = book.into_inner();
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    drop(source);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}
#[test]
fn rejected_archive_releases_owned_source() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let drops = Arc::new(AtomicUsize::new(0));
    let source = TrackedSource {
        cursor: Cursor::new(b"not a ZIP".to_vec()),
        drops: Arc::clone(&drops),
    };
    assert!(WorkbookReader::new(source).is_err());
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}
