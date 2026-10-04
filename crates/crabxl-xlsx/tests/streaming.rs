//! Small OOXML fixtures generated in memory; no copied upstream binary fixtures.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use crabxl_core::{
    AccessPattern, AutoMemory, CellAddress, CellValue, ColumnIndex, DecisionReason, ErrorKind,
    MemoryPolicy, MemorySource, ReadMode, ReadOptions, ResourceLimits, Row, RowIndex,
};
use crabxl_xlsx::{ReadData, WorkbookReader};
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
    assert_eq!(rows[0].cells[1].value, CellValue::Integer(0));
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
        CellValue::Integer(9)
    );
    assert_eq!(
        book.rows("A & B")
            .unwrap()
            .next_row()
            .unwrap()
            .unwrap()
            .cells[0]
            .value,
        CellValue::Integer(1)
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
    assert_eq!(snapshot.rows[0].cells[0].value, CellValue::Integer(7));
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
        CellValue::Integer(1)
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
            CellValue::Integer(42)
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
            CellValue::Integer(3)
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
    assert_eq!(snapshot.rows[1].cells[0].value, CellValue::Integer(4));
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
        matches!(book.read_with_policy("A & B", AccessPattern::RepeatedAccess, MemoryPolicy::default()), Err(error) if error.kind() == ErrorKind::InvalidData)
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
        matches!(book.read_with_policy("A & B", AccessPattern::RepeatedAccess, MemoryPolicy::Budget(16 * 1024 * 1024)), Err(error) if error.kind() == ErrorKind::InvalidData)
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
        CellValue::Integer(3)
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
    assert_eq!(row.cells[0].value, CellValue::Integer(2));
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
        max_batch_bytes: 700,
        ..ResourceLimits::default()
    };
    let mut book = WorkbookReader::with_limits(Cursor::new(fixture(&refs)), limits).unwrap();
    let mut rows = book.rows("A & B").unwrap();
    let first = rows.read_batch().unwrap().unwrap();
    assert!(first.memory_bytes() <= 700);
    let mut count = first.rows.len();
    while let Some(batch) = rows.read_batch().unwrap() {
        assert!(batch.memory_bytes() <= 700);
        count += batch.rows.len();
    }
    assert_eq!(count, 3);
    drop(rows);
    drop(book);
    assert_eq!(first.rows[0].cells[0].value, CellValue::Integer(1));
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
        CellValue::Integer(4)
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
    let content = "<c><f t=\"futureFormula\">1</f></c>";
    let mut book = open(&format!("<row>{content}</row>"));
    let mut rows = book.rows("A & B").unwrap();
    let error = rows.next_row().unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Unsupported);
    assert_eq!(error.cell().unwrap().to_string(), "A1");
    assert_eq!(error.part(), Some("data/values.xml"));
    assert!(rows.next().is_none());
    assert!(rows.next().is_none());
}
#[test]
fn malformed_numeric_values_and_coordinates_fail() {
    for content in [
        "<c><v>NaN</v></c>",
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
        CellValue::Integer(12)
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

#[test]
fn boolean_literals_preserve_type_and_reuse_all_read_modes() {
    let content = "<row><c t=\"b\"><v>0</v></c><c t=\"b\"><v>&#49;</v></c><c t=\"b\"><v> -00 </v></c><c t=\"b\"><v>+002</v></c><c t=\"b\"><v/></c><c><v>1</v></c></row>";
    let mut book = open(content);
    let expected = vec![
        CellValue::Boolean(false),
        CellValue::Boolean(true),
        CellValue::Boolean(false),
        CellValue::Boolean(true),
        CellValue::Empty,
        CellValue::Integer(1),
    ];
    let streamed = book
        .rows("A & B")
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        streamed[0]
            .cells
            .iter()
            .map(|cell| cell.value.clone())
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(book.read_sheet("A & B").unwrap().rows, streamed);
    let output = book
        .read_with_policy(
            "A & B",
            AccessPattern::RepeatedAccess,
            MemoryPolicy::Budget(64 * 1024 * 1024),
        )
        .unwrap();
    match output.data {
        ReadData::Materialized(data) => assert_eq!(data.rows, streamed),
        ReadData::Streaming(_) => panic!("Small boolean sheet should materialize"),
    }
}
#[test]
fn malformed_boolean_values_have_context_and_projection_can_skip_them() {
    for literal in ["true", "false", "1.0", "1e0", "+", "01x"] {
        let mut book = open(&format!(
            "<row><c t=\"b\"><v>{literal}</v></c><c><v>2</v></c></row>"
        ));
        let error = book.rows("A & B").unwrap().next_row().unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
        assert_eq!(error.part(), Some("data/values.xml"));
        assert_eq!(error.cell().unwrap().to_string(), "A1");
        let mut rows = book.rows_with_options("A & B", columns(1, 1)).unwrap();
        assert_eq!(
            rows.next_row().unwrap().unwrap().cells[0].value,
            CellValue::Integer(2)
        );
        assert_eq!(rows.decoded_cells(), 1);
    }
}
#[test]
fn boolean_payload_does_not_increase_cell_storage() {
    assert_eq!(size_of::<CellValue>(), 16);
    let mut book = open("<row><c t=\"b\"><v>1</v></c></row><row><c t=\"b\"><v>0</v></c></row>");
    let mut rows = book.rows("A & B").unwrap();
    let batch = rows.read_batch().unwrap().unwrap();
    drop(rows);
    drop(book);
    assert_eq!(batch.rows[1].cells[0].value, CellValue::Boolean(false));
}

#[test]
fn exact_integers_float_lexemes_errors_and_plain_inline_text() {
    let mut book = open(
        "<row><c><v>9007199254740993</v></c><c><v>-9223372036854775808</v></c><c><v>9223372036854775808</v></c><c><v>-000999999999999999999999999999999</v></c><c><v>1.0</v></c><c><v>1e0</v></c><c t=\"e\"><v>#DIV/0!</v></c><c t=\"e\"><v>#FUTURE!</v></c><c t=\"inlineStr\"><is><t xml:space=\"preserve\"> \t&amp;&lt;![CDATA[ignored]]&gt;\n </t></is></c><c t=\"inlineStr\"><is><t/></is></c><c t=\"str\"><v> preserved </v></c></row>",
    );
    let data = book.read_sheet("A & B").unwrap();
    let cells = &data.rows[0].cells;
    assert_eq!(cells[0].value, CellValue::Integer(9007199254740993));
    assert_eq!(cells[1].value, CellValue::Integer(i64::MIN));
    assert!(
        matches!(&cells[2].value, CellValue::BigInteger(value) if value.as_str() == "9223372036854775808")
    );
    assert!(
        matches!(&cells[3].value, CellValue::BigInteger(value) if value.as_str() == "-999999999999999999999999999999")
    );
    assert_eq!(cells[4].value, CellValue::Number(1.0));
    assert_eq!(cells[5].value, CellValue::Number(1.0));
    assert_eq!(cells[6].value, CellValue::error("#DIV/0!"));
    assert_eq!(cells[7].value, CellValue::error("#FUTURE!"));
    assert_eq!(
        cells[8].value,
        CellValue::text(" \t&<![CDATA[ignored]]>\n ")
    );
    assert_eq!(cells[9].value, CellValue::text(""));
    assert_eq!(cells[10].value, CellValue::text(" preserved "));
    assert_eq!(size_of::<CellValue>(), 16);
}
#[test]
fn inline_plain_projection_and_malformed_structure() {
    let mut book = open(
        "<row><c t=\"inlineStr\"><is><r><rPr><b/></rPr><t> rich </t></r><r><t>&amp;tail</t></r><rPh sb=\"0\" eb=\"1\"><t>pronunciation</t></rPh></is></c></row>",
    );
    assert_eq!(
        book.rows("A & B")
            .unwrap()
            .next_row()
            .unwrap()
            .unwrap()
            .cells[0]
            .value,
        CellValue::text(" rich &tail")
    );
    for content in [
        "<is><t>a</t><t>b</t></is>",
        "<is/><is/>",
        "<v>incorrect</v>",
    ] {
        let mut book = open(&format!("<row><c t=\"inlineStr\">{content}</c></row>"));
        assert_eq!(
            book.rows("A & B").unwrap().next_row().unwrap_err().kind(),
            ErrorKind::InvalidData
        );
    }
}
#[test]
fn owned_text_payloads_are_counted_in_row_batch_and_materialization_limits() {
    let text = "x".repeat(1000);
    let document = format!(
        "<worksheet xmlns=\"{MAIN}\"><sheetData><row><c t=\"inlineStr\"><is><t>{text}</t></is></c></row></sheetData></worksheet>"
    );
    let parts = entries(&document);
    let refs: Vec<_> = parts
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    let bytes = fixture(&refs);
    let limits = ResourceLimits {
        max_row_bytes: 600,
        ..ResourceLimits::default()
    };
    let mut book = WorkbookReader::with_limits(Cursor::new(bytes.clone()), limits).unwrap();
    assert_eq!(
        book.rows("A & B").unwrap().next_row().unwrap_err().kind(),
        ErrorKind::LimitExceeded
    );
    let limits = ResourceLimits {
        max_batch_rows: 1,
        max_batch_bytes: 600,
        ..ResourceLimits::default()
    };
    let mut book = WorkbookReader::with_limits(Cursor::new(bytes.clone()), limits).unwrap();
    assert_eq!(
        book.rows("A & B").unwrap().read_batch().unwrap_err().kind(),
        ErrorKind::LimitExceeded
    );
    let limits = ResourceLimits {
        max_materialized_bytes: 600,
        ..ResourceLimits::default()
    };
    let mut book = WorkbookReader::with_limits(Cursor::new(bytes), limits).unwrap();
    assert_eq!(
        book.read_sheet("A & B").unwrap_err().kind(),
        ErrorKind::MemoryBudgetExceeded
    );
    let batch = book.rows("A & B").unwrap().read_batch().unwrap().unwrap();
    assert!(batch.memory_bytes() >= 1000);
    drop(book);
    assert_eq!(
        batch.rows[0].cells[0].value,
        CellValue::text(text.into_boxed_str())
    );
}
#[test]
fn adaptive_text_growth_falls_back_without_losing_owned_values() {
    let first = format!("<row><c><v>{}1</v></c></row>", "0".repeat(5000)).repeat(128);
    let later = format!(
        "<row><c t=\"inlineStr\"><is><t>{}</t></is></c></row>",
        "x".repeat(512)
    )
    .repeat(200);
    let document =
        format!("<worksheet xmlns=\"{MAIN}\"><sheetData>{first}{later}</sheetData></worksheet>");
    let parts = entries(&document);
    let refs: Vec<_> = parts
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    let limits = ResourceLimits {
        input_buffer_bytes: 256,
        max_xml_event_bytes: 16384,
        max_cell_bytes: 8192,
        max_row_bytes: 1024,
        ..ResourceLimits::default()
    };
    let mut book = WorkbookReader::with_limits(Cursor::new(fixture(&refs)), limits).unwrap();
    let output = book
        .read_with_policy(
            "A & B",
            AccessPattern::RepeatedAccess,
            MemoryPolicy::Budget(256 * 1024),
        )
        .unwrap();
    assert_eq!(
        output.decision.reason,
        DecisionReason::ActualDataExceedsBudget
    );
    match output.data {
        ReadData::Streaming(rows) => {
            let values = rows.collect::<Result<Vec<_>, _>>().unwrap();
            assert_eq!(values.len(), 328);
            assert_eq!(
                values[128].cells[0].value,
                CellValue::text("x".repeat(512).into_boxed_str())
            );
        }
        ReadData::Materialized(_) => panic!("Heterogeneous payload must exceed budget"),
    }
}

#[test]
fn plain_text_normalizes_raw_xml_newlines_but_preserves_character_references() {
    let mut book = open(
        "<row><c t=\"inlineStr\"><is><t>raw\r\nline\r<![CDATA[cdata\r\n]]>&#13;ref</t></is></c></row>",
    );
    assert_eq!(
        book.read_sheet("A & B").unwrap().rows[0].cells[0].value,
        CellValue::text("raw\nline\ncdata\n\rref")
    );
}

#[test]
fn formula_payload_limits_projection_and_invalid_structure_are_enforced() {
    use crabxl_core::CellValue;
    let text = "x".repeat(1024);
    let content = format!(
        "<row><c t=\"str\"><f>CONCAT(&quot;{text}&quot;)</f><v>{text}</v></c><c><v>7</v></c></row>"
    );
    let mut book = open(&content);
    let row = book.rows("A & B").unwrap().next_row().unwrap().unwrap();
    assert!(row.memory_bytes() > 2048);
    assert!(matches!(row.cells[0].value, CellValue::Formula(_)));
    let row = book
        .rows_with_options("A & B", columns(1, 1))
        .unwrap()
        .next_row()
        .unwrap()
        .unwrap();
    assert_eq!(row.cells[0].value, CellValue::Integer(7));
    for content in [
        "<row><c><f>1</f><f>2</f></c></row>",
        "<row><c><f>1</f><v>1</v><v>2</v></c></row>",
    ] {
        assert_eq!(
            open(content)
                .rows("A & B")
                .unwrap()
                .next_row()
                .unwrap_err()
                .kind(),
            ErrorKind::InvalidData
        );
    }
    let mut parts = entries(&format!(
        "<worksheet xmlns=\"{MAIN}\"><sheetData>{content}</sheetData></worksheet>"
    ));
    let refs: Vec<_> = parts
        .iter_mut()
        .map(|(n, v)| (n.as_str(), v.as_str()))
        .collect();
    for limits in [
        ResourceLimits {
            max_cell_bytes: 100,
            ..ResourceLimits::default()
        },
        ResourceLimits {
            max_row_bytes: 2000,
            ..ResourceLimits::default()
        },
    ] {
        let mut book = WorkbookReader::with_limits(Cursor::new(fixture(&refs)), limits).unwrap();
        assert_eq!(
            book.rows("A & B").unwrap().next_row().unwrap_err().kind(),
            ErrorKind::LimitExceeded
        );
    }
}

#[test]
fn active_view_uses_first_direct_namespaced_view_and_handles_invalid_indexes() {
    for (views, expected) in [
        ("", Some(0)),
        (
            "<bookViews><workbookView activeTab=\"0\"/><workbookView activeTab=\"99\"/></bookViews>",
            Some(0),
        ),
        (
            "<bookViews><workbookView activeTab=\"99\"/></bookViews>",
            None,
        ),
        (
            "<bookViews xmlns=\"urn:opaque\"><workbookView activeTab=\"99\"/></bookViews>",
            Some(0),
        ),
        (
            "<ext><bookViews><workbookView activeTab=\"99\"/></bookViews></ext>",
            Some(0),
        ),
    ] {
        let mut parts = entries(&format!(
            "<worksheet xmlns=\"{MAIN}\"><sheetData/></worksheet>"
        ));
        parts[2].1 = parts[2].1.replace("<sheets>", &format!("{views}<sheets>"));
        assert_eq!(from_entries(&parts).active_index(), expected);
    }
    let mut parts = entries(&format!(
        "<worksheet xmlns=\"{MAIN}\"><sheetData/></worksheet>"
    ));
    parts[2].1 = parts[2].1.replace(
        "<sheets>",
        "<bookViews><workbookView activeTab=\"invalid\"/></bookViews><sheets>",
    );
    let borrowed: Vec<_> = parts
        .iter()
        .map(|(name, xml)| (name.as_str(), xml.as_str()))
        .collect();
    assert_eq!(
        WorkbookReader::new(Cursor::new(fixture(&borrowed)))
            .err()
            .unwrap()
            .kind(),
        ErrorKind::InvalidData
    );
}

fn with_strings(sheet: &str, strings: &str) -> Vec<u8> {
    let mut parts = entries(&format!(
        "<worksheet xmlns=\"{MAIN}\"><sheetData>{sheet}</sheetData></worksheet>"
    ));
    parts[3].1 = parts[3].1.replace("</Relationships>", &format!("<Relationship Id=\"strings\" Type=\"{REL}/sharedStrings\" Target=\"../data/text.xml\"/></Relationships>"));
    parts.push((
        "data/text.xml".into(),
        format!("<sst xmlns=\"{MAIN}\" uniqueCount=\"999999999999\">{strings}</sst>"),
    ));
    fixture(
        &parts
            .iter()
            .map(|(n, v)| (n.as_str(), v.as_str()))
            .collect::<Vec<_>>(),
    )
}

#[test]
fn shared_text_modes_preserve_ids_entities_whitespace_and_owned_lifetimes() {
    use crabxl_xlsx::{SharedStringOptions, SharedStringStorage};
    for storage in [
        SharedStringStorage::Memory,
        SharedStringStorage::Disk,
        SharedStringStorage::Auto,
    ] {
        let temp = tempfile::tempdir().unwrap();
        let bytes = with_strings(
            "<row><c t=\"s\"><v>2</v></c><c t=\"s\"><v>0</v></c><c t=\"s\"><v>1</v></c><c t=\"s\"><v>2</v></c></row>",
            "<si><t>  a &amp; b &#x1F980;  </t></si><si/><si><t>_x005F_x0041_</t></si>",
        );
        let mut book = WorkbookReader::new(Cursor::new(bytes)).unwrap();
        book.set_shared_string_options(SharedStringOptions {
            storage,
            temp_directory: Some(temp.path().to_owned()),
            ..SharedStringOptions::default()
        });
        assert!(book.shared_string_stats().is_none());
        let batch = book.rows("A & B").unwrap().read_batch().unwrap().unwrap();
        let stats = book.shared_string_stats().unwrap();
        assert_eq!(stats.entries, 3);
        assert_eq!(stats.disk_backed, storage == SharedStringStorage::Disk);
        if stats.disk_backed {
            assert_eq!(stats.cache_hits, 1);
            assert_eq!(stats.disk_reads, 3);
        }
        assert_eq!(batch.rows[0].cells[0].value, CellValue::text("_x0041_"));
        assert_eq!(
            batch.rows[0].cells[1].value,
            CellValue::text("  a & b 🦀  ")
        );
        assert_eq!(batch.rows[0].cells[2].value, CellValue::text(""));
        let again = book.rows("A & B").unwrap().next_row().unwrap().unwrap();
        assert_eq!(again, batch.rows[0]);
        drop(book);
        assert_eq!(batch.rows[0].cells[3].value, CellValue::text("_x0041_"));
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    }
}

#[test]
fn adaptive_shared_text_spills_payload_and_index_without_losing_ids() {
    use crabxl_xlsx::{SharedStringOptions, SharedStringStorage};
    let text = "z".repeat(40_000);
    let strings = format!("<si><t>{text}</t></si>").repeat(10);
    let bytes = with_strings(
        "<row><c t=\"s\"><v>9</v></c><c t=\"s\"><v>0</v></c></row>",
        &strings,
    );
    let mut book = WorkbookReader::new(Cursor::new(bytes.clone())).unwrap();
    let working = crabxl_xlsx::memory_allowance(
        MemoryPolicy::Budget(4 * 1024 * 1024),
        ResourceLimits::default(),
    )
    .unwrap()
    .working_reserve_bytes;
    let options = SharedStringOptions {
        memory_policy: MemoryPolicy::Budget(working + 100_000),
        cache_bytes: 64_000,
        ..SharedStringOptions::default()
    };
    book.set_shared_string_options(options.clone());
    let row = book.rows("A & B").unwrap().next_row().unwrap().unwrap();
    assert_eq!(row.cells[0].value, CellValue::text(text.as_str()));
    assert_eq!(row.cells[1].value, row.cells[0].value);
    let stats = book.shared_string_stats().unwrap();
    assert!(stats.disk_backed);
    assert_eq!(stats.temp_bytes, 400_000 + 10 * 16);
    assert!(stats.managed_bytes <= 64_000);
    book.set_shared_string_options(SharedStringOptions {
        storage: SharedStringStorage::Memory,
        ..options
    });
    assert!(matches!(book.rows("A & B"),Err(e) if e.kind()==ErrorKind::MemoryBudgetExceeded));
    assert!(book.shared_string_stats().is_none());
}

#[test]
fn shared_string_limits_invalid_ids_and_deferred_rich_entries() {
    use crabxl_xlsx::{SharedStringOptions, SharedStringStorage};
    for id in ["-1", "1", "18446744073709551616", "not-an-id", ""] {
        let mut book = WorkbookReader::new(Cursor::new(with_strings(
            &format!("<row><c t=\"s\"><v>{id}</v></c></row>"),
            "<si><t>x</t></si>",
        )))
        .unwrap();
        let e = book.rows("A & B").unwrap().next_row().unwrap_err();
        assert_eq!(e.kind(), ErrorKind::InvalidData);
        assert_eq!(e.cell().unwrap().to_string(), "A1");
    }
    let bytes = with_strings(
        "<row><c t=\"s\"><v>0</v></c><c t=\"s\"><v>1</v></c></row>",
        "<si><r><rPr><b/></rPr><t>rich</t></r></si><si><t>plain</t></si>",
    );
    for storage in [SharedStringStorage::Memory, SharedStringStorage::Disk] {
        let mut book = WorkbookReader::new(Cursor::new(bytes.clone())).unwrap();
        book.set_shared_string_options(SharedStringOptions {
            storage,
            ..SharedStringOptions::default()
        });
        assert_eq!(
            book.rows_with_options("A & B", columns(1, 1))
                .unwrap()
                .next_row()
                .unwrap()
                .unwrap()
                .cells[0]
                .value,
            CellValue::text("plain")
        );
        assert_eq!(
            book.rows("A & B")
                .unwrap()
                .next_row()
                .unwrap()
                .unwrap()
                .cells[0]
                .value,
            CellValue::text("rich")
        );
        let typed = book
            .rows_with_options(
                "A & B",
                ReadOptions {
                    rich_text: true,
                    ..ReadOptions::default()
                },
            )
            .unwrap()
            .next_row()
            .unwrap()
            .unwrap();
        let CellValue::RichText(value) = &typed.cells[0].value else {
            panic!("Expected rich text")
        };
        assert_eq!(value.runs[0].font.as_ref().unwrap().bold, Some(true));
        assert_eq!(value.runs[0].text.as_ref(), "rich");
    }
    for (entries, temp) in [(0, 1024), (1, 0)] {
        let directory = tempfile::tempdir().unwrap();
        let mut book =
            WorkbookReader::new(Cursor::new(with_strings("<row/>", "<si><t>text</t></si>")))
                .unwrap();
        book.set_shared_string_options(SharedStringOptions {
            storage: SharedStringStorage::Disk,
            max_entries: entries,
            max_temp_bytes: temp,
            temp_directory: Some(directory.path().into()),
            ..SharedStringOptions::default()
        });
        assert!(matches!(book.rows("A & B"),Err(e) if e.kind()==ErrorKind::LimitExceeded));
        assert!(book.shared_string_stats().is_none());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    }
}

#[test]
fn shared_string_crc_and_xml_failures_leave_no_prepared_table() {
    let mut bytes = with_strings("<row/>", "<si><t>marker</t></si>");
    let position = bytes.windows(6).position(|s| s == b"marker").unwrap();
    bytes[position] = b'M';
    let mut book = WorkbookReader::new(Cursor::new(bytes)).unwrap();
    assert!(book.rows("A & B").is_err());
    assert!(book.shared_string_stats().is_none());
    for content in [
        "<si><t>x</t><t>y</t></si>",
        "<si><t>&unknown;</t></si>",
        "<wrong/>",
        "<si><t>x</si>",
        "<si>unexpected</si>",
    ] {
        let mut book = WorkbookReader::new(Cursor::new(with_strings("<row/>", content))).unwrap();
        assert!(book.rows("A & B").is_err(), "{content}");
        assert!(book.shared_string_stats().is_none());
    }
}

#[test]
fn shared_string_zero_cache_and_reconfiguration_release_storage() {
    use crabxl_xlsx::{SharedStringOptions, SharedStringStorage};
    let directory = tempfile::tempdir().unwrap();
    let mut book = WorkbookReader::new(Cursor::new(with_strings(
        "<row><c t=\"s\"><v>0</v></c><c t=\"s\"><v>0</v></c></row>",
        "<si><t>value</t></si>",
    )))
    .unwrap();
    book.set_shared_string_options(SharedStringOptions {
        storage: SharedStringStorage::Disk,
        cache_bytes: 0,
        temp_directory: Some(directory.path().into()),
        ..SharedStringOptions::default()
    });
    book.rows("A & B").unwrap().next_row().unwrap();
    let stats = book.shared_string_stats().unwrap();
    assert_eq!(stats.disk_reads, 2);
    assert_eq!(stats.cache_hits, 0);
    assert_eq!(stats.managed_bytes, 0);
    book.set_shared_string_options(SharedStringOptions {
        storage: SharedStringStorage::Memory,
        ..SharedStringOptions::default()
    });
    assert!(book.shared_string_stats().is_none());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    assert_eq!(
        book.read_sheet("A & B").unwrap().rows[0].cells[0].value,
        CellValue::text("value")
    );
    assert!(!book.shared_string_stats().unwrap().disk_backed);
}

#[test]
fn shared_string_auto_availability_and_strict_namespaces() {
    use crabxl_xlsx::{SharedStringOptions, SharedStringStorage};
    let strings = format!("<si><t>{}</t></si>", "x".repeat(40_000)).repeat(10);
    let bytes = with_strings("<row><c t=\"s\"><v>9</v></c></row>", &strings);
    let working = crabxl_xlsx::memory_allowance(
        MemoryPolicy::Budget(4 * 1024 * 1024),
        ResourceLimits::default(),
    )
    .unwrap()
    .working_reserve_bytes;
    let mut book = WorkbookReader::new(Cursor::new(bytes)).unwrap();
    book.set_shared_string_options(SharedStringOptions {
        storage: SharedStringStorage::Auto,
        memory_policy: MemoryPolicy::Auto(AutoMemory {
            available_bytes: Some(((working + 100_000) * 2) as u64),
            headroom_bytes: 0,
            fraction_per_mille: 500,
            maximum_bytes: None,
        }),
        ..SharedStringOptions::default()
    });
    book.rows("A & B").unwrap().next_row().unwrap();
    let stats = book.shared_string_stats().unwrap();
    assert!(stats.disk_backed);
    assert_eq!(stats.retained_allowance_bytes, 100_000);
    assert!(stats.managed_bytes <= 100_000);
    let mut parts = entries(&format!(
        "<worksheet xmlns=\"{MAIN}\"><sheetData><row><c t=\"s\"><v>0</v></c></row></sheetData></worksheet>"
    ));
    parts[3].1=parts[3].1.replace("</Relationships>","<Relationship Id=\"sst\" Type=\"http://purl.oclc.org/ooxml/officeDocument/relationships/sharedStrings\" Target=\"../data/strings.xml\"/></Relationships>");
    parts.push(("data/strings.xml".into(),"<s:sst xmlns:s=\"http://purl.oclc.org/ooxml/spreadsheetml/main\"><s:si><s:t>strict</s:t></s:si></s:sst>".into()));
    let mut book = from_entries(&parts);
    assert_eq!(
        book.rows("A & B")
            .unwrap()
            .next_row()
            .unwrap()
            .unwrap()
            .cells[0]
            .value,
        CellValue::text("strict")
    );
}

#[test]
fn shared_rich_metadata_upgrade_disk_cache_and_protected_run_boundaries() {
    use crabxl_xlsx::{SharedStringOptions, SharedStringStorage};
    let body = "<si><r><rPr><b/><i val=\"0\"/><color theme=\"2\" tint=\"0.25\"/></rPr><t>_x005F</t></r><r><t>_x0041_</t></r><rPh sb=\"0\" eb=\"1\"><t>annotation</t></rPh><phoneticPr fontId=\"0\" type=\"Hiragana\" alignment=\"center\"/></si>";
    for storage in [
        SharedStringStorage::Memory,
        SharedStringStorage::Disk,
        SharedStringStorage::Auto,
    ] {
        let directory = tempfile::tempdir().unwrap();
        let mut book = WorkbookReader::new(Cursor::new(with_strings(
            "<row><c t=\"s\"><v>0</v></c><c t=\"s\"><v>0</v></c></row>",
            body,
        )))
        .unwrap();
        book.set_shared_string_options(SharedStringOptions {
            storage,
            temp_directory: Some(directory.path().into()),
            ..SharedStringOptions::default()
        });
        let plain = book.rows("A & B").unwrap().next_row().unwrap().unwrap();
        assert_eq!(plain.cells[0].value, CellValue::text("_x0041_"));
        assert!(!book.shared_string_stats().unwrap().rich_text_preserved);
        let typed = book
            .rows_with_options(
                "A & B",
                ReadOptions {
                    rich_text: true,
                    ..ReadOptions::default()
                },
            )
            .unwrap()
            .next_row()
            .unwrap()
            .unwrap();
        let CellValue::RichText(value) = &typed.cells[0].value else {
            panic!("Expected rich text")
        };
        assert_eq!(value.plain_text().unwrap().as_ref(), "_x005F_x0041_");
        assert_eq!(value.runs[0].font.as_ref().unwrap().bold, Some(true));
        assert_eq!(value.runs[0].font.as_ref().unwrap().italic, Some(false));
        assert_eq!(value.phonetic_runs[0].text.as_ref(), "annotation");
        assert_eq!(value.phonetic_properties.as_ref().unwrap().font_id, 0);
        assert_eq!(typed.cells[1].value, typed.cells[0].value);
        let stats = book.shared_string_stats().unwrap();
        assert!(stats.rich_text_preserved);
        if storage == SharedStringStorage::Disk {
            assert_eq!(stats.disk_reads, 1);
            assert_eq!(stats.cache_hits, 1);
            assert!(stats.temp_bytes > body.len() as u64);
        }
        let again = book.rows("A & B").unwrap().next_row().unwrap().unwrap();
        assert_eq!(again, plain);
        drop(book);
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
        assert_eq!(value.runs[1].text.as_ref(), "_x0041_");
    }
}

#[test]
fn rich_unknown_properties_are_projectable_but_typed_access_is_explicit() {
    use crabxl_xlsx::{SharedStringOptions, SharedStringStorage};
    for storage in [SharedStringStorage::Memory, SharedStringStorage::Disk] {
        let body = "<si><r><rPr><future/></rPr><t>visible</t></r></si><si><t>plain</t></si>";
        let mut book = WorkbookReader::new(Cursor::new(with_strings(
            "<row><c t=\"s\"><v>0</v></c><c t=\"s\"><v>1</v></c></row>",
            body,
        )))
        .unwrap();
        book.set_shared_string_options(SharedStringOptions {
            storage,
            ..SharedStringOptions::default()
        });
        assert_eq!(
            book.rows("A & B")
                .unwrap()
                .next_row()
                .unwrap()
                .unwrap()
                .cells[0]
                .value,
            CellValue::text("visible")
        );
        assert_eq!(
            book.rows_with_options(
                "A & B",
                ReadOptions {
                    rich_text: true,
                    ..columns(1, 1)
                }
            )
            .unwrap()
            .next_row()
            .unwrap()
            .unwrap()
            .cells[0]
                .value,
            CellValue::text("plain")
        );
        let error = book
            .rows_with_options(
                "A & B",
                ReadOptions {
                    rich_text: true,
                    ..ReadOptions::default()
                },
            )
            .unwrap()
            .next_row()
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Unsupported);
        assert_eq!(error.cell().unwrap().to_string(), "A1");
    }
}

#[test]
fn rich_projection_limits_and_illegal_characters_reject_partial_rows() {
    for body in [
        "<is><r><t>bad\0text</t></r></is>",
        "<is><t>&#xFFFF;</t></is>",
    ] {
        let mut book = open(&format!("<row><c t=\"inlineStr\">{body}</c></row>"));
        assert_eq!(
            book.rows("A & B").unwrap().next_row().unwrap_err().kind(),
            ErrorKind::InvalidData
        );
    }
    let mut parts = entries(&format!(
        "<worksheet xmlns=\"{MAIN}\"><sheetData><row><c t=\"inlineStr\"><is><r><t>12345</t></r><r><t>67890</t></r></is></c></row></sheetData></worksheet>"
    ));
    let data = fixture(
        &parts
            .iter()
            .map(|(n, v)| (n.as_str(), v.as_str()))
            .collect::<Vec<_>>(),
    );
    let mut book = WorkbookReader::with_limits(
        Cursor::new(data),
        ResourceLimits {
            max_cell_bytes: 9,
            ..ResourceLimits::default()
        },
    )
    .unwrap();
    assert_eq!(
        book.rows("A & B").unwrap().next_row().unwrap_err().kind(),
        ErrorKind::LimitExceeded
    );
    parts[4].1 = parts[4].1.replace(
        "<t>12345</t>",
        "<rPr><color rgb=\"FF000000\" tint=\"2\"/></rPr><t>12345</t>",
    );
    let mut book = from_entries(&parts);
    assert_eq!(
        book.rows_with_options(
            "A & B",
            ReadOptions {
                rich_text: true,
                ..ReadOptions::default()
            }
        )
        .unwrap()
        .next_row()
        .unwrap_err()
        .kind(),
        ErrorKind::InvalidData
    );
}

#[test]
fn rich_metadata_budget_spill_cache_bypass_and_failure_cleanup() {
    use crabxl_xlsx::{SharedStringOptions, SharedStringStorage};
    let text = "x".repeat(1000);
    let rich = format!("<si><r><rPr><b/></rPr><t>{text}</t></r></si>");
    let bytes = with_strings(
        "<row><c t=\"s\"><v>9</v></c><c t=\"s\"><v>9</v></c></row>",
        &rich.repeat(10),
    );
    let directory = tempfile::tempdir().unwrap();
    let mut book = WorkbookReader::new(Cursor::new(bytes)).unwrap();
    let reserve = crabxl_xlsx::memory_allowance(
        MemoryPolicy::Budget(4 * 1024 * 1024),
        ResourceLimits::default(),
    )
    .unwrap()
    .working_reserve_bytes;
    let options = SharedStringOptions {
        memory_policy: MemoryPolicy::Budget(reserve + 4000),
        cache_bytes: 500,
        temp_directory: Some(directory.path().into()),
        ..Default::default()
    };
    book.set_shared_string_options(options.clone());
    let projection = ReadOptions {
        rich_text: true,
        ..Default::default()
    };
    let row = book
        .rows_with_options("A & B", projection.clone())
        .unwrap()
        .next_row()
        .unwrap()
        .unwrap();
    assert_eq!(row.cells[0].value, row.cells[1].value);
    let CellValue::RichText(value) = &row.cells[0].value else {
        panic!("Expected rich text")
    };
    assert_eq!(value.runs[0].text.as_ref(), text);
    assert_eq!(value.runs[0].font.as_ref().unwrap().bold, Some(true));
    let stats = book.shared_string_stats().unwrap();
    assert!(stats.disk_backed);
    assert!(stats.temp_bytes > 10_000 + 10 * 16);
    assert!(stats.managed_bytes <= 500);
    assert_eq!(stats.disk_reads, 2);
    assert_eq!(stats.cache_hits, 0);
    book.set_shared_string_options(SharedStringOptions {
        storage: SharedStringStorage::Memory,
        ..options.clone()
    });
    assert!(
        matches!(book.rows_with_options("A & B", projection.clone()), Err(e) if e.kind() == ErrorKind::MemoryBudgetExceeded)
    );
    assert!(book.shared_string_stats().is_none());
    book.set_shared_string_options(SharedStringOptions {
        storage: SharedStringStorage::Disk,
        max_temp_bytes: 100,
        ..options
    });
    assert!(
        matches!(book.rows_with_options("A & B", projection), Err(e) if e.kind() == ErrorKind::LimitExceeded)
    );
    assert!(book.shared_string_stats().is_none());
    drop(book);
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    assert_eq!(value.runs[0].text.as_ref(), text);
}

#[test]
fn typed_inline_protection_matches_public_reference_per_run() {
    for (content, plain, typed) in [
        ("<t>_x005F_x0041_</t>", "_x005F_x0041_", "_x0041_"),
        ("<r><t>_x005F_x0041_</t></r>", "_x005F_x0041_", "_x0041_"),
        (
            "<r><rPr><b/></rPr><t>_x005F_x0041_</t></r>",
            "_x005F_x0041_",
            "_x0041_",
        ),
        (
            "<r><t>_x005F</t></r><r><t>_x0041_</t></r>",
            "_x005F_x0041_",
            "_x005F_x0041_",
        ),
    ] {
        let mut book = open(&format!(
            "<row><c t=\"inlineStr\"><is>{content}</is></c></row>"
        ));
        assert_eq!(
            book.rows("A & B")
                .unwrap()
                .next_row()
                .unwrap()
                .unwrap()
                .cells[0]
                .value,
            CellValue::text(plain)
        );
        let row = book
            .rows_with_options(
                "A & B",
                ReadOptions {
                    rich_text: true,
                    ..Default::default()
                },
            )
            .unwrap()
            .next_row()
            .unwrap()
            .unwrap();
        let display = match &row.cells[0].value {
            CellValue::RichText(value) => value.plain_text().unwrap(),
            CellValue::Text(value) => value.as_str().into(),
            _ => panic!("Expected text"),
        };
        assert_eq!(display.as_ref(), typed);
    }
}

fn with_styles(sheet: &str, styles: &str, date_1904: bool) -> Vec<u8> {
    let mut parts = entries(&format!(
        "<worksheet xmlns=\"{MAIN}\"><sheetData>{sheet}</sheetData></worksheet>"
    ));
    for (name, value) in &mut parts {
        if name == "book/workbook.xml" && !date_1904 {
            *value = value.replace("date1904=\"1\"", "date1904=\"0\"");
        }
        if name == "book/_rels/workbook.xml.rels" {
            *value=value.replace("</Relationships>",&format!("<Relationship Id=\"styles\" Type=\"{REL}/styles\" Target=\"../meta/styles.xml\"/></Relationships>"));
        }
        if name == "[Content_Types].xml" {
            *value=value.replace("</Types>","<Override PartName=\"/meta/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/></Types>");
        }
    }
    parts.push((
        "meta/styles.xml".into(),
        format!("<styleSheet xmlns=\"{MAIN}\">{styles}</styleSheet>"),
    ));
    let refs: Vec<_> = parts
        .iter()
        .map(|(n, v)| (n.as_str(), v.as_str()))
        .collect();
    fixture(&refs)
}
fn basic_styles(formats: &str) -> String {
    format!(
        "<numFmts><numFmt numFmtId=\"164\" formatCode=\"[h]:mm:ss.000\"/><numFmt numFmtId=\"4294967295\" formatCode=\"0.00 &quot;d&quot;\"/></numFmts><fonts count=\"4294967295\"><font><name val=\"Calibri\"/><sz val=\"11\"/></font></fonts><fills><fill><patternFill patternType=\"none\"/></fill></fills><borders><border/></borders><cellStyleXfs><xf/></cellStyleXfs><cellXfs>{formats}</cellXfs><cellStyles><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles>"
    )
}

#[test]
fn styled_dates_epochs_duration_cached_formulas_and_general_values_stream() {
    use crabxl_core::{DateKind, DateReadPolicy};
    let styles = basic_styles(
        "<xf/><xf numFmtId=\"14\"/><xf numFmtId=\"164\"/><xf numFmtId=\"4294967295\"/>",
    );
    let sheet = "<row><c s=\"1\"><v>0</v></c><c s=\"1\"><v>0.5</v></c><c s=\"1\"><v>0.99999999999</v></c><c s=\"1\"><v>59</v></c><c s=\"1\"><v>60</v></c><c s=\"1\"><v>61</v></c><c s=\"1\"><v>-0.5</v></c><c s=\"1\"><v>2958466</v></c><c s=\"2\"><v>1.25</v></c><c s=\"3\"><v>1.5</v></c><c s=\"1\" t=\"b\"><v>0</v></c><c s=\"1\" t=\"inlineStr\"><is><t>text</t></is></c><c s=\"1\"><f>1</f><v>61</v></c><c s=\"1\"><f>1</f></c></row>";
    for mac in [false, true] {
        let mut book = WorkbookReader::new(Cursor::new(with_styles(sheet, &styles, mac))).unwrap();
        let row = book.rows("A & B").unwrap().next_row().unwrap().unwrap();
        let date = |i: usize| {
            let CellValue::DateTime(value) = &row.cells[i].value else {
                panic!("Expected date/time at {i}")
            };
            **value
        };
        assert_eq!(date(0).kind(), DateKind::Time);
        assert_eq!(date(0).to_time().unwrap().to_string(), "00:00:00");
        assert_eq!(date(1).to_time().unwrap().to_string(), "12:00:00");
        assert_eq!(date(2).kind(), DateKind::DateTime);
        assert_eq!(
            date(2).to_datetime().unwrap().to_string(),
            if mac {
                "1904-01-02 00:00:00"
            } else {
                "1900-01-01 00:00:00"
            }
        );
        assert_eq!(date(4).serial(), 60.0);
        assert_eq!(
            date(4).to_datetime().unwrap().to_string(),
            if mac {
                "1904-03-01 00:00:00"
            } else {
                "1900-02-28 00:00:00"
            }
        );
        assert_eq!(
            date(6).to_datetime().unwrap().to_string(),
            if mac {
                "1903-12-31 12:00:00"
            } else {
                "1899-12-29 12:00:00"
            }
        );
        assert_eq!(row.cells[7].value, CellValue::error("#VALUE!"));
        assert_eq!(date(8).kind(), DateKind::Duration);
        assert_eq!(date(8).to_duration().unwrap().num_seconds(), 108000);
        assert_eq!(row.cells[9].value, CellValue::Number(1.5));
        assert_eq!(row.cells[10].value, CellValue::Boolean(false));
        assert_eq!(row.cells[11].value, CellValue::text("text"));
        let CellValue::Formula(formula) = &row.cells[12].value else {
            panic!("Expected formula")
        };
        assert!(matches!(formula.cached(), Some(CellValue::DateTime(_))));
        let CellValue::Formula(missing) = &row.cells[13].value else {
            panic!("Expected formula")
        };
        assert!(missing.cached().is_none());
        assert_eq!(row.cells[9].style.get(), 3);
        let cached = book
            .rows_with_options(
                "A & B",
                ReadOptions {
                    data_only: true,
                    ..Default::default()
                },
            )
            .unwrap()
            .next_row()
            .unwrap()
            .unwrap();
        assert_eq!(cached.cells[12].value, formula.cached().unwrap().clone());
        assert_eq!(cached.cells[13].value, CellValue::Empty);
        let raw = book
            .rows_with_options(
                "A & B",
                ReadOptions {
                    date_policy: DateReadPolicy::RetainSerial,
                    ..Default::default()
                },
            )
            .unwrap()
            .next_row()
            .unwrap()
            .unwrap();
        assert!(matches!(&raw.cells[7].value,CellValue::DateTime(v) if v.serial()==2958466.0));
        assert!(book.style_memory_bytes() < 16000);
        assert_eq!(
            book.style_catalog().unwrap().unwrap().number_formats.len(),
            2
        );
    }
}

#[test]
fn style_zero_date_format_is_interpreted_and_missing_format_context_is_explicit() {
    let styles = basic_styles("<xf numFmtId=\"14\"/>");
    let mut book = WorkbookReader::new(Cursor::new(with_styles(
        "<row><c><v>61</v></c></row>",
        &styles,
        false,
    )))
    .unwrap();
    assert!(matches!(
        book.rows("A & B")
            .unwrap()
            .next_row()
            .unwrap()
            .unwrap()
            .cells[0]
            .value,
        CellValue::DateTime(_)
    ));
    for styled in [false, true] {
        let mut book = if styled {
            WorkbookReader::new(Cursor::new(with_styles(
                "<row><c s=\"9\"><v>1</v></c></row>",
                &styles,
                false,
            )))
            .unwrap()
        } else {
            open("<row><c s=\"1\"><v>1</v></c></row>")
        };
        let error = book.rows("A & B").unwrap().next_row().unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
        assert_eq!(error.cell().unwrap().to_string(), "A1");
    }
}

#[test]
fn imported_style_components_keep_ids_optional_overrides_palettes_and_staged_sections() {
    use crabxl_core::{ColorKind, Fill, GradientKind, Underline};
    let styles = "<fonts count=\"4000000000\"><font><name val=\"Named\"/><sz val=\"12.5\"/><b val=\"0\"/><i/><u val=\"double\"/><color theme=\"7\" tint=\"0.25\"/><scheme val=\"major\"/></font></fonts><fills><fill><gradientFill type=\"path\" left=\"0.1\" right=\"0.2\" top=\"0.3\" bottom=\"0.4\"><stop position=\"0\"><color rgb=\"80112233\"/></stop><stop position=\"1\"><color indexed=\"64\"/></stop></gradientFill></fill></fills><borders><border diagonalUp=\"1\" diagonalDown=\"0\" outline=\"0\"><left/><start style=\"mediumDashDot\"><color auto=\"0\"/></start><diagonal style=\"slantDashDot\"/></border></borders><cellStyleXfs><xf/></cellStyleXfs><cellXfs><xf xfId=\"0\" applyFont=\"0\" quotePrefix=\"1\" pivotButton=\"0\"><alignment horizontal=\"distributed\" vertical=\"justify\" textRotation=\"255\" wrapText=\"1\" shrinkToFit=\"1\" indent=\"2.5\" relativeIndent=\"-1.5\" readingOrder=\"2\"/><protection locked=\"0\" hidden=\"1\"/><extLst/></xf></cellXfs><cellStyles><cellStyle name=\"Visible\" xfId=\"0\" builtinId=\"0\" hidden=\"0\" customBuiltin=\"0\" iLevel=\"1\"/></cellStyles><colors><indexedColors><rgbColor rgb=\"FFAABBCC\"/></indexedColors><mruColors><color theme=\"4\" tint=\"0\"/></mruColors></colors><tableStyles count=\"0\"/><dxfs count=\"0\"/>";
    let mut book = WorkbookReader::new(Cursor::new(with_styles(
        "<row><c><v>7</v></c></row>",
        styles,
        false,
    )))
    .unwrap();
    assert_eq!(
        book.rows("A & B")
            .unwrap()
            .next_row()
            .unwrap()
            .unwrap()
            .cells[0]
            .value,
        CellValue::Integer(7)
    );
    let catalog = book.style_catalog().unwrap().unwrap();
    assert_eq!(catalog.fonts.len(), 1);
    let font = &catalog.fonts[0];
    assert_eq!(font.name.as_deref(), Some("Named"));
    assert_eq!(font.bold, Some(false));
    assert_eq!(font.italic, Some(true));
    assert_eq!(font.underline, Some(Underline::Double));
    assert_eq!(font.color.unwrap().kind, ColorKind::Theme(7));
    let Fill::Gradient(fill) = &catalog.fills[0] else {
        panic!("Expected gradient")
    };
    assert_eq!(fill.kind, Some(GradientKind::Path));
    assert_eq!(fill.edges, [Some(0.1), Some(0.2), Some(0.3), Some(0.4)]);
    assert_eq!(fill.stops[0].color.kind, ColorKind::Argb(0x80112233));
    let border = &catalog.borders[0];
    assert_eq!(border.diagonal_up, Some(true));
    assert_eq!(border.diagonal_down, Some(false));
    assert_eq!(border.outline, Some(false));
    assert!(border.sides[0].is_some());
    assert!(border.sides[1].is_none());
    assert_eq!(
        border.sides[7].unwrap().color.unwrap().kind,
        ColorKind::Auto(false)
    );
    let format = &catalog.cell_formats[0];
    assert_eq!(format.apply_font, Some(false));
    assert_eq!(format.quote_prefix, Some(true));
    assert!(format.unmodeled_extensions);
    assert_eq!(format.alignment.as_ref().unwrap().rotation, Some(255));
    assert_eq!(format.alignment.as_ref().unwrap().indent, Some(2.5));
    assert_eq!(format.protection.unwrap().locked, Some(false));
    assert_eq!(catalog.named_styles[0].name.as_ref(), "Visible");
    assert_eq!(catalog.named_styles[0].hidden, Some(false));
    assert_eq!(catalog.indexed_colors, [0xFFAABBCC]);
    assert_eq!(catalog.recent_colors[0].tint, Some(0.0));
    assert_eq!(
        catalog
            .unmodeled_sections
            .iter()
            .map(|s| s.as_ref())
            .collect::<Vec<_>>(),
        ["tableStyles", "dxfs"]
    );
}

#[test]
fn style_catalog_actual_counts_bytes_bad_components_and_duplicate_ids_are_guarded() {
    let valid = basic_styles("<xf/>");
    for limits in [
        ResourceLimits {
            max_style_records: 1,
            ..Default::default()
        },
        ResourceLimits {
            max_style_bytes: 300,
            ..Default::default()
        },
    ] {
        let mut book =
            WorkbookReader::with_limits(Cursor::new(with_styles("<row/>", &valid, false)), limits)
                .unwrap();
        let error = book.style_catalog().unwrap_err();
        assert_eq!(error.kind(), ErrorKind::LimitExceeded);
        assert_eq!(error.part(), Some("meta/styles.xml"));
        assert_eq!(book.style_memory_bytes(), 0);
    }
    for invalid in [
        valid.replace("<xf/>", "<xf fontId=\"1000000\"/>"),
        valid.replace("<xf/>", "<xf numFmtId=\"165\"/>"),
        valid.replace(
            "<numFmt numFmtId=\"164\"",
            "<numFmt numFmtId=\"4294967295\"",
        ),
        valid.replace(
            "<cellXfs>",
            "<cellXfs><xf><alignment textRotation=\"181\"/></xf>",
        ),
    ] {
        let mut book =
            WorkbookReader::new(Cursor::new(with_styles("<row/>", &invalid, false))).unwrap();
        let error = book.style_catalog().unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
        assert_eq!(book.style_memory_bytes(), 0);
    }
    let unknown = valid.replace("<font>", "<font unsupported=\"1\">");
    let mut book =
        WorkbookReader::new(Cursor::new(with_styles("<row/>", &unknown, false))).unwrap();
    assert_eq!(
        book.style_catalog().unwrap_err().kind(),
        ErrorKind::Unsupported
    );
    assert_eq!(book.style_memory_bytes(), 0);
}

#[test]
fn materialized_date_policy_retains_serials_without_losing_style_identity() {
    use crabxl_core::{DateReadPolicy, StyleId};
    let styles = basic_styles("<xf/><xf numFmtId=\"14\"/>");
    let sheet = "<row><c s=\"1\"><v>2958466</v></c><c s=\"1\"><f>1</f><v>0.5</v></c></row>";
    let mut book = WorkbookReader::new(Cursor::new(with_styles(sheet, &styles, false))).unwrap();
    let compatible = book.read_sheet("A & B").unwrap();
    assert_eq!(
        compatible.rows[0].cells[0].value,
        CellValue::error("#VALUE!")
    );
    let raw = book
        .read_sheet_with_options(
            "A & B",
            ReadOptions {
                data_only: true,
                date_policy: DateReadPolicy::RetainSerial,
                ..Default::default()
            },
        )
        .unwrap();
    let CellValue::DateTime(value) = &raw.rows[0].cells[0].value else {
        panic!("Expected retained date serial")
    };
    assert_eq!(value.serial(), 2958466.0);
    assert_eq!(raw.rows[0].cells[0].style, StyleId::new(1));
    let CellValue::DateTime(time) = &raw.rows[0].cells[1].value else {
        panic!("Expected cached time")
    };
    assert_eq!(time.to_time().unwrap().to_string(), "12:00:00");
}

#[test]
fn iso_cells_dates_clocks_durations_and_formula_caches_share_read_modes() {
    use crabxl_core::DateKind;
    let mut book = open(
        "<row><c t=\"d\"><v>2024-02-29</v></c><c t=\"d\"><v>12:34:56.123456</v></c><c t=\"d\"><v>PT1H2M3.123S</v></c><c t=\"d\"><f>1</f><v>2024-02-29T12:34:56.123</v></c><c t=\"d\"><v/></c></row>",
    );
    let loaded = book.read_sheet("A & B").unwrap();
    let values = &loaded.rows[0].cells;
    let CellValue::DateTime(date) = &values[0].value else {
        panic!("Expected date")
    };
    assert_eq!(date.kind(), DateKind::Date);
    assert_eq!(date.to_date().unwrap().to_string(), "2024-02-29");
    let CellValue::DateTime(clock) = &values[1].value else {
        panic!("Expected clock")
    };
    assert_eq!(clock.to_time().unwrap().to_string(), "12:34:56.123");
    let CellValue::DateTime(duration) = &values[2].value else {
        panic!("Expected duration")
    };
    assert_eq!(duration.to_duration().unwrap().num_milliseconds(), 3723123);
    let CellValue::Formula(formula) = &values[3].value else {
        panic!("Expected formula")
    };
    assert!(matches!(formula.cached(), Some(CellValue::DateTime(_))));
    assert_eq!(values[4].value, CellValue::Empty);
    let cached = book
        .read_sheet_with_options(
            "A & B",
            ReadOptions {
                data_only: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(&cached.rows[0].cells[3].value, formula.cached().unwrap());
    for value in [" 2024-02-29", "2024-02-30", "PT999999999999999999999H"] {
        let mut invalid = open(&format!(
            "<row><c r=\"B1\" t=\"d\"><v>{value}</v></c></row>"
        ));
        let error = invalid.rows("A & B").unwrap().next_row().unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
        assert_eq!(error.cell(), Some(CellAddress::new(0, 1).unwrap()));
    }
}

const SHARED_ANCHOR: &str = "<row r=\"1\"><c r=\"B1\"><f t=\"shared\" si=\"4294967295\" ref=\"A1:D2\">B1+$B$2+C3</f><v>0</v></c><c r=\"D1\"><f t=\"shared\" si=\"4294967295\"/><v>1</v></c></row><row r=\"2\"><c r=\"A2\"><f t=\"shared\" si=\"4294967295\"/><v>2</v></c></row>";

#[test]
fn shared_formulas_use_actual_anchors_sparse_ids_and_projected_dependencies() {
    use crabxl_core::FormulaType;
    let mut book = open(SHARED_ANCHOR);
    let loaded = book.read_sheet("A & B").unwrap();
    for (cell, expected) in loaded.rows.iter().flat_map(|row| &row.cells).zip([
        "B1+$B$2+C3",
        "D1+$B$2+E3",
        "A2+$B$2+B4",
    ]) {
        let CellValue::Formula(formula) = &cell.value else {
            panic!("Expected formula")
        };
        assert_eq!(formula.expression(), expected);
        assert_eq!(formula.formula_type(), FormulaType::Normal);
    }
    let options = ReadOptions {
        rows: Some(RowIndex::new(1).unwrap()..=RowIndex::new(1).unwrap()),
        columns: Some(ColumnIndex::new(0).unwrap()..=ColumnIndex::new(0).unwrap()),
        formula_metadata: true,
        ..Default::default()
    };
    let mut rows = book.rows_with_options("A & B", options).unwrap();
    let row = rows.next_row().unwrap().unwrap();
    let CellValue::Formula(formula) = &row.cells[0].value else {
        panic!("Expected projected formula")
    };
    assert_eq!(formula.expression(), "A2+$B$2+B4");
    assert_eq!(
        formula.formula_type(),
        FormulaType::Shared {
            index: u32::MAX,
            master: false
        }
    );
    assert_eq!(rows.decoded_cells(), 1);
    let stats = rows.shared_formula_stats();
    assert_eq!(stats.templates, 1);
    assert_eq!(stats.expanded, 1);
    assert!(stats.accounted_bytes < 2048);
    assert!(rows.next_row().unwrap().is_none());
    drop(rows);
    let escaped = SHARED_ANCHOR.replace("t=\"shared\"", "t=\"shar&#101;d\"");
    let mut book = open(&escaped);
    let row = book
        .rows_with_options("A & B", columns(3, 3))
        .unwrap()
        .next_row()
        .unwrap()
        .unwrap();
    let CellValue::Formula(formula) = &row.cells[0].value else {
        panic!("Expected escaped dependency")
    };
    assert_eq!(formula.expression(), "D1+$B$2+E3");
}

#[test]
fn shared_compatibility_preserves_first_templates_and_strict_policy_is_explicit() {
    use crabxl_core::FormulaReadPolicy;
    for (content, expected) in [
        (
            "<row><c><f t=\"shared\" si=\"1\"/><v>1</v></c></row>",
            vec![""],
        ),
        (
            "<row><c r=\"A1\"><f t=\"shared\" si=\"1\" ref=\"A1:A2\">A1+1</f></c><c r=\"B1\"><f t=\"shared\" si=\"1\"/></c></row>",
            vec!["A1+1", "B1+1"],
        ),
        (
            "<row><c r=\"A1\"><f t=\"shared\" si=\"1\">A1+1</f></c><c r=\"B1\"><f t=\"shared\" si=\"1\">B1+2</f></c></row>",
            vec!["A1+1", "B1+1"],
        ),
    ] {
        let mut book = open(content);
        let row = book.rows("A & B").unwrap().next_row().unwrap().unwrap();
        for (cell, expression) in row.cells.iter().zip(expected) {
            let CellValue::Formula(value) = &cell.value else {
                panic!("Expected formula")
            };
            assert_eq!(value.expression(), expression);
        }
        let error = book
            .rows_with_options(
                "A & B",
                ReadOptions {
                    formula_policy: FormulaReadPolicy::ValidateGroups,
                    ..Default::default()
                },
            )
            .unwrap()
            .next_row()
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
        assert!(error.cell().is_some());
    }
    let mut book = open(
        "<row><c r=\"A1\"><f t=\"shared\" si=\"7\"/></c><c r=\"B1\"><f t=\"shared\" si=\"7\">B1+2</f></c></row>",
    );
    let row = book
        .rows_with_options("A & B", columns(1, 1))
        .unwrap()
        .next_row()
        .unwrap()
        .unwrap();
    let CellValue::Formula(value) = &row.cells[0].value else {
        panic!("Expected unresolved source")
    };
    assert_eq!(value.expression(), "");
}

#[test]
fn shared_template_counts_bytes_and_cache_only_storage_are_bounded() {
    let content = "<row><c><f t=\"shared\" si=\"4294967295\">A1+1</f><v>7</v></c><c><f t=\"shared\" si=\"1\">B1+1</f><v>8</v></c></row>";
    for limits in [
        ResourceLimits {
            max_shared_formulas: 1,
            ..Default::default()
        },
        ResourceLimits {
            max_formula_table_bytes: 1,
            ..Default::default()
        },
    ] {
        let mut book = WorkbookReader::with_limits(
            Cursor::new(fixture(
                &entries(&format!(
                    "<worksheet xmlns=\"{MAIN}\"><sheetData>{content}</sheetData></worksheet>"
                ))
                .iter()
                .map(|(n, v)| (n.as_str(), v.as_str()))
                .collect::<Vec<_>>(),
            )),
            limits,
        )
        .unwrap();
        let mut rows = book.rows("A & B").unwrap();
        let error = rows.next_row().unwrap_err();
        assert_eq!(error.kind(), ErrorKind::LimitExceeded);
        assert!(rows.next_row().unwrap().is_none());
    }
    let mut book = open(content);
    let mut rows = book
        .rows_with_options(
            "A & B",
            ReadOptions {
                data_only: true,
                ..Default::default()
            },
        )
        .unwrap();
    let row = rows.next_row().unwrap().unwrap();
    assert_eq!(row.cells[0].value, CellValue::Integer(7));
    assert_eq!(row.cells[1].value, CellValue::Integer(8));
    assert_eq!(rows.shared_formula_stats().templates, 0);
}

#[test]
fn array_table_source_properties_caches_and_empty_bodies_remain_distinct() {
    use crabxl_core::FormulaType;
    let mut book = open(
        "<row><c><f t=\"array\" ref=\"$A$1:$B$2\" aca=\"0\">SUM(C1:C2)</f><v>5</v></c><c r=\"D1\"><f t=\"dataTable\" ref=\"D1:E2\" dt2D=\"1\" dtr=\"0\" r1=\"$A$1\" r2=\"B1\" ca=\"0\" del1=\"0\" del2=\"1\"/><v>0</v></c><c r=\"F1\"><f/><v>1</v></c><c r=\"G1\"><f>=1</f></c></row>",
    );
    let loaded = book.read_sheet("A & B").unwrap();
    let cells = &loaded.rows[0].cells;
    let get = |index: usize| {
        let CellValue::Formula(value) = &cells[index].value else {
            panic!("Expected structured formula")
        };
        value
    };
    let array = get(0);
    assert_eq!(array.formula_type(), FormulaType::Array);
    assert_eq!(array.expression(), "SUM(C1:C2)");
    assert_eq!(
        array
            .metadata()
            .unwrap()
            .reference
            .as_ref()
            .unwrap()
            .spelling(),
        "$A$1:$B$2"
    );
    assert!(
        !array
            .metadata()
            .unwrap()
            .flags
            .always_calculate
            .as_ref()
            .unwrap()
            .value()
    );
    let table = get(1);
    assert_eq!(table.formula_type(), FormulaType::DataTable);
    assert_eq!(table.expression(), "");
    let metadata = table.metadata().unwrap();
    assert_eq!(
        metadata.flags.calculate_cell.as_ref().unwrap().source(),
        Some("0")
    );
    let options = metadata.data_table.as_ref().unwrap();
    assert_eq!(options.input1.as_deref(), Some("$A$1"));
    assert!(options.two_dimensions.as_ref().unwrap().value());
    assert_eq!(options.row_table.as_ref().unwrap().source(), Some("0"));
    assert_eq!(table.cached(), Some(&CellValue::Integer(0)));
    assert_eq!(get(2).expression(), "");
    assert_eq!(get(3).expression(), "=1");
    let cached = book
        .read_sheet_with_options(
            "A & B",
            ReadOptions {
                data_only: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(cached.rows[0].cells[0].value, CellValue::Integer(5));
    assert_eq!(cached.rows[0].cells[1].value, CellValue::Integer(0));
    assert_eq!(cached.rows[0].cells[2].value, CellValue::Integer(1));
    assert_eq!(cached.rows[0].cells[3].value, CellValue::Empty);
}

#[test]
fn overflow_numeric_lexemes_and_cached_results_retain_infinities() {
    let mut book =
        open("<row><c><v>1e999</v></c><c><v>-1e999</v></c><c><f>1</f><v>1e999</v></c></row>");
    let sheet = book.read_sheet("A & B").unwrap();
    assert!(
        matches!(sheet.rows[0].cells[0].value, CellValue::Number(value) if value == f64::INFINITY)
    );
    assert!(
        matches!(sheet.rows[0].cells[1].value, CellValue::Number(value) if value == f64::NEG_INFINITY)
    );
    let CellValue::Formula(formula) = &sheet.rows[0].cells[2].value else {
        panic!("Expected formula");
    };
    assert_eq!(formula.cached(), Some(&CellValue::Number(f64::INFINITY)));
    let cached = book
        .read_sheet_with_options(
            "A & B",
            ReadOptions {
                data_only: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        cached.rows[0].cells[2].value,
        CellValue::Number(f64::INFINITY)
    );
    for invalid in ["Infinity", "-inf", "NaN", "NaN.e", "1e999garbage"] {
        assert!(
            open(&format!("<row><c><v>{invalid}</v></c></row>"))
                .read_sheet("A & B")
                .is_err()
        );
    }
}

#[test]
fn lazy_themes_resolve_custom_parts_keep_opaque_bytes_and_enforce_limits() {
    let theme = "<a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" name=\"custom\"><a:extLst/></a:theme>";
    let mut parts = entries(&format!(
        "<worksheet xmlns=\"{MAIN}\"><sheetData><row><c><v>42</v></c></row></sheetData></worksheet>"
    ));
    parts[3].1 = parts[3].1.replace("</Relationships>", &format!("<Relationship Id=\"theme\" Type=\"{REL}/theme\" Target=\"../custom/colors.xml\"/></Relationships>"));
    parts.push(("custom/colors.xml".into(), theme.into()));
    let mut book = from_entries(&parts);
    assert_eq!(book.theme_memory_bytes(), 0);
    assert_eq!(
        book.rows("A & B")
            .unwrap()
            .next_row()
            .unwrap()
            .unwrap()
            .cells[0]
            .value,
        CellValue::Integer(42)
    );
    assert_eq!(book.theme_memory_bytes(), 0);
    assert_eq!(book.theme().unwrap().unwrap().bytes(), theme.as_bytes());
    let retained = book.theme_memory_bytes();
    assert_eq!(book.theme().unwrap().unwrap().bytes(), theme.as_bytes());
    assert_eq!(book.theme_memory_bytes(), retained);
    book.validate_theme().unwrap();
    let refs: Vec<_> = parts
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    let mut limited = WorkbookReader::with_limits(
        Cursor::new(fixture(&refs)),
        ResourceLimits {
            max_theme_bytes: theme.len() - 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        limited.theme().unwrap_err().kind(),
        ErrorKind::LimitExceeded
    );
    assert_eq!(limited.theme_memory_bytes(), 0);
    assert!(limited.rows("A & B").unwrap().next_row().unwrap().is_some());
    parts.last_mut().unwrap().1 = "not XML".into();
    let mut opaque = from_entries(&parts);
    assert_eq!(opaque.theme().unwrap().unwrap().bytes(), b"not XML");
    assert!(opaque.validate_theme().is_err());
    parts[3].1 = parts[3].1.replace("</Relationships>", &format!("<Relationship Id=\"duplicate\" Type=\"{REL}/theme\" Target=\"../custom/colors.xml\"/></Relationships>"));
    let refs: Vec<_> = parts
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    assert!(WorkbookReader::new(Cursor::new(fixture(&refs))).is_err());
}

#[test]
fn dynamic_array_and_opaque_metadata_project_visible_values_or_explicitly_reject() {
    use crabxl_core::{CellMetadataReadPolicy, FormulaType};
    let content = "<row r=\"1\"><c r=\"A1\" cm=\"1\"><f t=\"array\" ref=\"A1:A2\">_xlfn.SEQUENCE(2)</f><v>7</v></c><c r=\"B1\" vm=\"4294967295\"><v>42</v></c><c r=\"C1\" cm=\"bad\" t=\"str\"><v>visible</v></c></row>";
    let mut book = open(content);
    let mut rows = book.rows("A & B").unwrap();
    let row = rows.next_row().unwrap().unwrap();
    let CellValue::Formula(formula) = &row.cells[0].value else {
        panic!("Expected array formula");
    };
    assert_eq!(formula.formula_type(), FormulaType::Array);
    assert_eq!(formula.expression(), "_xlfn.SEQUENCE(2)");
    assert_eq!(formula.cached(), Some(&CellValue::Integer(7)));
    assert_eq!(row.cells[1].value, CellValue::Integer(42));
    assert_eq!(row.cells[2].value, CellValue::text("visible"));
    assert_eq!(rows.projected_metadata_cells(), 3);
    drop(rows);
    let mut cached = book
        .rows_with_options(
            "A & B",
            ReadOptions {
                data_only: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        cached.next_row().unwrap().unwrap().cells[0].value,
        CellValue::Integer(7)
    );
    assert_eq!(cached.projected_metadata_cells(), 3);
    drop(cached);
    let mut reject = book
        .rows_with_options(
            "A & B",
            ReadOptions {
                cell_metadata_policy: CellMetadataReadPolicy::Reject,
                ..Default::default()
            },
        )
        .unwrap();
    let error = reject.next_row().unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Unsupported);
    assert_eq!(error.cell().unwrap().to_string(), "A1");
    drop(reject);
    let mut excluded = book
        .rows_with_options(
            "A & B",
            ReadOptions {
                columns: Some(ColumnIndex::new(3).unwrap()..=ColumnIndex::new(4).unwrap()),
                cell_metadata_policy: CellMetadataReadPolicy::Reject,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(excluded.next_row().unwrap().unwrap().cells.is_empty());
    assert_eq!(excluded.projected_metadata_cells(), 0);
}

#[test]
fn aggregate_scan_charges_prepared_catalogs_and_shared_formula_storage() {
    let mut book = open("<row><c><f t=\"shared\" si=\"1\" ref=\"A1\">A1+1</f><v>2</v></c></row>");
    let limits = ResourceLimits::default();
    let working = crabxl_xlsx::memory_allowance(MemoryPolicy::Budget(usize::MAX), limits)
        .unwrap()
        .working_reserve_bytes;
    let catalog = book.catalog_memory_bytes();
    assert!(
        book.read_with_policy(
            "A & B",
            AccessPattern::Scan,
            MemoryPolicy::Budget(working + catalog - 1)
        )
        .is_err()
    );
    let output = book
        .read_with_policy(
            "A & B",
            AccessPattern::Scan,
            MemoryPolicy::Budget(working + catalog + 1024),
        )
        .unwrap();
    assert_eq!(output.decision.catalog_bytes, catalog);
    assert_eq!(output.decision.retained_data_bytes, 1024);
    let ReadData::Streaming(mut rows) = output.data else {
        panic!("Expected stream");
    };
    assert!(rows.next_row().unwrap().is_some());
    assert!(rows.shared_formula_stats().accounted_bytes > 0);
    assert!(rows.managed_retained_bytes() <= catalog + 1024);
}

#[test]
fn aggregate_auto_rebalances_existing_strings_and_bounds_mixed_components() {
    use crabxl_xlsx::{SharedStringOptions, SharedStringStorage};
    let payload = "X".repeat(1000);
    let strings = (0..100)
        .map(|_| format!("<si><t>{payload}</t></si>"))
        .collect::<String>();
    let mut content = String::new();
    for index in 1..=100 {
        content.push_str(&format!("<row r=\"{index}\"><c r=\"A{index}\" t=\"s\"><v>{}</v></c><c r=\"B{index}\"><f t=\"shared\" si=\"{index}\" ref=\"B{index}\">B{index}+1</f><v>{index}</v></c></row>", index - 1));
    }
    let bytes = with_strings(&content, &strings);
    let mut book = WorkbookReader::new(Cursor::new(&bytes)).unwrap();
    // Prepare under the old independent allowance, then choose a tighter policy.
    assert!(book.rows("A & B").unwrap().next_row().unwrap().is_some());
    assert!(!book.shared_string_stats().unwrap().disk_backed);
    let working =
        crabxl_xlsx::memory_allowance(MemoryPolicy::Budget(usize::MAX), ResourceLimits::default())
            .unwrap()
            .working_reserve_bytes;
    let maximum = book.catalog_memory_bytes() + 32_000;
    let mut output = book
        .read_with_policy(
            "A & B",
            AccessPattern::Scan,
            MemoryPolicy::Budget(working + maximum),
        )
        .unwrap();
    let ReadData::Streaming(ref mut rows) = output.data else {
        panic!("Expected stream");
    };
    let mut count = 0;
    while let Some(row) = rows.next_row().unwrap() {
        assert_eq!(row.cells[0].value, CellValue::text(payload.as_str()));
        count += 1;
        assert!(rows.managed_retained_bytes() <= maximum);
    }
    assert_eq!(count, 100);
    drop(output);
    assert!(book.shared_string_stats().unwrap().disk_backed);
    assert!(book.shared_string_stats().unwrap().temp_bytes > 100_000);
    // A forced-memory table cannot silently spill to satisfy a new global cap.
    let mut strict = WorkbookReader::new(Cursor::new(&bytes)).unwrap();
    strict.set_shared_string_options(SharedStringOptions {
        storage: SharedStringStorage::Memory,
        ..Default::default()
    });
    assert!(strict.rows("A & B").unwrap().next_row().unwrap().is_some());
    assert!(
        strict
            .read_with_policy(
                "A & B",
                AccessPattern::Scan,
                MemoryPolicy::Budget(working + maximum)
            )
            .is_err()
    );
    assert!(!strict.shared_string_stats().unwrap().disk_backed);
}

#[test]
fn aggregate_repeated_access_can_retain_rows_after_lending_string_table_memory() {
    let payload = "X".repeat(1000);
    let strings = (0..100)
        .map(|_| format!("<si><t>{payload}</t></si>"))
        .collect::<String>();
    let content = (1..=100)
        .map(|index| {
            format!(
                "<row r=\"{index}\"><c r=\"A{index}\" t=\"s\"><v>{}</v></c></row>",
                index - 1
            )
        })
        .collect::<String>();
    let mut book = WorkbookReader::new(Cursor::new(with_strings(&content, &strings))).unwrap();
    let working =
        crabxl_xlsx::memory_allowance(MemoryPolicy::Budget(usize::MAX), ResourceLimits::default())
            .unwrap()
            .working_reserve_bytes;
    let maximum = book.catalog_memory_bytes() + 180_000;
    let output = book
        .read_with_policy(
            "A & B",
            AccessPattern::RepeatedAccess,
            MemoryPolicy::Budget(working + maximum),
        )
        .unwrap();
    assert_eq!(output.decision.mode, ReadMode::Materialized);
    let catalog = output.decision.catalog_bytes;
    let cache = output.decision.cache_bytes;
    let ReadData::Materialized(ref sheet) = output.data else {
        panic!("Expected owned rows");
    };
    assert_eq!(sheet.rows.len(), 100);
    assert!(sheet.memory_bytes() + catalog + cache <= maximum);
    drop(output);
    assert!(book.shared_string_stats().unwrap().disk_backed);
}

#[test]
fn aggregate_batches_lend_cache_space_and_release_library_retention_on_delivery() {
    let payload = "X".repeat(1000);
    let strings = (0..100)
        .map(|_| format!("<si><t>{payload}</t></si>"))
        .collect::<String>();
    let content = (1..=100)
        .map(|index| {
            format!(
                "<row r=\"{index}\"><c r=\"A{index}\" t=\"s\"><v>{}</v></c></row>",
                index - 1
            )
        })
        .collect::<String>();
    let mut book = WorkbookReader::new(Cursor::new(with_strings(&content, &strings))).unwrap();
    let working =
        crabxl_xlsx::memory_allowance(MemoryPolicy::Budget(usize::MAX), ResourceLimits::default())
            .unwrap()
            .working_reserve_bytes;
    let maximum = book.catalog_memory_bytes() + 100_000;
    let output = book
        .read_with_policy(
            "A & B",
            AccessPattern::Scan,
            MemoryPolicy::Budget(working + maximum),
        )
        .unwrap();
    let ReadData::Streaming(mut rows) = output.data else {
        panic!("Expected stream");
    };
    let mut count = 0;
    while let Some(batch) = rows.read_batch().unwrap() {
        assert!(batch.memory_bytes() + rows.managed_retained_bytes() <= maximum);
        count += batch.rows.len();
        for row in batch.rows {
            assert_eq!(row.cells[0].value, CellValue::text(payload.as_str()));
        }
    }
    assert_eq!(count, 100);
}

#[test]
fn aggregate_failed_auto_spill_keeps_the_existing_table_and_cleans_partial_temp_storage() {
    use crabxl_xlsx::SharedStringOptions;
    let directory = tempfile::tempdir().unwrap();
    let strings = format!("<si><t>{}</t></si>", "X".repeat(10_000));
    let mut book = WorkbookReader::new(Cursor::new(with_strings(
        "<row><c t=\"s\"><v>0</v></c></row>",
        &strings,
    )))
    .unwrap();
    book.set_shared_string_options(SharedStringOptions {
        temp_directory: Some(directory.path().into()),
        max_temp_bytes: 32,
        ..Default::default()
    });
    assert!(book.rows("A & B").unwrap().next_row().unwrap().is_some());
    let previous = book.shared_string_stats().unwrap().managed_bytes;
    let working =
        crabxl_xlsx::memory_allowance(MemoryPolicy::Budget(usize::MAX), ResourceLimits::default())
            .unwrap()
            .working_reserve_bytes;
    let maximum = book.catalog_memory_bytes() + 1000;
    assert!(
        book.read_with_policy(
            "A & B",
            AccessPattern::Scan,
            MemoryPolicy::Budget(working + maximum)
        )
        .is_err()
    );
    assert!(!book.shared_string_stats().unwrap().disk_backed);
    assert_eq!(book.shared_string_stats().unwrap().managed_bytes, previous);
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    assert!(book.rows("A & B").unwrap().next_row().unwrap().is_some());
}

#[test]
fn aggregate_policy_options_retain_projected_shared_metadata_and_cache_modes() {
    use crabxl_core::FormulaType;
    for access in [AccessPattern::Scan, AccessPattern::RepeatedAccess] {
        for data_only in [false, true] {
            let mut book = open(
                "<row r=\"1\"><c r=\"A1\"><f t=\"shared\" si=\"9\" ref=\"A1:A2\">A1+1</f><v>2</v></c></row><row r=\"2\"><c r=\"A2\"><f t=\"shared\" si=\"9\"/><v>3</v></c></row>",
            );
            let mut output = book
                .read_with_policy_options(
                    "A & B",
                    ReadOptions {
                        rows: Some(
                            crabxl_core::RowIndex::new(1).unwrap()
                                ..=crabxl_core::RowIndex::new(1).unwrap(),
                        ),
                        data_only,
                        formula_metadata: true,
                        ..Default::default()
                    },
                    access,
                    MemoryPolicy::Budget(8 * 1024 * 1024),
                )
                .unwrap();
            let check = |row: &crabxl_core::Row| {
                assert_eq!(row.index.get(), 1);
                assert_eq!(row.cells.len(), 1);
                if data_only {
                    assert_eq!(row.cells[0].value, CellValue::Integer(3));
                } else {
                    let CellValue::Formula(formula) = &row.cells[0].value else {
                        panic!("Expected shared formula");
                    };
                    assert_eq!(formula.expression(), "A2+1");
                    assert_eq!(
                        formula.formula_type(),
                        FormulaType::Shared {
                            index: 9,
                            master: false
                        }
                    );
                }
            };
            match &mut output.data {
                ReadData::Streaming(rows) => {
                    check(&rows.next_row().unwrap().unwrap());
                    assert!(rows.next_row().unwrap().is_none());
                }
                ReadData::Materialized(sheet) => {
                    assert_eq!(sheet.rows.len(), 1);
                    check(&sheet.rows[0]);
                }
            }
        }
    }
}

#[test]
fn aggregate_policy_options_upgrade_rich_strings_and_preserve_raw_date_extension() {
    use crabxl_core::{DateReadPolicy, RowIndex};
    let bytes = with_strings(
        "<row><c t=\"s\"><v>0</v></c></row>",
        "<si><r><rPr><b/></rPr><t>rich</t></r></si>",
    );
    let mut book = WorkbookReader::new(Cursor::new(bytes)).unwrap();
    assert_eq!(
        book.rows("A & B")
            .unwrap()
            .next_row()
            .unwrap()
            .unwrap()
            .cells[0]
            .value,
        CellValue::text("rich")
    );
    let mut output = book
        .read_with_policy_options(
            "A & B",
            ReadOptions {
                rich_text: true,
                ..Default::default()
            },
            AccessPattern::RepeatedAccess,
            MemoryPolicy::Budget(8 * 1024 * 1024),
        )
        .unwrap();
    let ReadData::Materialized(ref mut sheet) = output.data else {
        panic!("Expected rich owned rows");
    };
    let CellValue::RichText(rich) = &sheet.rows[0].cells[0].value else {
        panic!("Expected retained runs");
    };
    assert_eq!(rich.runs[0].font.as_ref().unwrap().bold, Some(true));
    drop(output);
    assert!(book.shared_string_stats().unwrap().rich_text_preserved);
    let styles = basic_styles("<xf numFmtId=\"14\"/>");
    let mut dates = WorkbookReader::new(Cursor::new(with_styles(
        "<row r=\"1\"><c r=\"A1\"><v>10000000</v></c></row>",
        &styles,
        false,
    )))
    .unwrap();
    let mut output = dates
        .read_with_policy_options(
            "A & B",
            ReadOptions {
                rows: Some(RowIndex::new(0).unwrap()..=RowIndex::new(0).unwrap()),
                date_policy: DateReadPolicy::RetainSerial,
                ..Default::default()
            },
            AccessPattern::RepeatedAccess,
            MemoryPolicy::Budget(8 * 1024 * 1024),
        )
        .unwrap();
    let ReadData::Materialized(ref mut sheet) = output.data else {
        panic!("Expected date owned rows");
    };
    let CellValue::DateTime(date) = &sheet.rows[0].cells[0].value else {
        panic!("Expected exact source serial");
    };
    assert_eq!(date.serial(), 10_000_000.0);
}

#[test]
fn projected_policy_does_not_retain_shared_groups_after_the_requested_last_row() {
    use crabxl_core::{FormulaReadPolicy, RowIndex};
    let content = "<row r=\"1\"><c><v>7</v></c></row>".to_owned() + &(2..=1000).map(|index| format!("<row r=\"{index}\"><c r=\"A{index}\"><f t=\"shared\" si=\"{index}\" ref=\"A{index}\">A{index}+1</f><v>{index}</v></c></row>")).collect::<String>();
    let mut book = open(&content);
    let working =
        crabxl_xlsx::memory_allowance(MemoryPolicy::Budget(usize::MAX), ResourceLimits::default())
            .unwrap()
            .working_reserve_bytes;
    let maximum = book.catalog_memory_bytes() + 4096;
    let mut output = book
        .read_with_policy_options(
            "A & B",
            ReadOptions {
                rows: Some(RowIndex::new(0).unwrap()..=RowIndex::new(0).unwrap()),
                ..Default::default()
            },
            AccessPattern::Scan,
            MemoryPolicy::Budget(working + maximum),
        )
        .unwrap();
    let ReadData::Streaming(ref mut rows) = output.data else {
        panic!("Expected stream");
    };
    assert_eq!(
        rows.next_row().unwrap().unwrap().cells[0].value,
        CellValue::Integer(7)
    );
    assert!(rows.next_row().unwrap().is_none());
    assert_eq!(rows.shared_formula_stats().templates, 0);
    drop(output);
    let mut strict = book
        .rows_with_options(
            "A & B",
            ReadOptions {
                rows: Some(RowIndex::new(0).unwrap()..=RowIndex::new(0).unwrap()),
                formula_policy: FormulaReadPolicy::ValidateGroups,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(strict.next_row().unwrap().is_some());
    assert!(strict.next_row().unwrap().is_none());
    assert_eq!(strict.shared_formula_stats().templates, 999);
}

#[test]
fn cache_only_projection_ignores_formula_semantics_without_ignoring_xml_or_values() {
    let options = ReadOptions {
        data_only: true,
        ..ReadOptions::default()
    };
    for formula in [
        "<f t=\"future\" ref=\"invalid\">1+1</f>".to_owned(),
        format!("<f>{}</f>", "1&amp;".repeat(40000)),
    ] {
        let mut book = open(&format!(
            "<row r=\"1\"><c r=\"A1\">{formula}<v>2</v></c><c r=\"B1\" t=\"str\"><f t=\"future\"/><v></v></c></row>"
        ));
        let rows = book
            .rows_with_options("A & B", options.clone())
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(rows[0].cells[0].value, CellValue::Integer(2));
        assert_eq!(rows[0].cells[1].value, CellValue::Empty);
    }
    for content in [
        "<f>&unknown;</f><v>2</v>",
        "<f><nested/></f><v>2</v>",
        "<f>1</f><f>2</f><v>2</v>",
        "<f>1</f><v>invalid</v>",
    ] {
        let mut book = open(&format!("<row r=\"1\"><c r=\"A1\">{content}</c></row>"));
        assert!(
            book.rows_with_options("A & B", options.clone())
                .unwrap()
                .next()
                .unwrap()
                .is_err()
        );
    }
    let mut book = open("<row r=\"1\"><c r=\"A1\"><f t=\"future\">1</f><v>2</v></c></row>");
    let strict = ReadOptions {
        formula_policy: crabxl_core::FormulaReadPolicy::ValidateGroups,
        ..options
    };
    assert!(
        book.rows_with_options("A & B", strict)
            .unwrap()
            .next()
            .unwrap()
            .is_err()
    );
}

#[test]
fn consuming_reader_transfers_original_style_records_for_canonical_registration() {
    use crabxl_core::{CellStyle, StyleLimits, StyleRegistry};
    let styles = basic_styles("<xf/><xf numFmtId=\"14\"/>");
    let mut book = WorkbookReader::new(Cursor::new(with_styles(
        "<row><c s=\"1\"><v>43831</v></c></row>",
        &styles,
        false,
    )))
    .unwrap();
    let source = book.style_catalog().unwrap().unwrap();
    let pointer = source.fonts.as_ptr();
    let source_formats = source.cell_formats.clone();
    let catalog = book.into_style_catalog().unwrap().unwrap();
    assert_eq!(catalog.fonts.as_ptr(), pointer);
    let mut registry = StyleRegistry::from_catalog(catalog, StyleLimits::default()).unwrap();
    assert_eq!(registry.catalog().cell_formats, source_formats);
    registry
        .register(CellStyle {
            number_format: "0.000".into(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        &registry.catalog().cell_formats[..source_formats.len()],
        source_formats.as_slice()
    );
}

#[test]
fn color_identities_use_public_priority_and_validate_all_xml_attributes() {
    use crabxl_core::{ColorKind, StyleId};
    for (attributes, expected) in [
        ("rgb=\"invalid\" theme=\"1\"", ColorKind::Theme(1)),
        (
            "rgb=\"invalid\" theme=\"invalid\" auto=\"invalid\" indexed=\"3\"",
            ColorKind::Indexed(3),
        ),
        ("rgb=\"invalid\" auto=\"0\"", ColorKind::Auto(false)),
    ] {
        let styles =
            basic_styles("<xf/>").replace("</font>", &format!("<color {attributes}/></font>"));
        let mut book =
            WorkbookReader::new(Cursor::new(with_styles("<row/>", &styles, false))).unwrap();
        let catalog = book.style_catalog().unwrap().unwrap();
        assert_eq!(
            catalog
                .cell_style(StyleId::new(0))
                .unwrap()
                .font
                .color
                .unwrap()
                .kind,
            expected
        );
    }
    for attributes in ["rgb=\"&unknown;\" theme=\"1\"", "theme=\"1\" unknown=\"1\""] {
        let styles =
            basic_styles("<xf/>").replace("</font>", &format!("<color {attributes}/></font>"));
        let mut book =
            WorkbookReader::new(Cursor::new(with_styles("<row/>", &styles, false))).unwrap();
        assert!(book.style_catalog().is_err());
    }
}

#[test]
fn finite_style_domains_and_empty_number_format_decode_from_source() {
    use crabxl_core::{Fill, StyleId};
    let styles = basic_styles("<xf numFmtId=\"164\"/>")
        .replace("[h]:mm:ss.000", "")
        .replace("<sz val=\"11\"/>", "<sz val=\"-1\"/>")
        .replace(
            "<patternFill patternType=\"none\"/>",
            "<gradientFill type=\"path\" left=\"-1\" right=\"2\"/>",
        );
    let mut reader =
        WorkbookReader::new(Cursor::new(with_styles("<row/>", &styles, false))).unwrap();
    let style = reader
        .style_catalog()
        .unwrap()
        .unwrap()
        .cell_style(StyleId::new(0))
        .unwrap();
    assert_eq!(style.number_format, Some(""));
    assert_eq!(style.font.size, Some(-1.0));
    assert!(
        matches!(style.fill, Fill::Gradient(value) if value.edges[0] == Some(-1.0) && value.edges[1] == Some(2.0))
    );
}
