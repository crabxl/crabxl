//! Compression options preserve values and original-package payloads.
#![allow(clippy::unwrap_used)]
use crabxl_core::{Cell, CellAddress, CellValue, ErrorKind, Row, RowIndex, StyleId};
use crabxl_xlsx::{SaveOptions, WorkbookEditor, WorkbookReader, WorkbookWriter, WriteOptions};
use std::io::{Cursor, Read};
use zip::ZipArchive;

fn create(level: Option<u8>) -> Vec<u8> {
    let mut writer = WorkbookWriter::new(WriteOptions {
        compression_level: level,
        ..Default::default()
    })
    .unwrap();
    writer.start_sheet("Sheet").unwrap();
    writer
        .write_row(&Row {
            index: RowIndex::new(0).unwrap(),
            cells: vec![Cell {
                address: CellAddress::new(0, 0).unwrap(),
                value: CellValue::text("Unicode 中文 & < > repeated repeated"),
                style: StyleId::new(0),
            }],
        })
        .unwrap();
    writer.close_sheet().unwrap();
    writer.finish(Cursor::new(Vec::new())).unwrap().into_inner()
}

fn xml(bytes: &[u8]) -> String {
    let mut zip = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut text = String::new();
    zip.by_name("xl/worksheets/sheet1.xml")
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    text
}

#[test]
fn compression_levels_preserve_creation_edits_and_unchanged_entries() {
    let baseline = create(None);
    let mut editor = WorkbookEditor::new(Cursor::new(&baseline)).unwrap();
    editor
        .set_value(
            "Sheet",
            CellAddress::new(0, 0).unwrap(),
            CellValue::Integer(7),
        )
        .unwrap();
    let mut edited_xml = None;
    for level in [None, Some(0), Some(1), Some(3), Some(6), Some(9)] {
        let created = create(level);
        assert_eq!(xml(&created), xml(&baseline));
        let options = SaveOptions {
            compression_level: level,
            verify_unchanged: true,
        };
        let (saved, _) = editor.save(Cursor::new(Vec::new()), options).unwrap();
        let text = xml(saved.get_ref());
        if let Some(expected) = &edited_xml {
            assert_eq!(&text, expected);
        }
        edited_xml = Some(text);
        let mut reader = WorkbookReader::new(Cursor::new(saved.into_inner())).unwrap();
        let sheet = reader.read_sheet("Sheet").unwrap();
        assert_eq!(sheet.rows[0].cells[0].value, CellValue::Integer(7));
        let mut source_zip = ZipArchive::new(Cursor::new(&baseline)).unwrap();
        let (unchanged, _) = WorkbookEditor::new(Cursor::new(&baseline))
            .unwrap()
            .save(Cursor::new(Vec::new()), options)
            .unwrap();
        let mut saved_zip = ZipArchive::new(unchanged).unwrap();
        for index in 0..source_zip.len() {
            let mut original = source_zip.by_index_raw(index).unwrap();
            let mut copy = saved_zip.by_index_raw(index).unwrap();
            assert_eq!(original.name(), copy.name());
            let mut a = Vec::new();
            let mut b = Vec::new();
            original.read_to_end(&mut a).unwrap();
            copy.read_to_end(&mut b).unwrap();
            assert_eq!(a, b);
        }
    }
}

#[test]
fn invalid_levels_reject_before_output_and_allow_retry() {
    assert!(
        WorkbookWriter::new(WriteOptions {
            compression_level: Some(10),
            ..Default::default()
        })
        .is_err()
    );
    let mut writer = WorkbookWriter::new(Default::default()).unwrap();
    assert_eq!(
        writer.set_compression_level(Some(255)).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    writer.set_compression_level(Some(1)).unwrap();
    let source = create(None);
    let mut editor = WorkbookEditor::new(Cursor::new(source)).unwrap();
    let mut output = Cursor::new(vec![1, 2, 3]);
    assert!(
        editor
            .save(
                &mut output,
                SaveOptions {
                    compression_level: Some(10),
                    ..Default::default()
                }
            )
            .is_err()
    );
    assert_eq!(output.into_inner(), vec![1, 2, 3]);
    editor
        .save(Cursor::new(Vec::new()), Default::default())
        .unwrap();
}
