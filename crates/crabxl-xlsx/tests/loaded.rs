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

fn parts(bytes: Vec<u8>) -> BTreeMap<String, Vec<u8>> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..archive.len())
        .map(|index| {
            let mut file = archive.by_index(index).unwrap();
            let mut data = Vec::new();
            file.read_to_end(&mut data).unwrap();
            (file.name().to_owned(), data)
        })
        .collect()
}
fn package(parts: BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in parts {
        zip.start_file(name, SimpleFileOptions::default()).unwrap();
        zip.write_all(&bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

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
    for storage in [
        SharedStringStorage::Memory,
        SharedStringStorage::Disk,
        SharedStringStorage::Auto,
    ] {
        let directory = tempfile::tempdir().unwrap();
        let mut workbook = LoadedWorkbook::with_options(
            Cursor::new(source(3, true)),
            LoadOptions {
                shared_strings: SharedStringOptions {
                    storage,
                    cache_bytes: 0,
                    temp_directory: Some(directory.path().into()),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .unwrap();
        let first = workbook.sheet_id("First").unwrap();
        let second = workbook.sheet_id("Second").unwrap();
        let value = workbook
            .sheet(first)
            .unwrap()
            .get(CellAddress::new(0, 0).unwrap())
            .unwrap()
            .value
            .clone();
        let CellValue::Text(retained) = &value else {
            unreachable!()
        };
        let other = &workbook
            .sheet(second)
            .unwrap()
            .get(CellAddress::new(0, 0).unwrap())
            .unwrap()
            .value;
        assert_eq!(&value, other);
        if storage == SharedStringStorage::Memory {
            let CellValue::Text(other) = other else {
                unreachable!()
            };
            assert_eq!(retained.as_str().as_ptr(), other.as_str().as_ptr());
        }
        drop(workbook);
        assert_eq!(retained.as_str(), format!("00000000{}", "x".repeat(120)));
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    }
    // An unimplemented chartsheet stays opaque without blocking an unrelated
    // worksheet's typed read/edit or removing its original package part.
    let mut input = ZipArchive::new(Cursor::new(source(3, false))).unwrap();
    let mut output = ZipWriter::new(Cursor::new(Vec::new()));
    let chart = b"<chartsheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetViews><sheetView workbookViewId=\"0\"/></sheetViews></chartsheet>";
    for index in 0..input.len() {
        let mut file = input.by_index(index).unwrap();
        let name = file.name().to_owned();
        let mut data = Vec::new();
        file.read_to_end(&mut data).unwrap();
        if name == "xl/_rels/workbook.xml.rels" {
            data = String::from_utf8(data)
                .unwrap()
                .replace(
                    "/worksheet\" Target=\"worksheets/sheet2.xml\"",
                    "/chartsheet\" Target=\"worksheets/sheet2.xml\"",
                )
                .into_bytes();
        } else if name == "xl/worksheets/sheet2.xml" {
            data = chart.to_vec();
        } else if name == "[Content_Types].xml" {
            data = String::from_utf8(data).unwrap().replace("PartName=\"/xl/worksheets/sheet2.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"", "PartName=\"/xl/worksheets/sheet2.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.chartsheet+xml\"").into_bytes();
        }
        output
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        output.write_all(&data).unwrap();
    }
    let mut workbook = LoadedWorkbook::with_options(
        Cursor::new(output.finish().unwrap().into_inner()),
        LoadOptions::default(),
    )
    .unwrap();
    let first = workbook.sheet_id("First").unwrap();
    let chart_id = workbook.sheet_id("Second").unwrap();
    assert_eq!(
        workbook.sheet_kind(chart_id),
        Some(crabxl_xlsx::SheetKind::ChartSheet)
    );
    assert_eq!(
        workbook.sheet(chart_id).err().unwrap().kind(),
        ErrorKind::Unsupported
    );
    workbook.set_active_sheet(chart_id).unwrap();
    assert_eq!(workbook.model().active_sheet(), Some(chart_id));
    workbook.sheet(first).unwrap();
    workbook
        .set_value(
            first,
            CellAddress::new(0, 0).unwrap(),
            CellValue::Integer(44),
        )
        .unwrap();
    let (bytes, _) = workbook
        .save(Cursor::new(Vec::new()), crabxl_xlsx::SaveOptions::default())
        .unwrap();
    let mut saved = ZipArchive::new(bytes).unwrap();
    let mut data = Vec::new();
    saved
        .by_name("xl/worksheets/sheet2.xml")
        .unwrap()
        .read_to_end(&mut data)
        .unwrap();
    assert_eq!(data, chart);
}

#[test]
fn active_selection_is_lazy_repeatable_and_rejects_affected_metadata_before_mutation() {
    let mut original = parts(source(3, false));
    let xml = String::from_utf8(original.remove("xl/workbook.xml").unwrap()).unwrap();
    original.insert(
        "xl/workbook.xml".into(),
        xml.replace(
            "<sheets>",
            "<bookViews><workbookView activeTab=\"0\"/></bookViews><sheets>",
        )
        .into_bytes(),
    );
    let sheet = String::from_utf8(original.remove("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    assert!(sheet.contains("<v>0</v>"));
    original.insert(
        "xl/worksheets/sheet1.xml".into(),
        sheet
            .replacen("<v>0</v>", "<f>A2+1</f><v>37</v>", 1)
            .into_bytes(),
    );
    original.insert("xl/calcChain.xml".into(), b"<calcChain xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><c r=\"A1\" i=\"1\"/></calcChain>".to_vec());
    let rels = String::from_utf8(original.remove("xl/_rels/workbook.xml.rels").unwrap()).unwrap();
    original.insert("xl/_rels/workbook.xml.rels".into(), rels.replace("</Relationships>", "<Relationship Id=\"calcActive\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/calcChain\" Target=\"calcChain.xml\"/></Relationships>").into_bytes());
    let types = String::from_utf8(original.remove("[Content_Types].xml").unwrap()).unwrap();
    original.insert("[Content_Types].xml".into(), types.replace("</Types>", "<Override PartName=\"/xl/calcChain.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.calcChain+xml\"/></Types>").into_bytes());
    for mode in [
        "original",
        "missing",
        "empty",
        "multiple",
        "extension",
        "strict",
        "prefixed",
        "custom-part",
    ] {
        let mut input = original.clone();
        let book = String::from_utf8(input["xl/workbook.xml"].clone()).unwrap();
        let views = "<bookViews><workbookView activeTab=\"0\"/></bookViews>";
        assert!(book.contains(views));
        let book = match mode {
            "missing" => book.replace(views, ""),
            "empty" => book.replace(views, "<bookViews/>"),
            "multiple" => book.replace(views, "<bookViews><workbookView activeTab=\"0\" showHorizontalScroll=\"0\"/><workbookView activeTab=\"0\" windowWidth=\"123\"/></bookViews>"),
            "extension" => book.replace("</workbook>", "<extLst><sheet name=\"OpaqueSheet\"/></extLst></workbook>"),
            "prefixed" => {
                let mut xml = book.replace("xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"",
                    "xmlns:w=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"");
                for tag in ["workbook", "bookViews", "workbookView", "sheets", "sheet"] {
                    xml = xml.replace(&format!("<{tag}"), &format!("<w:{tag}"))
                        .replace(&format!("</{tag}"), &format!("</w:{tag}"));
                }
                xml
            },
            _ => book,
        };
        input.insert("xl/workbook.xml".into(), book.into_bytes());
        if mode == "strict" {
            for bytes in input.values_mut() {
                let xml = String::from_utf8(bytes.clone()).unwrap();
                *bytes = xml
                    .replace(
                        "http://schemas.openxmlformats.org/spreadsheetml/2006/main",
                        "http://purl.oclc.org/ooxml/spreadsheetml/main",
                    )
                    .replace(
                        "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
                        "http://purl.oclc.org/ooxml/officeDocument/relationships",
                    )
                    .into_bytes();
            }
        }
        let workbook_part = if mode == "custom-part" {
            for name in ["_rels/.rels", "[Content_Types].xml"] {
                let xml = String::from_utf8(input.remove(name).unwrap()).unwrap();
                input.insert(
                    name.into(),
                    xml.replace("xl/workbook.xml", "xl/custom-book.xml")
                        .into_bytes(),
                );
            }
            let xml = input.remove("xl/workbook.xml").unwrap();
            input.insert("xl/custom-book.xml".into(), xml);
            let rels = input.remove("xl/_rels/workbook.xml.rels").unwrap();
            input.insert("xl/_rels/custom-book.xml.rels".into(), rels);
            "xl/custom-book.xml"
        } else {
            "xl/workbook.xml"
        };
        let mut workbook = LoadedWorkbook::with_options(
            Cursor::new(package(input.clone())),
            LoadOptions::default(),
        )
        .unwrap();
        let first = workbook.sheet_id("First").unwrap();
        let second = workbook.sheet_id("Second").unwrap();
        workbook.set_active_sheet(second).unwrap();
        assert_eq!(workbook.model().active_sheet(), Some(second));
        assert!(!workbook.is_materialized(first));
        assert!(!workbook.is_materialized(second));
        for _ in 0..2 {
            let (output, stats) = workbook
                .save(Cursor::new(Vec::new()), Default::default())
                .unwrap();
            assert_eq!(stats.rewritten_parts, 1);
            let saved = parts(output.into_inner());
            for (name, bytes) in &input {
                if name != workbook_part {
                    assert_eq!(&saved[name], bytes, "{mode}: {name}");
                }
            }
            assert!(
                !String::from_utf8(saved[workbook_part].clone())
                    .unwrap()
                    .contains("forceFullCalc")
            );
            if mode == "multiple" {
                let xml = String::from_utf8(saved[workbook_part].clone()).unwrap();
                assert!(xml.contains("showHorizontalScroll=\"0\" activeTab=\"1\""));
                assert!(xml.contains("activeTab=\"0\" windowWidth=\"123\""));
            }
            let reader = crabxl_xlsx::WorkbookReader::with_limits(
                Cursor::new(package(saved)),
                Default::default(),
            )
            .unwrap();
            assert_eq!(reader.active_index(), Some(1));
        }
        workbook.set_active_sheet(first).unwrap();
        let (output, _) = workbook
            .save(Cursor::new(Vec::new()), Default::default())
            .unwrap();
        assert_eq!(
            crabxl_xlsx::WorkbookReader::with_limits(output, Default::default())
                .unwrap()
                .active_index(),
            Some(0)
        );
        use crabxl_core::SheetVisibility::{Hidden, VeryHidden, Visible};
        workbook.set_sheet_visibility(first, Hidden).unwrap();
        assert!(!workbook.is_materialized(first));
        assert_eq!(workbook.sheet(first).unwrap().visibility(), Hidden);
        assert!(workbook.set_active_sheet(first).is_err());
        for _ in 0..2 {
            let (output, stats) = workbook
                .save(Cursor::new(Vec::new()), Default::default())
                .unwrap();
            assert_eq!(stats.rewritten_parts, 1);
            assert_eq!(workbook.model().active_sheet(), Some(second));
            let saved = parts(output.into_inner());
            if mode == "extension" {
                let xml = String::from_utf8(saved[workbook_part].clone()).unwrap();
                assert!(xml.contains("<sheet name=\"OpaqueSheet\">"));
                assert!(!xml.contains("name=\"OpaqueSheet\" state="));
            }
            for (name, bytes) in &input {
                if name != workbook_part {
                    assert_eq!(&saved[name], bytes);
                }
            }
            let reader = crabxl_xlsx::WorkbookReader::new(Cursor::new(package(saved))).unwrap();
            assert_eq!(reader.sheets()[0].visibility(), Hidden);
            assert_eq!(reader.active_index(), Some(1));
        }
        workbook.set_sheet_visibility(second, VeryHidden).unwrap();
        let mut target = Cursor::new(b"unchanged output".to_vec());
        assert!(workbook.save(&mut target, Default::default()).is_err());
        assert_eq!(target.into_inner(), b"unchanged output");
        assert_eq!(workbook.model().active_sheet(), Some(second));
        workbook
            .set_sheet_visibility_and_active_view(second, VeryHidden, 1)
            .unwrap();
        let mut target = Cursor::new(b"unchanged deferred output".to_vec());
        assert_eq!(
            workbook
                .save(&mut target, Default::default())
                .unwrap_err()
                .kind(),
            ErrorKind::NoVisibleSheet
        );
        assert_eq!(target.into_inner(), b"unchanged deferred output");
        workbook.set_sheet_visibility(first, Visible).unwrap();
        workbook.set_active_sheet(first).unwrap();
        let (output, _) = workbook
            .save(Cursor::new(Vec::new()), Default::default())
            .unwrap();
        let reader = crabxl_xlsx::WorkbookReader::new(output).unwrap();
        assert_eq!(reader.sheets()[0].visibility(), Visible);
        assert_eq!(reader.sheets()[1].visibility(), VeryHidden);
        assert_eq!(reader.active_index(), Some(0));
        let cells_before = workbook.model().cell_count();
        let renamed = "Second<&\" \u{65b0}";
        workbook.rename_sheet(second, renamed).unwrap();
        workbook.rename_sheet(first, "RenamedFirst").unwrap();
        assert_eq!(workbook.sheet_id(renamed), Some(second));
        assert_eq!(workbook.sheet_id("Second"), None);
        assert_eq!(workbook.model().cell_count(), cells_before);
        assert!(!workbook.is_materialized(second));
        let retained = workbook.managed_retained_bytes();
        let patched = workbook.patch_bytes();
        for rejected in ["renamedfirst", "", "Invalid/Name", "Invalid\u{0}"] {
            assert!(workbook.rename_sheet(second, rejected).is_err());
            assert_eq!(workbook.sheet_id(renamed), Some(second));
            assert_eq!(workbook.managed_retained_bytes(), retained);
            assert_eq!(workbook.patch_bytes(), patched);
        }
        assert_eq!(workbook.sheet(second).unwrap().name(), renamed);
        assert_eq!(
            workbook
                .sheet(second)
                .unwrap()
                .get(CellAddress::new(0, 0).unwrap())
                .unwrap()
                .value,
            CellValue::Integer(0)
        );
        for _ in 0..2 {
            let (output, stats) = workbook
                .save(Cursor::new(Vec::new()), Default::default())
                .unwrap();
            assert_eq!(stats.rewritten_parts, 1);
            let saved = parts(output.into_inner());
            for (name, bytes) in &input {
                if name != workbook_part {
                    assert_eq!(&saved[name], bytes, "renamed {mode}: {name}");
                }
            }
            let reader = crabxl_xlsx::WorkbookReader::new(Cursor::new(package(saved))).unwrap();
            assert_eq!(reader.sheets()[0].name(), "RenamedFirst");
            assert_eq!(reader.sheets()[1].name(), renamed);
            assert_eq!(reader.sheets()[1].visibility(), VeryHidden);
        }
        // Reordering keeps source identities and deferred display indexes
        // distinct. Rename/visibility overlays remain keyed to original parts.
        workbook.set_sheet_visibility(second, Visible).unwrap();
        workbook.set_active_view_index(0).unwrap();
        workbook.move_sheet(second, 0).unwrap();
        assert_eq!(workbook.model().active_sheet(), Some(second));
        assert_eq!(workbook.model().sheets().next().unwrap().0, second);
        workbook.set_sheet_visibility(first, Hidden).unwrap();
        workbook.set_active_sheet(second).unwrap();
        let cells = workbook.model().cell_count();
        for _ in 0..2 {
            let (output, stats) = workbook
                .save(Cursor::new(Vec::new()), Default::default())
                .unwrap();
            assert_eq!(stats.rewritten_parts, 1);
            let saved = parts(output.into_inner());
            for (name, bytes) in &input {
                if name != workbook_part {
                    assert_eq!(&saved[name], bytes, "reordered {mode}: {name}");
                }
            }
            let reader = crabxl_xlsx::WorkbookReader::new(Cursor::new(package(saved))).unwrap();
            assert_eq!(reader.sheets()[0].name(), renamed);
            assert_eq!(reader.sheets()[1].name(), "RenamedFirst");
            assert_eq!(reader.sheets()[1].visibility(), Hidden);
            assert_eq!(reader.active_index(), Some(0));
        }
        assert_eq!(workbook.model().cell_count(), cells);
        workbook.move_sheet(first, 0).unwrap();
        assert_eq!(workbook.model().active_sheet(), Some(first));
        assert_eq!(workbook.active_view_index(), 0);
        assert!(workbook.move_sheet(first, 2).is_err());
    }
    for mode in ["hidden", "signed", "alternative", "patch-cap"] {
        let mut input = original.clone();
        let options = if mode == "patch-cap" {
            LoadOptions {
                editor: crabxl_xlsx::EditorOptions {
                    max_patch_bytes: 1,
                    ..Default::default()
                },
                ..Default::default()
            }
        } else {
            LoadOptions::default()
        };
        if mode == "signed" {
            input.insert("_xmlsignatures/sig1.xml".into(), b"<signature/>".to_vec());
        } else if mode == "hidden" {
            let xml = String::from_utf8(input.remove("xl/workbook.xml").unwrap()).unwrap();
            input.insert(
                "xl/workbook.xml".into(),
                xml.replace("name=\"Second\"", "name=\"Second\" state=\"hidden\"")
                    .into_bytes(),
            );
        } else if mode == "alternative" {
            let xml = String::from_utf8(input.remove("xl/workbook.xml").unwrap()).unwrap();
            input.insert("xl/workbook.xml".into(), xml.replace("</workbook>", "<mc:AlternateContent xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\"><mc:Fallback/></mc:AlternateContent></workbook>").into_bytes());
        }
        let mut workbook =
            LoadedWorkbook::with_options(Cursor::new(package(input)), options).unwrap();
        let first = workbook.sheet_id("First").unwrap();
        let second = workbook.sheet_id("Second").unwrap();
        let before = workbook.managed_retained_bytes();
        let error = workbook.set_active_sheet(second).unwrap_err();
        assert_eq!(
            error.kind(),
            match mode {
                "hidden" => ErrorKind::InvalidData,
                "patch-cap" => ErrorKind::MemoryBudgetExceeded,
                _ => ErrorKind::Unsupported,
            }
        );
        assert_eq!(workbook.model().active_sheet(), Some(first));
        assert_eq!(workbook.patch_bytes(), 0);
        assert_eq!(workbook.managed_retained_bytes(), before);
        if mode != "hidden" {
            assert!(workbook.rename_sheet(second, "Renamed").is_err());
            assert_eq!(workbook.sheet_id("Second"), Some(second));
            assert!(workbook.move_sheet(second, 0).is_err());
            assert_eq!(workbook.model().sheets().next().unwrap().0, first);
            assert!(
                workbook
                    .set_sheet_visibility(second, crabxl_core::SheetVisibility::Hidden)
                    .is_err()
            );
            assert_eq!(
                workbook.model().sheet(second).unwrap().visibility(),
                crabxl_core::SheetVisibility::Visible
            );
            assert_eq!(workbook.patch_bytes(), 0);
            assert_eq!(workbook.managed_retained_bytes(), before);
            assert!(workbook.set_active_view_index(-1).is_err());
            assert!(
                workbook
                    .set_sheet_visibility_and_active_view(
                        second,
                        crabxl_core::SheetVisibility::Hidden,
                        -1
                    )
                    .is_err()
            );
            assert_eq!(workbook.model().active_sheet(), Some(first));
            assert_eq!(workbook.patch_bytes(), 0);
            assert_eq!(workbook.managed_retained_bytes(), before);
        }
    }
    let mut editor =
        crabxl_xlsx::WorkbookEditor::new(Cursor::new(package(original.clone()))).unwrap();
    editor.move_sheet("Second", 0).unwrap();
    editor.set_active_sheet("Second").unwrap();
    assert!(editor.is_dirty());
    editor.clear_edits();
    assert!(!editor.is_dirty());
    assert_eq!(editor.patch_bytes(), 0);
    let (output, stats) = editor
        .save(Cursor::new(Vec::new()), Default::default())
        .unwrap();
    assert_eq!(stats.rewritten_parts, 0);
    assert_eq!(parts(output.into_inner()), original);
    for local in [false, true] {
        let mut input = original.clone();
        let xml = String::from_utf8(input.remove("xl/workbook.xml").unwrap()).unwrap();
        let names = if local {
            "<definedNames><definedName name=\"Scope\" localSheetId=\"0\">First!$A$1</definedName></definedNames>"
        } else {
            "<definedNames><definedName name=\"Scope\">First!$A$1</definedName></definedNames>"
        };
        input.insert(
            "xl/workbook.xml".into(),
            xml.replace("</workbook>", &format!("{names}</workbook>"))
                .into_bytes(),
        );
        let mut book =
            LoadedWorkbook::with_options(Cursor::new(package(input)), LoadOptions::default())
                .unwrap();
        let first = book.sheet_id("First").unwrap();
        let second = book.sheet_id("Second").unwrap();
        let before = book.managed_retained_bytes();
        if local {
            assert_eq!(
                book.move_sheet(second, 0).unwrap_err().kind(),
                ErrorKind::Unsupported
            );
            assert_eq!(book.model().sheets().next().unwrap().0, first);
            assert_eq!(book.patch_bytes(), 0);
            assert_eq!(book.managed_retained_bytes(), before);
        } else {
            book.move_sheet(second, 0).unwrap();
            assert_eq!(book.model().cell_count(), 0);
            assert_eq!(book.model().sheets().next().unwrap().0, second);
            let (output, _) = book
                .save(Cursor::new(Vec::new()), Default::default())
                .unwrap();
            let saved = parts(output.into_inner());
            assert!(
                String::from_utf8(saved["xl/workbook.xml"].clone())
                    .unwrap()
                    .contains(names)
            );
        }
    }
    // Deferred view indexes match public reference behavior without eager cells.
    for (count, requested, after, read_index) in [
        (2, -3, 0, 0),
        (2, -1, 0, 0),
        (2, 1, 1, 0),
        (2, 10, 10, 0),
        (3, -3, -3, 0),
        (3, -1, -1, 2),
        (3, 1, 2, 2),
        (3, 10, 10, 0),
    ] {
        let mut input = original.clone();
        let mut xml = String::from_utf8(input.remove("xl/workbook.xml").unwrap())
            .unwrap()
            .replace("name=\"Second\"", "name=\"Second\" state=\"hidden\"");
        if count == 3 {
            xml = xml.replace(
                "</sheets>",
                "<sheet name=\"Third\" sheetId=\"3\" r:id=\"viewThird\"/></sheets>",
            );
            input.insert(
                "xl/worksheets/sheet3.xml".into(),
                input["xl/worksheets/sheet2.xml"].clone(),
            );
            let rels =
                String::from_utf8(input.remove("xl/_rels/workbook.xml.rels").unwrap()).unwrap();
            input.insert("xl/_rels/workbook.xml.rels".into(), rels.replace("</Relationships>", "<Relationship Id=\"viewThird\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet3.xml\"/></Relationships>").into_bytes());
            let types = String::from_utf8(input.remove("[Content_Types].xml").unwrap()).unwrap();
            input.insert("[Content_Types].xml".into(), types.replace("</Types>", "<Override PartName=\"/xl/worksheets/sheet3.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/></Types>").into_bytes());
        }
        xml = xml.replacen("activeTab=\"0\"", &format!("activeTab=\"{requested}\""), 1);
        input.insert("xl/workbook.xml".into(), xml.into_bytes());
        let mut workbook = LoadedWorkbook::with_options(
            Cursor::new(package(input.clone())),
            LoadOptions::default(),
        )
        .unwrap();
        assert_eq!(
            workbook.model().active_index(),
            crabxl_core::resolve_sheet_index(requested, count)
        );
        workbook.set_active_view_index(requested).unwrap();
        assert_eq!(
            workbook.model().active_index(),
            crabxl_core::resolve_sheet_index(requested, count)
        );
        for _ in 0..2 {
            let (output, _) = workbook
                .save(Cursor::new(Vec::new()), Default::default())
                .unwrap();
            assert_eq!(workbook.active_view_index(), after);
            assert_eq!(
                workbook.model().active_index(),
                crabxl_core::resolve_sheet_index(after, count)
            );
            assert_eq!(workbook.model().cell_count(), 0);
            let saved = parts(output.into_inner());
            for (name, bytes) in &input {
                if name != "xl/workbook.xml" {
                    assert_eq!(&saved[name], bytes);
                }
            }
            let reader = crabxl_xlsx::WorkbookReader::new(Cursor::new(package(saved))).unwrap();
            assert_eq!(reader.active_index(), Some(read_index));
        }
    }
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

#[test]
fn preserving_overlays_share_lazy_models_and_failed_mutations_leave_both_states_usable() {
    let bytes = source(3, false);
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let mut original = ZipArchive::new(Cursor::new(bytes)).unwrap();
    for index in 0..original.len() {
        zip.raw_copy_file(original.by_index(index).unwrap())
            .unwrap();
    }
    zip.start_file("opaque/unaffected.bin", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"retained original asset").unwrap();
    let bytes = zip.finish().unwrap().into_inner();
    let mut workbook = LoadedWorkbook::with_options(
        Cursor::new(bytes.clone()),
        LoadOptions {
            editor: crabxl_xlsx::EditorOptions {
                max_patch_cells: 3,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
    let first = workbook.sheet_id("First").unwrap();
    let second = workbook.sheet_id("Second").unwrap();
    workbook
        .upsert_value(
            second,
            CellAddress::new(0, 0).unwrap(),
            CellValue::text("lazy overlay"),
        )
        .unwrap();
    assert!(!workbook.is_materialized(second));
    assert!(
        matches!(workbook.pending_value(second, CellAddress::new(0, 0).unwrap()), Some(CellValue::Text(text)) if text.as_str() == "lazy overlay")
    );
    workbook.sheet(first).unwrap();
    workbook
        .set_value(
            first,
            CellAddress::new(0, 0).unwrap(),
            CellValue::Integer(7),
        )
        .unwrap();
    workbook
        .upsert_value(
            first,
            CellAddress::new(5, 2).unwrap(),
            CellValue::Integer(33),
        )
        .unwrap();
    assert_eq!(
        workbook
            .sheet(first)
            .unwrap()
            .get(CellAddress::new(0, 0).unwrap())
            .unwrap()
            .value,
        CellValue::Integer(7)
    );
    let before = workbook.managed_retained_bytes();
    assert_eq!(
        workbook
            .upsert_value(
                first,
                CellAddress::new(1, 0).unwrap(),
                CellValue::Integer(99)
            )
            .unwrap_err()
            .kind(),
        ErrorKind::MemoryBudgetExceeded
    );
    assert_eq!(workbook.managed_retained_bytes(), before);
    assert!(
        workbook
            .pending_value(first, CellAddress::new(1, 0).unwrap())
            .is_none()
    );
    assert_eq!(
        workbook
            .sheet(first)
            .unwrap()
            .get(CellAddress::new(1, 0).unwrap())
            .unwrap()
            .value,
        CellValue::Integer(1)
    );
    assert!(
        matches!(&workbook.sheet(second).unwrap().get(CellAddress::new(0, 0).unwrap()).unwrap().value, CellValue::Text(text) if text.as_str() == "lazy overlay")
    );
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("output.xlsx");
    std::fs::write(&target, b"keep target").unwrap();
    assert!(
        workbook
            .save_path(
                &target,
                crabxl_xlsx::SaveOptions {
                    compression_level: Some(10),
                    ..Default::default()
                }
            )
            .is_err()
    );
    assert_eq!(std::fs::read(&target).unwrap(), b"keep target");
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    for _ in 0..2 {
        workbook
            .save_path(&target, crabxl_xlsx::SaveOptions::default())
            .unwrap();
        let mut saved = crabxl_xlsx::WorkbookReader::open(&target).unwrap();
        assert_eq!(
            saved
                .read_sheet("First")
                .unwrap()
                .rows
                .last()
                .unwrap()
                .cells[0]
                .value,
            CellValue::Integer(33)
        );
        assert!(
            matches!(&saved.read_sheet("Second").unwrap().rows[0].cells[0].value, CellValue::Text(text) if text.as_str() == "lazy overlay")
        );
        let mut archive = ZipArchive::new(std::fs::File::open(&target).unwrap()).unwrap();
        let mut asset = Vec::new();
        archive
            .by_name("opaque/unaffected.bin")
            .unwrap()
            .read_to_end(&mut asset)
            .unwrap();
        assert_eq!(asset, b"retained original asset");
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }
    assert!(workbook.managed_retained_bytes() <= workbook.memory_allowance().retained_data_bytes);
    assert_eq!(workbook.into_source().into_inner(), bytes);

    // Append uses actual source rows, including explicit empty rows, rather
    // than either undersized or oversized advertised dimensions. Failed rows
    // commit neither overlays nor cursor changes; repeated saves retain assets.
    for dimension in ["A1:A1", "A1:XFD1048576"] {
        let mut original = parts(bytes.clone());
        let xml = String::from_utf8(original.remove("xl/worksheets/sheet1.xml").unwrap()).unwrap();
        let xml = xml
            .replace(
                "<sheetData>",
                &format!("<dimension ref=\"{dimension}\"/><sheetData>"),
            )
            .replace("</sheetData>", "<row r=\"9\"/></sheetData>");
        original.insert("xl/worksheets/sheet1.xml".into(), xml.into_bytes());
        let mut appended = LoadedWorkbook::with_options(
            Cursor::new(package(original)),
            LoadOptions {
                editor: crabxl_xlsx::EditorOptions {
                    max_patch_cells: 2,
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .unwrap();
        let id = appended.sheet_id("First").unwrap();
        assert_eq!(appended.sheet(id).unwrap().row_extent(), 9);
        let retained = appended.managed_retained_bytes();
        assert_eq!(
            appended
                .append(id, vec![CellValue::Integer(1); 3])
                .unwrap_err()
                .kind(),
            ErrorKind::MemoryBudgetExceeded
        );
        assert_eq!(appended.sheet(id).unwrap().row_extent(), 9);
        assert_eq!(appended.patch_bytes(), 0);
        assert_eq!(appended.managed_retained_bytes(), retained);
        let date = ExcelDateTime::from_serial(
            2.5,
            crabxl_core::DateEpoch::Windows1900,
            DateKind::DateTime,
        )
        .unwrap();
        assert_eq!(
            appended
                .append(
                    id,
                    vec![CellValue::Integer(1), CellValue::DateTime(Box::new(date))]
                )
                .unwrap_err()
                .kind(),
            ErrorKind::Unsupported
        );
        assert_eq!(appended.patch_bytes(), 0);
        assert_eq!(appended.sheet(id).unwrap().row_extent(), 9);
        assert_eq!(
            appended
                .append(
                    id,
                    vec![CellValue::Integer(41), CellValue::text("appended")]
                )
                .unwrap()
                .get(),
            9
        );
        assert_eq!(appended.append(id, Vec::new()).unwrap().get(), 10);
        assert_eq!(appended.sheet(id).unwrap().row_extent(), 11);
        assert!(!appended.is_materialized(appended.sheet_id("Second").unwrap()));
        for _ in 0..2 {
            let mut target = Cursor::new(Vec::new());
            appended.save(&mut target, Default::default()).unwrap();
            let saved = target.into_inner();
            let mut reloaded =
                LoadedWorkbook::with_options(Cursor::new(saved.clone()), Default::default())
                    .unwrap();
            let id = reloaded.sheet_id("First").unwrap();
            assert_eq!(
                reloaded
                    .sheet(id)
                    .unwrap()
                    .get(CellAddress::new(9, 0).unwrap())
                    .unwrap()
                    .value,
                CellValue::Integer(41)
            );
            assert!(
                matches!(&reloaded.sheet(id).unwrap().get(CellAddress::new(9, 1).unwrap()).unwrap().value, CellValue::Text(text) if text.as_str() == "appended")
            );
            assert_eq!(
                parts(saved)["opaque/unaffected.bin"],
                b"retained original asset"
            );
        }
    }
}
