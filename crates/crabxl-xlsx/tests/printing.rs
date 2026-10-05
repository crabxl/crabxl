//! Generated printing fixtures and public descriptor-compatible round trips.
#![allow(clippy::unwrap_used)]
use crabxl_core::{
    Cell, CellValue, EditLimits, ErrorKind, PageBreak, PageMargins, PageOrder, PageOrientation,
    PaperDimension, PrintSettings, PrintedComments, PrintedErrors, Row, RowIndex, SheetViews,
    StyleId, Worksheet,
};
use crabxl_xlsx::{
    EditorOptions, SaveOptions, WorkbookEditor, WorkbookReader, WorkbookWriter, WriteOptions,
};
use std::io::{Cursor, Read, Write};
fn settings() -> PrintSettings {
    let mut value = PrintSettings {
        auto_page_breaks: Some(false),
        fit_to_page: Some(true),
        margins: Some(PageMargins {
            left: -1.5,
            right: 0.25,
            top: 0.5,
            bottom: 0.75,
            header: 0.2,
            footer: 0.3,
        }),
        ..PrintSettings::default()
    };
    value.options.horizontal_centered = Some(true);
    value.options.vertical_centered = Some(false);
    value.options.headings = Some(true);
    value.options.grid_lines = Some(false);
    value.options.grid_lines_set = Some(true);
    value.setup.orientation = Some(PageOrientation::Landscape);
    value.setup.paper_size = Some(9);
    value.setup.scale = Some(85);
    value.setup.fit_to_height = Some(0);
    value.setup.fit_to_width = Some(1);
    value.setup.first_page_number = Some(7);
    value.setup.use_first_page_number = Some(true);
    value.setup.paper_height = Some(PaperDimension::parse("11.5in tail").unwrap());
    value.setup.paper_width = Some(PaperDimension::parse("8.25in").unwrap());
    value.setup.page_order = Some(PageOrder::OverThenDown);
    value.setup.use_printer_defaults = Some(false);
    value.setup.black_and_white = Some(true);
    value.setup.draft = Some(false);
    value.setup.cell_comments = Some(PrintedComments::AtEnd);
    value.setup.errors = Some(PrintedErrors::NotAvailable);
    value.setup.horizontal_dpi = Some(300);
    value.setup.vertical_dpi = Some(600);
    value.setup.copies = Some(2);
    value.row_breaks = vec![
        PageBreak {
            id: Some(10),
            minimum: Some(2),
            maximum: Some(7),
            manual: Some(false),
            pivot: Some(true),
        },
        PageBreak {
            id: Some(25),
            ..PageBreak::default()
        },
    ];
    value.column_breaks = vec![PageBreak {
        id: Some(3),
        minimum: Some(1),
        maximum: Some(1048575),
        manual: None,
        pivot: Some(false),
    }];
    value
}
fn fixture(value: Option<&PrintSettings>) -> Vec<u8> {
    let mut writer = WorkbookWriter::new(WriteOptions::default()).unwrap();
    writer
        .start_sheet_with_settings("Sheet", None, value)
        .unwrap();
    writer
        .write_row(&Row {
            index: RowIndex::new(0).unwrap(),
            cells: vec![Cell {
                address: "A1".parse().unwrap(),
                value: CellValue::Integer(42),
                style: StyleId::new(0),
            }],
        })
        .unwrap();
    writer.finish(Cursor::new(Vec::new())).unwrap().into_inner()
}
fn part(bytes: &[u8], name: &str) -> String {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut out = String::new();
    zip.by_name(name).unwrap().read_to_string(&mut out).unwrap();
    out
}
fn replace(bytes: &[u8], name: &str, body: &str) -> Vec<u8> {
    let mut input = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let mut found = false;
    for index in 0..input.len() {
        let file = input.by_index(index).unwrap();
        if file.name() == name {
            found = true;
            output
                .start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            output.write_all(body.as_bytes()).unwrap();
        } else {
            output.raw_copy_file(file).unwrap();
        }
    }
    if !found {
        output
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        output.write_all(body.as_bytes()).unwrap();
    }
    output.finish().unwrap().into_inner()
}
#[test]
fn all_print_fields_sparse_breaks_and_load_defaults_round_trip() {
    let value = settings();
    let bytes = fixture(Some(&value));
    let mut expected = value;
    expected.column_breaks[0].manual = Some(true);
    let mut reader = WorkbookReader::new(Cursor::new(bytes)).unwrap();
    assert_eq!(reader.print_settings("Sheet").unwrap(), expected);
    assert_eq!(
        reader
            .rows("Sheet")
            .unwrap()
            .next_row()
            .unwrap()
            .unwrap()
            .cells[0]
            .value,
        CellValue::Integer(42)
    );
    assert_eq!(
        WorkbookReader::new(Cursor::new(fixture(None)))
            .unwrap()
            .print_settings("Sheet")
            .unwrap(),
        PrintSettings::default()
    );
}
#[test]
fn printing_and_views_share_one_owned_model_export() {
    let mut sheet = Worksheet::new("Sheet", EditLimits::default()).unwrap();
    sheet.set_print_settings(Some(settings())).unwrap();
    let mut views = SheetViews::default();
    views.views[0]
        .freeze_at(Some("B3".parse().unwrap()))
        .unwrap();
    sheet.set_sheet_views(Some(views)).unwrap();
    let mut writer = WorkbookWriter::new(WriteOptions::default()).unwrap();
    writer.write_worksheet(&sheet).unwrap();
    let bytes = writer.finish(Cursor::new(Vec::new())).unwrap().into_inner();
    let mut reader = WorkbookReader::new(Cursor::new(bytes)).unwrap();
    assert_eq!(
        reader.print_settings("Sheet").unwrap().setup.scale,
        Some(85)
    );
    assert_eq!(
        reader.sheet_views("Sheet").unwrap().views[0]
            .pane
            .as_ref()
            .unwrap()
            .top_left_cell
            .as_deref(),
        Some("B3")
    );
}
#[test]
fn repeated_printing_edits_preserve_other_properties_and_tail_order() {
    let bytes = fixture(None);
    let source=part(&bytes,"xl/worksheets/sheet1.xml")
        .replace("<sheetData>","<sheetPr codeName=\"Keep\"><tabColor rgb=\"FF123456\"/><pageSetUpPr fitToPage=\"0\"/></sheetPr><sheetData>")
        .replace("<v>42</v>","<f>20+22</f><v>42</v>")
        .replace("</worksheet>","<headerFooter><oddHeader>Keep header</oddHeader></headerFooter><ignoredErrors><ignoredError sqref=\"A1\" numberStoredAsText=\"1\"/></ignoredErrors></worksheet>");
    let bytes = replace(&bytes, "xl/worksheets/sheet1.xml", &source);
    let mut editor = WorkbookEditor::new(Cursor::new(&bytes)).unwrap();
    editor.set_print_settings("Sheet", settings()).unwrap();
    for _ in 0..2 {
        let (out, stats) = editor
            .save(Cursor::new(Vec::new()), SaveOptions::default())
            .unwrap();
        let out = out.into_inner();
        let xml = part(&out, "xl/worksheets/sheet1.xml");
        assert_eq!(stats.rewritten_parts, 1);
        assert!(xml.contains("codeName=\"Keep\""));
        assert!(xml.contains("FF123456"));
        assert!(xml.contains("Keep header"));
        assert!(xml.contains("<v>42</v>"));
        assert!(xml.find("pageSetup ").unwrap() < xml.find("headerFooter").unwrap());
        assert!(xml.find("headerFooter").unwrap() < xml.find("rowBreaks").unwrap());
        assert!(xml.find("colBreaks").unwrap() < xml.find("ignoredErrors").unwrap());
        assert_eq!(
            WorkbookReader::new(Cursor::new(out))
                .unwrap()
                .print_settings("Sheet")
                .unwrap()
                .setup
                .scale,
            Some(85)
        );
    }
    editor
        .upsert_value("Sheet", "B1".parse().unwrap(), CellValue::Integer(7))
        .unwrap();
    let out = editor
        .save(Cursor::new(Vec::new()), SaveOptions::default())
        .unwrap()
        .0
        .into_inner();
    assert!(!part(&out, "xl/worksheets/sheet1.xml").contains("<v>42</v>"));
    editor.clear_edits();
    assert!(!editor.is_dirty());
    assert_eq!(editor.patch_bytes(), 0);
}
#[test]
fn source_printer_identity_is_preserved_and_mutation_requires_graph_support() {
    let bytes = fixture(Some(&settings()));
    let xml=part(&bytes,"xl/worksheets/sheet1.xml").replace("<pageSetup ","<pageSetup xmlns:p=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" p:id=\"printer-source\" ");
    let bytes = replace(&bytes, "xl/worksheets/sheet1.xml", &xml);
    let bytes = replace(
        &bytes,
        "xl/worksheets/_rels/sheet1.xml.rels",
        "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"printer-source\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/printerSettings\" Target=\"../printerSettings/printerSettings1.bin\"/></Relationships>",
    );
    let bytes = replace(
        &bytes,
        "xl/printerSettings/printerSettings1.bin",
        "printer binary payload",
    );
    let mut editor = WorkbookEditor::new(Cursor::new(&bytes)).unwrap();
    let mut value = editor.print_settings("Sheet").unwrap();
    assert_eq!(
        value.setup.printer_relationship.as_deref(),
        Some("printer-source")
    );
    value.setup.scale = Some(65);
    editor.set_print_settings("Sheet", value.clone()).unwrap();
    value.setup.printer_relationship = Some("other".into());
    assert_eq!(
        editor
            .set_print_settings("Sheet", value)
            .unwrap_err()
            .kind(),
        ErrorKind::Unsupported
    );
    let out = editor
        .save(Cursor::new(Vec::new()), SaveOptions::default())
        .unwrap()
        .0
        .into_inner();
    assert_eq!(
        part(&out, "xl/printerSettings/printerSettings1.bin"),
        "printer binary payload"
    );
    assert_eq!(
        part(&out, "xl/worksheets/_rels/sheet1.xml.rels"),
        part(&bytes, "xl/worksheets/_rels/sheet1.xml.rels")
    );
    let loaded = WorkbookReader::new(Cursor::new(out))
        .unwrap()
        .print_settings("Sheet")
        .unwrap();
    assert_eq!(
        loaded.setup.printer_relationship.as_deref(),
        Some("printer-source")
    );
    assert_eq!(loaded.setup.scale, Some(65));
    let mut writer = WorkbookWriter::new(WriteOptions::default()).unwrap();
    assert_eq!(
        writer
            .start_sheet_with_settings("Sheet", None, Some(&loaded))
            .unwrap_err()
            .kind(),
        ErrorKind::Unsupported
    );
    assert_eq!(writer.temporary_bytes(), 0);
}
#[test]
fn nonfinite_xml_and_overlay_limits_fail_without_committing() {
    let directory = tempfile::tempdir().unwrap();
    let mut writer = WorkbookWriter::new(WriteOptions {
        temp_directory: Some(directory.path().into()),
        ..WriteOptions::default()
    })
    .unwrap();
    writer.start_sheet("First").unwrap();
    let before = writer.temporary_bytes();
    let mut bad = settings();
    bad.margins.as_mut().unwrap().top = f64::INFINITY;
    assert_eq!(
        writer
            .start_sheet_with_settings("Bad", None, Some(&bad))
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidData
    );
    assert_eq!(writer.temporary_bytes(), before);
    bad = settings();
    bad.setup.paper_height = Some(PaperDimension::parse("1in\0bad").unwrap());
    assert_eq!(
        writer
            .start_sheet_with_settings("Bad", None, Some(&bad))
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidData
    );
    assert_eq!(writer.temporary_bytes(), before);
    writer
        .start_sheet_with_settings("Valid", None, Some(&settings()))
        .unwrap();
    writer.finish(Cursor::new(Vec::new())).unwrap();
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    let mut editor = WorkbookEditor::with_options(
        Cursor::new(fixture(None)),
        EditorOptions {
            max_patch_bytes: 100,
            ..EditorOptions::default()
        },
    )
    .unwrap();
    assert!(editor.set_print_settings("Sheet", settings()).is_err());
    assert!(!editor.is_dirty());
}
#[test]
fn source_extensions_and_unknown_relationship_namespaces_reject_replacement() {
    let bytes = fixture(Some(&settings()));
    let source = part(&bytes, "xl/worksheets/sheet1.xml");
    for xml in [
        source.replace("<pageSetup ", "<pageSetup unknown=\"true\" "),
        source.replace(
            "<pageSetup ",
            "<pageSetup xmlns:p=\"urn:wrong\" p:id=\"printer\" ",
        ),
    ] {
        let mut editor = WorkbookEditor::new(Cursor::new(replace(
            &bytes,
            "xl/worksheets/sheet1.xml",
            &xml,
        )))
        .unwrap();
        assert!(editor.set_print_settings("Sheet", settings()).is_err());
        assert!(!editor.is_dirty());
    }
}

#[test]
fn footer_and_encoded_metadata_caps_reject_before_closing_previous_sheet() {
    for options in [
        WriteOptions {
            max_sheet_bytes: 256,
            ..WriteOptions::default()
        },
        WriteOptions {
            max_metadata_bytes: 16384,
            buffer_bytes: 1024,
            ..WriteOptions::default()
        },
    ] {
        let mut writer = WorkbookWriter::new(options).unwrap();
        writer.start_sheet("First").unwrap();
        let before = writer.temporary_bytes();
        let mut value = settings();
        value.row_breaks = vec![PageBreak::default(); 1000];
        assert_eq!(
            writer
                .start_sheet_with_settings("TooLarge", None, Some(&value))
                .unwrap_err()
                .kind(),
            ErrorKind::LimitExceeded
        );
        assert_eq!(writer.temporary_bytes(), before);
        writer
            .write_row(&Row::new(RowIndex::new(0).unwrap()))
            .unwrap();
        let out = writer.finish(Cursor::new(Vec::new())).unwrap().into_inner();
        assert_eq!(
            WorkbookReader::new(Cursor::new(out))
                .unwrap()
                .sheets()
                .len(),
            1
        );
    }
}

#[test]
fn nullable_break_coordinates_omit_and_acquire_public_load_defaults() {
    let mut value = PrintSettings::default();
    value.row_breaks.push(PageBreak {
        id: None,
        minimum: None,
        maximum: None,
        manual: None,
        pivot: None,
    });
    let bytes = fixture(Some(&value));
    assert!(part(&bytes, "xl/worksheets/sheet1.xml").contains("<brk/>"));
    assert_eq!(
        WorkbookReader::new(Cursor::new(bytes))
            .unwrap()
            .print_settings("Sheet")
            .unwrap()
            .row_breaks,
        vec![PageBreak::default()]
    );
}
#[test]
fn literal_none_page_tokens_use_public_descriptor_absence() {
    let bytes = fixture(None);
    let body=part(&bytes,"xl/worksheets/sheet1.xml").replace("</worksheet>","<pageSetup orientation=\"none\" pageOrder=\"none\" cellComments=\"none\" errors=\"none\"/></worksheet>");
    let value = WorkbookReader::new(Cursor::new(replace(
        &bytes,
        "xl/worksheets/sheet1.xml",
        &body,
    )))
    .unwrap()
    .print_settings("Sheet")
    .unwrap();
    assert_eq!(value.setup, crabxl_core::PageSetup::default());
}
#[test]
fn strict_printing_retains_namespace_context_and_other_source_properties() {
    let bytes = fixture(None);
    let source = "<s:worksheet xmlns:s=\"http://purl.oclc.org/ooxml/spreadsheetml/main\" xmlns:p=\"http://purl.oclc.org/ooxml/officeDocument/relationships\"><s:sheetPr codeName=\"Keep\"><s:pageSetUpPr autoPageBreaks=\"1\"/></s:sheetPr><s:sheetData/><s:pageSetup p:id=\"printer-source\" paperSize=\"9\"/><s:headerFooter><s:oddHeader>Keep header</s:oddHeader></s:headerFooter></s:worksheet>";
    let bytes = replace(&bytes, "xl/worksheets/sheet1.xml", source);
    let mut editor = WorkbookEditor::new(Cursor::new(bytes)).unwrap();
    let mut value = settings();
    value.setup.printer_relationship = Some("printer-source".into());
    editor.set_print_settings("Sheet", value).unwrap();
    let out = editor
        .save(Cursor::new(Vec::new()), SaveOptions::default())
        .unwrap()
        .0
        .into_inner();
    let xml = part(&out, "xl/worksheets/sheet1.xml");
    assert!(xml.contains("xmlns:r=\"http://purl.oclc.org/ooxml/officeDocument/relationships\""));
    assert!(xml.contains("codeName=\"Keep\""));
    assert!(xml.contains("Keep header"));
    let loaded = WorkbookReader::new(Cursor::new(out))
        .unwrap()
        .print_settings("Sheet")
        .unwrap();
    assert_eq!(loaded.setup.scale, Some(85));
    assert_eq!(
        loaded.setup.printer_relationship.as_deref(),
        Some("printer-source")
    );
    assert_eq!(loaded.auto_page_breaks, Some(false));
}

#[test]
fn validated_shrinking_replacements_do_not_reparse_source_under_live_overlays() {
    let mut original = PrintSettings::default();
    original.setup.paper_height =
        Some(PaperDimension::parse(&format!("1in{}", "x".repeat(400))).unwrap());
    let bytes = fixture(Some(&original));
    let mut large = PrintSettings::default();
    large.setup.paper_height =
        Some(PaperDimension::parse(&format!("1in{}", "x".repeat(1000))).unwrap());
    let mut probe = WorkbookEditor::new(Cursor::new(&bytes)).unwrap();
    probe.set_print_settings("Sheet", large.clone()).unwrap();
    let print_charge = probe.patch_bytes();
    probe
        .set_sheet_views("Sheet", SheetViews::default())
        .unwrap();
    let full_charge = probe.patch_bytes();
    let view_work = WorkbookReader::new(Cursor::new(&bytes))
        .unwrap()
        .sheet_views("Sheet")
        .unwrap()
        .memory_bytes();
    let cap = full_charge + view_work + 16;
    assert!(print_charge + original.memory_bytes() <= cap);
    assert!(full_charge + original.memory_bytes() > cap);
    let mut editor = WorkbookEditor::with_options(
        Cursor::new(&bytes),
        EditorOptions {
            memory_policy: crabxl_core::MemoryPolicy::Budget(
                probe.memory_allowance().working_reserve_bytes + cap,
            ),
            ..EditorOptions::default()
        },
    )
    .unwrap();
    editor.set_print_settings("Sheet", large).unwrap();
    editor
        .set_sheet_views("Sheet", SheetViews::default())
        .unwrap();
    let before = editor.patch_bytes();
    editor
        .set_print_settings("Sheet", PrintSettings::default())
        .unwrap();
    assert_eq!(editor.patch_bytes(), before - 1003);
    assert_eq!(
        editor.pending_print_settings("Sheet"),
        Some(&PrintSettings::default())
    );
    editor.clear_edits();
    editor
        .set_print_settings("Sheet", PrintSettings::default())
        .unwrap();
}

#[test]
fn printing_component_updates_reuse_source_validation_and_preserve_vector_ownership() {
    use crabxl_core::{PageSetup, PrintOptions, PrintSettingsChange};
    use std::{
        cell::Cell as Counter,
        io::{self, Seek, SeekFrom},
        rc::Rc,
    };
    struct Counted {
        inner: Cursor<Vec<u8>>,
        reads: Rc<Counter<usize>>,
    }
    impl Read for Counted {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let count = self.inner.read(buffer)?;
            self.reads.set(self.reads.get() + count);
            Ok(count)
        }
    }
    impl Seek for Counted {
        fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
            self.inner.seek(position)
        }
    }
    let reads = Rc::new(Counter::new(0));
    let source = settings();
    let mut editor = WorkbookEditor::new(Counted {
        inner: Cursor::new(fixture(Some(&source))),
        reads: reads.clone(),
    })
    .unwrap();
    editor
        .update_print_settings(
            "Sheet",
            PrintSettingsChange::Options(PrintOptions {
                headings: Some(false),
                ..source.options
            }),
        )
        .unwrap();
    let pointer = editor
        .pending_print_settings("Sheet")
        .unwrap()
        .row_breaks
        .as_ptr();
    let charge = editor.patch_bytes();
    reads.set(0);
    editor
        .update_print_settings(
            "Sheet",
            PrintSettingsChange::Margins(Some(PageMargins {
                left: 0.1,
                ..source.margins.unwrap()
            })),
        )
        .unwrap();
    editor
        .update_print_settings(
            "Sheet",
            PrintSettingsChange::Properties {
                auto_page_breaks: Some(true),
                fit_to_page: Some(false),
            },
        )
        .unwrap();
    assert_eq!(reads.get(), 0);
    assert_eq!(editor.patch_bytes(), charge);
    assert_eq!(
        editor
            .pending_print_settings("Sheet")
            .unwrap()
            .row_breaks
            .as_ptr(),
        pointer
    );
    let before = editor.pending_print_settings("Sheet").unwrap().clone();
    let error = editor
        .update_print_settings(
            "Sheet",
            PrintSettingsChange::Setup(PageSetup {
                printer_relationship: Some("new-printer".into()),
                ..PageSetup::default()
            }),
        )
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Unsupported);
    assert!(
        editor
            .update_print_settings(
                "Sheet",
                PrintSettingsChange::Margins(Some(PageMargins {
                    left: f64::NAN,
                    ..PageMargins::default()
                }))
            )
            .is_err()
    );
    assert_eq!(editor.pending_print_settings("Sheet"), Some(&before));
    assert_eq!(editor.patch_bytes(), charge);
    assert_eq!(reads.get(), 0);
    editor.set_print_settings("Sheet", before.clone()).unwrap();
    assert_eq!(reads.get(), 0);
    for _ in 0..2 {
        let out = editor
            .save(Cursor::new(Vec::new()), SaveOptions::default())
            .unwrap()
            .0
            .into_inner();
        let loaded = WorkbookReader::new(Cursor::new(out))
            .unwrap()
            .print_settings("Sheet")
            .unwrap();
        assert_eq!(loaded, before);
    }
    editor.clear_edits();
    reads.set(0);
    editor
        .update_print_settings("Sheet", PrintSettingsChange::ColumnBreaks(Vec::new()))
        .unwrap();
    assert!(reads.get() > 0);
    assert!(
        editor
            .pending_print_settings("Sheet")
            .unwrap()
            .column_breaks
            .is_empty()
    );
}

#[test]
fn component_update_failures_preserve_patch_budget_and_validate_xml_before_source_reads() {
    use crabxl_core::{PageSetup, PrintOptions, PrintSettingsChange};
    let source = fixture(Some(&settings()));
    let mut editor = WorkbookEditor::with_options(
        Cursor::new(source),
        EditorOptions {
            max_patch_bytes: 2500,
            ..EditorOptions::default()
        },
    )
    .unwrap();
    editor
        .update_print_settings(
            "Sheet",
            PrintSettingsChange::Options(PrintOptions::default()),
        )
        .unwrap();
    let before = editor.pending_print_settings("Sheet").unwrap().clone();
    let charge = editor.patch_bytes();
    assert_eq!(
        editor
            .update_print_settings(
                "Sheet",
                PrintSettingsChange::RowBreaks(vec![PageBreak::default(); 100])
            )
            .unwrap_err()
            .kind(),
        ErrorKind::MemoryBudgetExceeded
    );
    assert_eq!(
        editor
            .update_print_settings(
                "Sheet",
                PrintSettingsChange::Setup(PageSetup {
                    paper_width: Some(PaperDimension::parse("1in\0").unwrap()),
                    ..PageSetup::default()
                })
            )
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidData
    );
    assert_eq!(editor.pending_print_settings("Sheet"), Some(&before));
    assert_eq!(editor.patch_bytes(), charge);
    editor.clear_edits();
    assert!(
        editor
            .update_print_settings("Missing", PrintSettingsChange::RowBreaks(Vec::new()))
            .is_err()
    );
    assert!(!editor.is_dirty());
}
