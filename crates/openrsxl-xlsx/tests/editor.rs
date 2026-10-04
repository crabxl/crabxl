//! Generated original-package preservation and atomic-output/failure fixtures.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use openrsxl_core::{
    Cell, CellAddress as Address, CellValue as Value, ErrorKind, Formula, ResourceLimits, Row,
    RowIndex, StyleId,
};
use openrsxl_xlsx::{
    EditorOptions, SaveOptions, WorkbookEditor, WorkbookReader, WorkbookWriter, WriteOptions,
};
use std::{
    collections::BTreeMap,
    io::{Cursor, Read, Write},
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};
const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
fn source() -> Vec<u8> {
    let mut writer = WorkbookWriter::new(WriteOptions::default()).unwrap();
    for name in ["Sheet", "Other"] {
        writer.start_sheet(name).unwrap();
        writer
            .write_row(&Row {
                index: RowIndex::new(0).unwrap(),
                cells: vec![
                    Cell {
                        address: Address::new(0, 0).unwrap(),
                        value: Value::Integer(1),
                        style: StyleId::new(0),
                    },
                    Cell {
                        address: Address::new(0, 1).unwrap(),
                        value: Value::Formula(Box::new(
                            Formula::new("=A1+1", Some(Value::Integer(2))).unwrap(),
                        )),
                        style: StyleId::new(0),
                    },
                ],
            })
            .unwrap();
    }
    writer.finish(Cursor::new(Vec::new())).unwrap().into_inner()
}
fn parts(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut zip = ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..zip.len())
        .map(|index| {
            let mut file = zip.by_index(index).unwrap();
            let name = file.name().into();
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).unwrap();
            (name, bytes)
        })
        .collect()
}
fn packed(parts: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    zip.set_comment("Original package comment").unwrap();
    for (name, bytes) in parts {
        zip.start_file(
            name,
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
        )
        .unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}
fn rich_source() -> Vec<u8> {
    let mut data = parts(&source());
    data.insert("xl/media/image1.png".into(), vec![0, 1, 2, 255]);
    data.insert("xl/vbaProject.bin".into(), vec![255, 254, 253, 0]);
    data.insert(
        "custom/original.xml".into(),
        b"<u:root xmlns:u=\"urn:unknown\"><u:node><![CDATA[opaque <text>]]></u:node></u:root>"
            .to_vec(),
    );
    let sheet=String::from_utf8(data["xl/worksheets/sheet1.xml"].clone()).unwrap().replace("</worksheet>","<extLst><ext uri=\"opaque\"><f><v>keep me</v></f><x:data xmlns:x=\"urn:custom\" x:flag=\"yes\"><![CDATA[ unchanged ]]></x:data></ext></extLst></worksheet>");
    data.insert("xl/worksheets/sheet1.xml".into(), sheet.into_bytes());
    packed(&data)
}
#[test]
fn unchanged_saves_keep_every_original_part_and_repeat_with_owned_source() {
    let original = rich_source();
    let expected = parts(&original);
    let mut editor = WorkbookEditor::new(Cursor::new(original.clone())).unwrap();
    assert_eq!(editor.parts().len(), expected.len());
    assert!(!editor.is_dirty());
    for verify_unchanged in [false, true] {
        let (output, stats) = editor
            .save(Cursor::new(Vec::new()), SaveOptions { verify_unchanged })
            .unwrap();
        assert_eq!(stats.rewritten_parts, 0);
        assert_eq!(stats.copied_parts, expected.len());
        assert_eq!(parts(output.get_ref()), expected);
        assert_eq!(
            ZipArchive::new(output).unwrap().comment(),
            b"Original package comment"
        );
    }
    assert_eq!(editor.into_source().into_inner(), original);
}
#[test]
fn one_cell_edit_preserves_assets_extensions_and_clears_caches_across_sheets() {
    let original = rich_source();
    let expected = parts(&original);
    let mut editor = WorkbookEditor::new(Cursor::new(original)).unwrap();
    editor
        .set_value(
            "Sheet",
            Address::new(0, 0).unwrap(),
            Value::text(" <&>\r\n "),
        )
        .unwrap();
    assert!(editor.is_dirty());
    assert!(editor.patch_bytes() > 0);
    let mut previous = None;
    for _ in 0..2 {
        let (output, stats) = editor
            .save(Cursor::new(Vec::new()), SaveOptions::default())
            .unwrap();
        assert_eq!(stats.rewritten_parts, 3);
        let actual = parts(output.get_ref());
        for (name, bytes) in &expected {
            if ![
                "xl/worksheets/sheet1.xml",
                "xl/worksheets/sheet2.xml",
                "xl/workbook.xml",
            ]
            .contains(&name.as_str())
            {
                assert_eq!(&actual[name], bytes, "Part {name}");
            }
        }
        let sheet = String::from_utf8(actual["xl/worksheets/sheet1.xml"].clone()).unwrap();
        assert!(sheet.contains("keep me"));
        assert!(sheet.contains("<![CDATA[ unchanged ]]>"));
        assert!(sheet.contains("x:flag=\"yes\""));
        let mut book = WorkbookReader::new(output).unwrap();
        assert_eq!(
            book.read_sheet("Sheet").unwrap().rows[0].cells[0].value,
            Value::text(" <&>\r\n ")
        );
        for name in ["Sheet", "Other"] {
            let row = book.read_sheet(name).unwrap().rows.remove(0);
            assert!(
                matches!(&row.cells[1].value,Value::Formula(formula) if formula.cached().is_none() && formula.expression()=="A1+1")
            );
        }
        let workbook = String::from_utf8(actual["xl/workbook.xml"].clone()).unwrap();
        assert!(workbook.contains("fullCalcOnLoad=\"1\""));
        if let Some(previous) = &previous {
            assert_eq!(&actual, previous);
        }
        previous = Some(actual);
    }
    assert!(editor.is_dirty());
    editor.clear_edits();
    assert!(!editor.is_dirty());
    assert_eq!(editor.patch_bytes(), 0);
    let (output, _) = editor
        .save(Cursor::new(Vec::new()), SaveOptions::default())
        .unwrap();
    assert_eq!(parts(output.get_ref()), expected);
}
#[test]
fn prefixes_strict_namespaces_inferred_addresses_and_styles_are_preserved() {
    for uri in [MAIN, "http://purl.oclc.org/ooxml/spreadsheetml/main"] {
        let mut data = parts(&source());
        let sheet = format!(
            "<s:worksheet xmlns:s=\"{uri}\" xmlns:x=\"urn:custom\"><s:sheetData><s:row r=\"1\" x:row=\"keep\"><s:c s=\"2\" x:cell=\"keep\"><s:v>1</s:v></s:c><s:c><s:f>A1+1</s:f><s:v>2</s:v></s:c></s:row></s:sheetData></s:worksheet>"
        );
        data.insert("xl/worksheets/sheet1.xml".into(), sheet.into_bytes());
        let mut editor = WorkbookEditor::new(Cursor::new(packed(&data))).unwrap();
        editor
            .set_value("Sheet", Address::new(0, 0).unwrap(), Value::Boolean(false))
            .unwrap();
        let (output, _) = editor
            .save(Cursor::new(Vec::new()), SaveOptions::default())
            .unwrap();
        let data = parts(output.get_ref());
        let xml = String::from_utf8(data["xl/worksheets/sheet1.xml"].clone()).unwrap();
        assert!(xml.contains(&format!("xmlns=\"{uri}\"")));
        assert!(xml.contains("s=\"2\""));
        assert!(xml.contains("x:cell=\"keep\""));
        assert!(xml.contains("x:row=\"keep\""));
        assert!(xml.contains("<v>0</v>"));
    }
}
#[test]
fn macro_and_template_main_types_and_relationships_survive_edits() {
    for kind in [
        "application/vnd.ms-excel.sheet.macroEnabled.main+xml",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.template.main+xml",
        "application/vnd.ms-excel.template.macroEnabled.main+xml",
    ] {
        let mut data = parts(&rich_source());
        let types = String::from_utf8(data["[Content_Types].xml"].clone())
            .unwrap()
            .replace(
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml",
                kind,
            );
        data.insert("[Content_Types].xml".into(), types.into_bytes());
        let mut editor = WorkbookEditor::new(Cursor::new(packed(&data))).unwrap();
        editor
            .set_value("Sheet", Address::new(0, 0).unwrap(), Value::Integer(9))
            .unwrap();
        let (output, _) = editor
            .save(Cursor::new(Vec::new()), SaveOptions::default())
            .unwrap();
        let actual = parts(output.get_ref());
        assert_eq!(actual["[Content_Types].xml"], data["[Content_Types].xml"]);
        assert_eq!(actual["xl/vbaProject.bin"], data["xl/vbaProject.bin"]);
        assert_eq!(
            actual["xl/_rels/workbook.xml.rels"],
            data["xl/_rels/workbook.xml.rels"]
        );
    }
}
#[test]
fn unsupported_targets_and_missing_cells_leave_atomic_path_target_unchanged() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("target.xlsx");
    std::fs::write(&target, b"original target").unwrap();
    for cell in [
        "<c r=\"A1\" vm=\"1\"><v>1</v></c>",
        "<c r=\"A1\"><f t=\"shared\" si=\"0\">A1+1</f><v>2</v></c>",
        "<c r=\"A1\"><extLst><ext/></extLst><v>1</v></c>",
        "<c r=\"B1\"><v>1</v></c>",
    ] {
        let mut data = parts(&source());
        data.insert("xl/worksheets/sheet1.xml".into(),format!("<worksheet xmlns=\"{MAIN}\"><sheetData><row r=\"1\">{cell}</row></sheetData></worksheet>").into_bytes());
        let mut editor = WorkbookEditor::new(Cursor::new(packed(&data))).unwrap();
        editor
            .set_value("Sheet", Address::new(0, 0).unwrap(), Value::Integer(9))
            .unwrap();
        assert!(editor.save_path(&target, SaveOptions::default()).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"original target");
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
        editor.clear_edits();
        assert!(
            editor
                .save(Cursor::new(Vec::new()), SaveOptions::default())
                .is_ok()
        );
    }
}
#[test]
fn save_over_original_path_and_resave_retain_original_source_images() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("book.xlsx");
    std::fs::write(&path, rich_source()).unwrap();
    let mut editor = WorkbookEditor::open(&path).unwrap();
    editor
        .set_value("Sheet", Address::new(0, 0).unwrap(), Value::Integer(42))
        .unwrap();
    editor.save_path(&path, SaveOptions::default()).unwrap();
    let once = parts(&std::fs::read(&path).unwrap());
    editor.save_path(&path, SaveOptions::default()).unwrap();
    assert_eq!(parts(&std::fs::read(&path).unwrap()), once);
    assert_eq!(once["xl/media/image1.png"], vec![0, 1, 2, 255]);
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}
#[test]
fn patch_budgets_signed_sources_and_calculation_chain_edits_are_guarded() {
    let mut editor = WorkbookEditor::with_options(
        Cursor::new(source()),
        EditorOptions {
            max_patch_bytes: 600,
            max_patch_cells: 1,
            ..EditorOptions::default()
        },
    )
    .unwrap();
    editor
        .set_value("Sheet", Address::new(0, 0).unwrap(), Value::Integer(2))
        .unwrap();
    let before = editor.patch_bytes();
    assert!(
        editor
            .set_value("Other", Address::new(0, 0).unwrap(), Value::Integer(3))
            .is_err()
    );
    assert_eq!(editor.patch_bytes(), before);
    assert!(
        editor
            .set_value(
                "Sheet",
                Address::new(0, 0).unwrap(),
                Value::text("x".repeat(2000).into_boxed_str())
            )
            .is_err()
    );
    assert_eq!(
        editor.pending_value("Sheet", Address::new(0, 0).unwrap()),
        Some(&Value::Integer(2))
    );
    for kind in [
        "application/vnd.openxmlformats-package.digital-signature-xmlsignature+xml",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.calcChain+xml",
    ] {
        let mut data = parts(&source());
        let types = String::from_utf8(data["[Content_Types].xml"].clone())
            .unwrap()
            .replace(
                "</Types>",
                &format!(
                    "<Override PartName=\"/custom/guard.xml\" ContentType=\"{kind}\"/></Types>"
                ),
            );
        data.insert("[Content_Types].xml".into(), types.into_bytes());
        data.insert("custom/guard.xml".into(), b"<opaque/>".to_vec());
        let mut editor = WorkbookEditor::new(Cursor::new(packed(&data))).unwrap();
        assert_eq!(
            editor
                .set_value("Sheet", Address::new(0, 0).unwrap(), Value::Integer(9))
                .unwrap_err()
                .kind(),
            ErrorKind::Unsupported
        );
        assert!(
            editor
                .save(Cursor::new(Vec::new()), SaveOptions::default())
                .is_ok()
        );
    }
}
#[test]
fn source_inventory_and_rewritten_part_limits_are_enforced() {
    let options = EditorOptions {
        resources: ResourceLimits {
            max_metadata_bytes: 2000,
            ..ResourceLimits::default()
        },
        ..EditorOptions::default()
    };
    assert!(WorkbookEditor::with_options(Cursor::new(rich_source()), options).is_err());
    let mut data = parts(&source());
    data.insert("xl/worksheets/sheet1.xml".into(),format!("<worksheet xmlns=\"{MAIN}\"><sheetData><row><c><v>1</v></c></row></sheetData></worksheet>").into_bytes());
    // The largest metadata part fits, but the replacement can exceed it.
    let largest = data.values().map(|value| value.len() as u64).max().unwrap();
    let mut editor = WorkbookEditor::with_options(
        Cursor::new(packed(&data)),
        EditorOptions {
            resources: ResourceLimits {
                max_part_bytes: largest + 100,
                ..ResourceLimits::default()
            },
            ..EditorOptions::default()
        },
    )
    .unwrap();
    editor
        .set_value(
            "Sheet",
            Address::new(0, 0).unwrap(),
            Value::text("x".repeat(largest as usize + 200).into_boxed_str()),
        )
        .unwrap();
    assert!(
        editor
            .save(Cursor::new(Vec::new()), SaveOptions::default())
            .is_err()
    );
}

#[test]
fn editor_auto_and_explicit_budgets_share_the_reader_policy() {
    use openrsxl_core::{AutoMemory, MemoryPolicy, MemorySource};
    let mut allowances = Vec::new();
    for available in [32 * 1024 * 1024, 1024 * 1024 * 1024] {
        let editor = WorkbookEditor::with_options(
            Cursor::new(source()),
            EditorOptions {
                memory_policy: MemoryPolicy::Auto(AutoMemory {
                    available_bytes: Some(available),
                    ..AutoMemory::default()
                }),
                ..EditorOptions::default()
            },
        )
        .unwrap();
        assert_eq!(
            editor.memory_allowance().memory_source,
            MemorySource::CallerAvailability
        );
        assert_eq!(
            editor.patch_allowance(),
            editor.memory_allowance().retained_data_bytes
        );
        allowances.push(editor.patch_allowance());
    }
    assert!(allowances[1] > allowances[0]);
    let editor = WorkbookEditor::with_options(
        Cursor::new(source()),
        EditorOptions {
            memory_policy: MemoryPolicy::Budget(4 * 1024 * 1024),
            max_patch_bytes: 1000,
            ..EditorOptions::default()
        },
    )
    .unwrap();
    assert_eq!(editor.patch_allowance(), 1000);
    assert_eq!(
        editor.memory_allowance().memory_source,
        MemorySource::ExplicitBudget
    );
    assert!(
        WorkbookEditor::with_options(
            Cursor::new(source()),
            EditorOptions {
                memory_policy: MemoryPolicy::Budget(1),
                ..EditorOptions::default()
            }
        )
        .is_err()
    );
}
#[test]
fn injected_output_failure_retains_source_overlays_and_allows_retry() {
    struct Failure {
        position: u64,
    }
    impl Write for Failure {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("Injected edit output failure"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl std::io::Seek for Failure {
        fn seek(&mut self, from: std::io::SeekFrom) -> std::io::Result<u64> {
            if let std::io::SeekFrom::Start(position) = from {
                self.position = position;
            }
            Ok(self.position)
        }
    }
    let mut editor = WorkbookEditor::new(Cursor::new(rich_source())).unwrap();
    editor
        .set_value("Sheet", Address::new(0, 0).unwrap(), Value::Integer(7))
        .unwrap();
    let error = match editor.save(Failure { position: 0 }, SaveOptions::default()) {
        Ok(_) => panic!("Injected failure unexpectedly succeeded"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), ErrorKind::Io);
    assert!(std::error::Error::source(&error).is_some());
    assert_eq!(
        editor.pending_value("Sheet", Address::new(0, 0).unwrap()),
        Some(&Value::Integer(7))
    );
    let (output, _) = editor
        .save(Cursor::new(Vec::new()), SaveOptions::default())
        .unwrap();
    assert_eq!(
        WorkbookReader::new(output)
            .unwrap()
            .read_sheet("Sheet")
            .unwrap()
            .rows[0]
            .cells[0]
            .value,
        Value::Integer(7)
    );
}
#[test]
fn optional_validation_detects_opaque_part_crc_corruption() {
    let mut data = parts(&source());
    data.insert("custom/binary.bin".into(), b"payload CRC marker".to_vec());
    // Stored opaque entry makes corruption reproducible without changing ZIP metadata.
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in &data {
        zip.start_file(
            name,
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
        )
        .unwrap();
        zip.write_all(bytes).unwrap();
    }
    let mut bytes = zip.finish().unwrap().into_inner();
    let position = bytes
        .windows(b"payload CRC marker".len())
        .position(|bytes| bytes == b"payload CRC marker")
        .unwrap();
    bytes[position] ^= 1;
    let mut editor = WorkbookEditor::new(Cursor::new(bytes)).unwrap();
    assert!(
        editor
            .save(Cursor::new(Vec::new()), SaveOptions::default())
            .is_ok()
    );
    assert!(
        editor
            .save(
                Cursor::new(Vec::new()),
                SaveOptions {
                    verify_unchanged: true
                }
            )
            .is_err()
    );
}

#[test]
fn shared_string_ids_rich_runs_and_optional_reference_counts_remain_consistent() {
    let mut data = parts(&source());
    let types=String::from_utf8(data["[Content_Types].xml"].clone()).unwrap().replace("</Types>","<Override PartName=\"/custom/strings.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml\"/></Types>");
    data.insert("[Content_Types].xml".into(), types.into_bytes());
    data.insert("custom/strings.xml".into(),format!("<sst xmlns=\"{MAIN}\" xmlns:x=\"urn:custom\" count=\"2\" uniqueCount=\"1\" x:flag=\"keep\"><si><r><rPr><b/></rPr><t xml:space=\"preserve\"> shared </t></r></si></sst>").into_bytes());
    data.insert("xl/worksheets/sheet1.xml".into(),format!("<worksheet xmlns=\"{MAIN}\"><sheetData><row><c t=\"s\"><v>0</v></c><c t=\"s\"><v>0</v></c></row></sheetData></worksheet>").into_bytes());
    let mut editor = WorkbookEditor::new(Cursor::new(packed(&data))).unwrap();
    editor
        .set_value(
            "Sheet",
            Address::new(0, 0).unwrap(),
            Value::text("new inline text"),
        )
        .unwrap();
    let (output, stats) = editor
        .save(Cursor::new(Vec::new()), SaveOptions::default())
        .unwrap();
    assert_eq!(stats.rewritten_parts, 4);
    let actual = parts(output.get_ref());
    let strings = String::from_utf8(actual["custom/strings.xml"].clone()).unwrap();
    assert!(!strings.contains(" count="));
    assert!(strings.contains("uniqueCount=\"1\""));
    assert!(strings.contains(" shared "));
    assert!(strings.contains("<b></b>"));
    assert!(strings.contains("x:flag=\"keep\""));
    let sheet = String::from_utf8(actual["xl/worksheets/sheet1.xml"].clone()).unwrap();
    assert!(sheet.contains("new inline text"));
    assert!(sheet.contains("t=\"s\"><v>0</v>"));
}
