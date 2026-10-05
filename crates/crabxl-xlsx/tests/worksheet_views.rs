//! Generated worksheet viewport fixtures; public openpyxl behavior is independently probed.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use crabxl_core::{
    Cell, CellAddress, CellValue, EditLimits, ErrorKind, Pane, PanePosition, PaneState,
    ResourceLimits, Row, RowIndex, Selection, SheetView, SheetViews, StyleId, ViewMode, Worksheet,
};
use crabxl_xlsx::{
    EditorOptions, SaveOptions, WorkbookEditor, WorkbookReader, WorkbookWriter, WriteOptions,
};
use std::io::{Cursor, Read, Write};

fn fixture(views: Option<&SheetViews>) -> Vec<u8> {
    let mut writer = WorkbookWriter::new(WriteOptions::default()).unwrap();
    match views {
        Some(views) => writer.start_sheet_with_views("Sheet", views).unwrap(),
        None => writer.start_sheet("Sheet").unwrap(),
    }
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
fn replace_part(bytes: &[u8], part: &str, body: &str) -> Vec<u8> {
    let mut input = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let mut found = false;
    for index in 0..input.len() {
        let file = input.by_index(index).unwrap();
        if file.name() == part {
            found = true;
            output
                .start_file(part, zip::write::SimpleFileOptions::default())
                .unwrap();
            output.write_all(body.as_bytes()).unwrap();
        } else {
            output.raw_copy_file(file).unwrap();
        }
    }
    if !found {
        output
            .start_file(part, zip::write::SimpleFileOptions::default())
            .unwrap();
        output.write_all(body.as_bytes()).unwrap();
    }
    output.finish().unwrap().into_inner()
}
fn part(bytes: &[u8], name: &str) -> String {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut text = String::new();
    archive
        .by_name(name)
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    text
}
fn custom() -> SheetViews {
    let mut views = SheetViews::default();
    let view = &mut views.views[0];
    view.freeze_at(Some("B3".parse().unwrap())).unwrap();
    view.show_grid_lines = Some(false);
    view.right_to_left = Some(true);
    view.zoom_scale = Some(145);
    view.zoom_scale_normal = Some(-3);
    view.mode = Some(ViewMode::PageLayout);
    view.top_left_cell = Some("B3 & literal".into());
    view.show_white_space = Some(false);
    view.zoom_to_fit = Some(true);
    view.selections[2].active_cell_id = Some(2);
    view.selections[2].ranges = Some("A1 C3:D4".into());
    views.views.push(SheetView {
        workbook_view_id: 2,
        pane: Some(Box::new(Pane {
            x_split: Some(-1.5),
            y_split: Some(2.5),
            top_left_cell: Some("opaque".into()),
            active_pane: PanePosition::BottomRight,
            state: PaneState::FrozenSplit,
        })),
        ..SheetView::default()
    });
    views
}
#[test]
fn explicit_views_round_trip_multiple_panes_and_source_literals() {
    let views = custom();
    let bytes = fixture(Some(&views));
    let mut expected = views;
    // Missing source selection attributes acquire public descriptor defaults on load.
    for selection in &mut expected.views[0].selections[..2] {
        selection.active_cell = Some("A1".into());
        selection.ranges = Some("A1".into());
    }
    let mut reader = WorkbookReader::new(Cursor::new(&bytes)).unwrap();
    assert_eq!(reader.sheet_views("Sheet").unwrap(), expected);
    assert!(part(&bytes, "xl/worksheets/sheet1.xml").contains("B3 &amp; literal"));
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
}
#[test]
fn missing_empty_and_empty_selections_use_reference_load_defaults() {
    for views in [
        None,
        Some(SheetViews { views: Vec::new() }),
        Some(SheetViews {
            views: vec![SheetView {
                selections: Vec::new(),
                ..SheetView::default()
            }],
        }),
    ] {
        let mut reader = WorkbookReader::new(Cursor::new(fixture(views.as_ref()))).unwrap();
        assert_eq!(reader.sheet_views("Sheet").unwrap(), SheetViews::default());
    }
}
#[test]
fn owned_view_charges_survive_structural_recounts_and_failed_replacements() {
    let views = custom();
    let charge = views.clone().memory_bytes();
    let mut sheet = Worksheet::new(
        "Sheet",
        EditLimits {
            max_bytes: 5 + charge,
            max_cells: 10,
        },
    )
    .unwrap();
    sheet.set_sheet_views(Some(views.clone())).unwrap();
    assert_eq!(sheet.charged_bytes(), 5 + charge);
    sheet.insert_rows(RowIndex::new(0).unwrap(), 1).unwrap();
    assert_eq!(sheet.charged_bytes(), 5 + charge);
    let mut large = views.clone();
    large.views[0].top_left_cell = Some("a".repeat(charge).into());
    assert_eq!(
        sheet.set_sheet_views(Some(large)).unwrap_err().kind(),
        ErrorKind::MemoryBudgetExceeded
    );
    assert_eq!(sheet.sheet_views(), Some(&views));
    sheet.set_sheet_views(None).unwrap();
    assert_eq!(sheet.charged_bytes(), 5);
}
#[test]
fn invalid_view_validation_leaves_active_writer_and_spools_unchanged() {
    let directory = tempfile::tempdir().unwrap();
    let mut writer = WorkbookWriter::new(WriteOptions {
        temp_directory: Some(directory.path().into()),
        ..WriteOptions::default()
    })
    .unwrap();
    writer.start_sheet("Sheet").unwrap();
    let before = writer.temporary_bytes();
    let mut views = SheetViews::default();
    views.views[0].top_left_cell = Some("bad\0cell".into());
    assert_eq!(
        writer
            .start_sheet_with_views("Bad", &views)
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidData
    );
    assert_eq!(writer.temporary_bytes(), before);
    views.views[0].top_left_cell = None;
    views.views[0].pane = Some(Box::new(Pane {
        x_split: Some(f64::INFINITY),
        ..Pane::default()
    }));
    assert_eq!(
        writer
            .start_sheet_with_views("Bad", &views)
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidData
    );
    assert_eq!(writer.temporary_bytes(), before);
    writer
        .start_sheet_with_views("Valid", &SheetViews::default())
        .unwrap();
    writer.finish(Cursor::new(Vec::new())).unwrap();
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}
#[test]
fn editing_views_preserves_formula_caches_repeat_saves_and_other_parts() {
    let bytes = fixture(None);
    let body =
        part(&bytes, "xl/worksheets/sheet1.xml").replace("<v>42</v>", "<f>20+22</f><v>42</v>");
    let bytes = replace_part(&bytes, "xl/worksheets/sheet1.xml", &body);
    let mut editor =
        WorkbookEditor::with_options(Cursor::new(&bytes), EditorOptions::default()).unwrap();
    editor.set_sheet_views("Sheet", custom()).unwrap();
    assert!(editor.is_dirty());
    for _ in 0..2 {
        let (saved, stats) = editor
            .save(Cursor::new(Vec::new()), SaveOptions::default())
            .unwrap();
        let saved = saved.into_inner();
        assert_eq!(stats.rewritten_parts, 1);
        assert!(part(&saved, "xl/worksheets/sheet1.xml").contains("<v>42</v>"));
        assert_eq!(
            part(&saved, "xl/workbook.xml"),
            part(&bytes, "xl/workbook.xml")
        );
        assert_eq!(
            WorkbookReader::new(Cursor::new(saved))
                .unwrap()
                .sheet_views("Sheet")
                .unwrap()
                .views
                .len(),
            2
        );
    }
    editor
        .upsert_value(
            "Sheet",
            "B1".parse::<CellAddress>().unwrap(),
            CellValue::Integer(7),
        )
        .unwrap();
    let saved = editor
        .save(Cursor::new(Vec::new()), SaveOptions::default())
        .unwrap()
        .0
        .into_inner();
    let body = part(&saved, "xl/worksheets/sheet1.xml");
    assert!(!body.contains("<v>42</v>"));
    assert!(body.contains("<v>7</v>"));
    assert!(body.contains("zoomScale=\"145\""));
    editor.clear_edits();
    assert!(!editor.is_dirty());
    assert_eq!(editor.patch_bytes(), 0);
}
#[test]
fn strict_prefixed_namespaces_and_unknown_extension_guard() {
    let bytes = fixture(None);
    let body = "<s:worksheet xmlns:s=\"http://purl.oclc.org/ooxml/spreadsheetml/main\"><s:sheetViews><s:sheetView workbookViewId=\"0\"><s:selection sqref=\"A1 B2\"/></s:sheetView></s:sheetViews><s:sheetData/></s:worksheet>";
    let strict = replace_part(&bytes, "xl/worksheets/sheet1.xml", body);
    let mut editor =
        WorkbookEditor::with_options(Cursor::new(strict), EditorOptions::default()).unwrap();
    assert_eq!(
        editor.sheet_views("Sheet").unwrap().views[0].selections[0]
            .ranges
            .as_deref(),
        Some("A1 B2")
    );
    editor.set_sheet_views("Sheet", custom()).unwrap();
    let saved = editor
        .save(Cursor::new(Vec::new()), SaveOptions::default())
        .unwrap()
        .0
        .into_inner();
    assert!(
        part(&saved, "xl/worksheets/sheet1.xml")
            .contains("<sheetViews xmlns=\"http://purl.oclc.org/ooxml/spreadsheetml/main\"")
    );
    assert_eq!(
        WorkbookReader::new(Cursor::new(saved))
            .unwrap()
            .sheet_views("Sheet")
            .unwrap()
            .views
            .len(),
        2
    );
    let unknown = body.replace("<s:selection", "<s:extLst/><s:selection");
    let mut editor = WorkbookEditor::with_options(
        Cursor::new(replace_part(&bytes, "xl/worksheets/sheet1.xml", &unknown)),
        EditorOptions::default(),
    )
    .unwrap();
    assert_eq!(
        editor
            .set_sheet_views("Sheet", custom())
            .unwrap_err()
            .kind(),
        ErrorKind::Unsupported
    );
    assert!(!editor.is_dirty());
}
#[test]
fn metadata_limits_bound_retained_selections_independently_of_sheet_size() {
    let bytes = fixture(Some(&custom()));
    let limits = ResourceLimits {
        max_metadata_bytes: 256,
        ..ResourceLimits::default()
    };
    // Package discovery also has its own metadata limit; lower only after construction
    // is not public. A sufficiently sized catalog limit and huge views isolates growth.
    let mut huge = SheetViews::default();
    huge.views[0].selections = vec![Selection::default(); 1000];
    let huge_bytes = fixture(Some(&huge));
    let mut reader = WorkbookReader::with_limits(
        Cursor::new(huge_bytes),
        ResourceLimits {
            max_metadata_bytes: 8192,
            ..limits
        },
    )
    .unwrap();
    assert!(reader.sheet_views("Sheet").is_err());
    let mut editor = WorkbookEditor::with_options(
        Cursor::new(bytes),
        EditorOptions {
            max_patch_bytes: 100,
            ..EditorOptions::default()
        },
    )
    .unwrap();
    assert!(editor.set_sheet_views("Sheet", custom()).is_err());
    assert!(!editor.is_dirty());
}

#[test]
fn pure_view_edits_retain_chain_even_under_value_rejection_policy() {
    let mut bytes = fixture(None);
    let types = part(&bytes, "[Content_Types].xml").replace("</Types>", "<Override PartName=\"/xl/calcChain.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.calcChain+xml\"/></Types>");
    bytes = replace_part(&bytes, "[Content_Types].xml", &types);
    let rels = part(&bytes, "xl/_rels/workbook.xml.rels").replace("</Relationships>", "<Relationship Id=\"chain\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/calcChain\" Target=\"calcChain.xml\"/></Relationships>");
    bytes = replace_part(&bytes, "xl/_rels/workbook.xml.rels", &rels);
    let chain = "<calcChain xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><c r=\"A1\" i=\"1\"/></calcChain>";
    bytes = replace_part(&bytes, "xl/calcChain.xml", chain);
    let mut editor = WorkbookEditor::with_options(
        Cursor::new(&bytes),
        EditorOptions {
            calculation_chain: crabxl_xlsx::CalculationChainPolicy::RejectEdits,
            ..EditorOptions::default()
        },
    )
    .unwrap();
    editor.set_sheet_views("Sheet", custom()).unwrap();
    assert!(
        editor
            .set_value("Sheet", "A1".parse().unwrap(), CellValue::Integer(7))
            .is_err()
    );
    let (saved, stats) = editor
        .save(Cursor::new(Vec::new()), SaveOptions::default())
        .unwrap();
    let saved = saved.into_inner();
    assert_eq!(stats.removed_parts, 0);
    assert_eq!(stats.rewritten_parts, 1);
    assert_eq!(part(&saved, "xl/calcChain.xml"), chain);
    assert_eq!(part(&saved, "xl/_rels/workbook.xml.rels"), rels);
    assert_eq!(part(&saved, "[Content_Types].xml"), types);
}
