//! Source-generated lazy canonical bank ownership and aggregate failure fixtures.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use crabxl_core::{
    Cell, CellAddress, CellStyle, CellValue, DateKind, EditLimits, ErrorKind, ExcelDateTime,
    MemoryPolicy, Row, RowIndex, StyleId, WorkbookLimits,
};
use crabxl_xlsx::{
    LoadOptions, LoadedWorkbook, SharedStringOptions, SharedStringStorage, WorkbookWriter,
    WriteOptions,
};
use std::{
    collections::BTreeMap,
    io::{Cursor, Read, Write},
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

fn source(count: u32, sst: bool) -> Vec<u8> {
    let mut writer = WorkbookWriter::new(WriteOptions::default()).unwrap();
    let style = writer
        .register_style(CellStyle {
            number_format: "0.00".into(),
            ..Default::default()
        })
        .unwrap();
    for name in ["First", "Second"] {
        writer.start_sheet(name).unwrap();
        for index in 0..count {
            writer
                .write_row(&Row {
                    index: RowIndex::new(index).unwrap(),
                    cells: vec![Cell {
                        address: CellAddress::new(index, 0).unwrap(),
                        value: if sst {
                            CellValue::text(format!("{index:08}{}", "x".repeat(120)))
                        } else {
                            CellValue::Integer(index.into())
                        },
                        style,
                    }],
                })
                .unwrap();
        }
        if !sst {
            writer
                .write_row(&Row {
                    index: RowIndex::new(count).unwrap(),
                    cells: vec![Cell {
                        address: CellAddress::new(count, 0).unwrap(),
                        value: CellValue::DateTime(Box::new(
                            ExcelDateTime::from_serial(
                                2.5,
                                crabxl_core::DateEpoch::Windows1900,
                                DateKind::DateTime,
                            )
                            .unwrap(),
                        )),
                        style: StyleId::new(0),
                    }],
                })
                .unwrap();
        }
    }
    let bytes = writer.finish(Cursor::new(Vec::new())).unwrap().into_inner();
    if !sst {
        return bytes;
    }
    let mut original = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut parts = BTreeMap::new();
    for index in 0..original.len() {
        let mut file = original.by_index(index).unwrap();
        let name = file.name().to_owned();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        parts.insert(name, bytes);
    }
    let mut strings =
        String::from("<sst xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">");
    for index in 0..count {
        let text = format!("{index:08}{}", "x".repeat(120));
        strings.push_str(&format!("<si><t>{text}</t></si>"));
    }
    let mut worksheet = String::from(
        "<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData>",
    );
    for index in 0..count {
        worksheet.push_str(&format!(
            "<row r=\"{}\"><c r=\"A{}\" s=\"{}\" t=\"s\"><v>{index}</v></c></row>",
            index + 1,
            index + 1,
            style.get()
        ));
    }
    worksheet.push_str("</sheetData></worksheet>");
    for sheet in ["xl/worksheets/sheet1.xml", "xl/worksheets/sheet2.xml"] {
        parts.insert(sheet.into(), worksheet.as_bytes().to_vec());
    }
    strings.push_str("</sst>");
    parts.insert("xl/sharedStrings.xml".into(), strings.into_bytes());
    let name = "xl/_rels/workbook.xml.rels";
    let xml = String::from_utf8(parts.remove(name).unwrap()).unwrap();
    parts.insert(name.into(), xml.replace("</Relationships>", "<Relationship Id=\"rIdStrings\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings\" Target=\"sharedStrings.xml\"/></Relationships>").into_bytes());
    let mut output = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in parts {
        output
            .start_file(
                name,
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
            )
            .unwrap();
        output.write_all(&bytes).unwrap();
    }
    output.finish().unwrap().into_inner()
}

#[test]
fn lazy_models_share_source_styles_and_stable_ids_and_keep_source_repeatable() {
    let bytes = source(3, false);
    let mut workbook =
        LoadedWorkbook::with_options(Cursor::new(bytes.clone()), LoadOptions::default()).unwrap();
    let first = workbook.sheet_id("First").unwrap();
    let second = workbook.sheet_id("Second").unwrap();
    assert!(!workbook.is_materialized(first));
    assert!(!workbook.is_materialized(second));
    assert_eq!(workbook.model().cell_count(), 0);
    let catalog_pointer = workbook.model().style_catalog().unwrap().fonts.as_ptr();
    let sheet = workbook.sheet(first).unwrap();
    assert_eq!(sheet.len(), 4);
    assert!(matches!(
        sheet.get(CellAddress::new(3, 0).unwrap()).unwrap().value,
        CellValue::DateTime(_)
    ));
    let text_pointer = sheet.get(CellAddress::new(0, 0).unwrap()).unwrap() as *const Cell;
    assert!(workbook.is_materialized(first));
    assert!(!workbook.is_materialized(second));
    assert_eq!(
        workbook
            .sheet(first)
            .unwrap()
            .get(CellAddress::new(0, 0).unwrap())
            .unwrap() as *const Cell,
        text_pointer
    );
    workbook.sheet(second).unwrap();
    assert_eq!(workbook.model().cell_count(), 8);
    assert_eq!(
        workbook.model().style_catalog().unwrap().fonts.as_ptr(),
        catalog_pointer
    );
    assert!(workbook.managed_retained_bytes() <= workbook.memory_allowance().retained_data_bytes);
    assert_eq!(workbook.into_source().into_inner(), bytes);
}

#[test]
fn aggregate_loading_rejects_without_committing_and_sst_temp_resources_are_owned() {
    let bytes = source(8, false);
    let mut workbook = LoadedWorkbook::with_options(
        Cursor::new(bytes),
        LoadOptions {
            workbook: WorkbookLimits {
                max_cells: 10,
                sheet: EditLimits {
                    max_cells: 10,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
    let first = workbook.sheet_id("First").unwrap();
    let second = workbook.sheet_id("Second").unwrap();
    workbook.sheet(first).unwrap();
    for _ in 0..2 {
        assert_eq!(
            workbook.sheet(second).err().unwrap().kind(),
            ErrorKind::MemoryBudgetExceeded
        );
        assert!(!workbook.is_materialized(second));
        assert_eq!(workbook.model().cell_count(), 9);
        assert_eq!(workbook.sheet(first).unwrap().len(), 9);
        assert!(
            workbook.managed_retained_bytes() <= workbook.memory_allowance().retained_data_bytes
        );
    }
    let count = 3000;
    let bytes = source(count, true);
    for storage in [
        SharedStringStorage::Memory,
        SharedStringStorage::Auto,
        SharedStringStorage::Disk,
    ] {
        let directory = tempfile::tempdir().unwrap();
        let mut workbook = LoadedWorkbook::with_options(
            Cursor::new(bytes.clone()),
            LoadOptions {
                memory_policy: MemoryPolicy::Budget(4 * 1024 * 1024),
                shared_strings: SharedStringOptions {
                    storage,
                    cache_bytes: 1024,
                    temp_directory: Some(directory.path().to_owned()),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .unwrap();
        let first = workbook.sheet_id("First").unwrap();
        let second = workbook.sheet_id("Second").unwrap();
        for id in [first, second] {
            if id == second && storage == SharedStringStorage::Memory {
                assert_eq!(
                    workbook.sheet(id).err().unwrap().kind(),
                    ErrorKind::MemoryBudgetExceeded
                );
                assert!(!workbook.is_materialized(id));
                assert_eq!(workbook.sheet(first).unwrap().len(), count as usize);
                assert!(
                    workbook.managed_retained_bytes()
                        <= workbook.memory_allowance().retained_data_bytes
                );
                continue;
            }
            let sheet = workbook.sheet(id).unwrap();
            assert_eq!(sheet.len(), count as usize);
            assert!(
                matches!(&sheet.get(CellAddress::new(count - 1, 0).unwrap()).unwrap().value, CellValue::Text(text) if text.as_str().starts_with("00002999"))
            );
            assert!(
                workbook.managed_retained_bytes()
                    <= workbook.memory_allowance().retained_data_bytes
            );
        }
        if storage != SharedStringStorage::Memory {
            let stats = workbook.shared_string_stats().unwrap();
            assert!(stats.disk_backed);
            assert!(stats.disk_reads > 0);
            assert!(stats.temp_bytes > 0);
            #[cfg(target_os = "linux")]
            assert!(
                std::fs::read_dir("/proc/self/fd")
                    .unwrap()
                    .filter_map(|entry| { std::fs::read_link(entry.ok()?.path()).ok() })
                    .any(|path| path.starts_with(directory.path()))
            );
        }
        drop(workbook);
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    }
}
