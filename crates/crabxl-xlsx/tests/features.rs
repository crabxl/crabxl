//! Read, write and edit worksheet and workbook features against an openpyxl fixture.
#![allow(clippy::unwrap_used)]
use crabxl_core::{Workbook, WorkbookLimits};
use crabxl_xlsx::{WorkbookReader, WorkbookWriter, WriteOptions};
use std::io::Cursor;

const FIXTURE: &[u8] = include_bytes!("fixtures/features.xlsx");

fn reader(bytes: Vec<u8>) -> WorkbookReader<Cursor<Vec<u8>>> {
    WorkbookReader::new(Cursor::new(bytes)).unwrap()
}

#[test]
fn reads_openpyxl_features() {
    let mut book = reader(FIXTURE.to_vec());
    let data = book.sheet_features("Data").unwrap();
    assert_eq!(data.tables.len(), 1);
    assert_eq!(data.tables[0].display_name.as_ref(), "Items");
    assert_eq!(data.conditional_formats.len(), 3);
    assert_eq!(data.data_validations.validations.len(), 2);
    assert_eq!(data.comments.len(), 2);
    assert!(data.protection.as_ref().unwrap().sheet);
    assert!(data.header_footer.is_some());
    let filter = book.sheet_features("Filter").unwrap();
    assert!(filter.auto_filter.is_some());
    let workbook = book.workbook_features().unwrap();
    assert!(workbook.defined_names.len() >= 3);
    assert!(workbook.protection.is_some());
    assert_eq!(workbook.properties.title.as_deref(), Some("Features"));
    assert_eq!(workbook.properties.custom.len(), 3);
}

#[test]
fn writes_features_that_read_back_equal() {
    let mut source = reader(FIXTURE.to_vec());
    let data = source.sheet_features("Data").unwrap();
    let filter = source.sheet_features("Filter").unwrap();
    let features = source.workbook_features().unwrap();

    let mut model = Workbook::new(WorkbookLimits::default()).unwrap();
    let first = model.create_sheet("Data").unwrap();
    let second = model.create_sheet("Filter").unwrap();
    model
        .sheet_mut(first)
        .unwrap()
        .set_features(Some(data.clone()))
        .unwrap();
    model
        .sheet_mut(second)
        .unwrap()
        .set_features(Some(filter.clone()))
        .unwrap();
    model.set_features(Some(features.clone()));

    // Conditional formats refer to the source differential styles.
    let catalog = reader(FIXTURE.to_vec())
        .into_style_catalog()
        .unwrap()
        .unwrap();
    let mut writer = WorkbookWriter::from_style_catalog(WriteOptions::default(), catalog).unwrap();
    writer.write_workbook(&model).unwrap();
    let bytes = writer.finish(Cursor::new(Vec::new())).unwrap().into_inner();
    std::fs::write(
        std::env::temp_dir().join("crabxl-features-written.xlsx"),
        &bytes,
    )
    .unwrap();

    let mut written = reader(bytes);
    assert_eq!(written.sheet_features("Data").unwrap(), data);
    assert_eq!(written.sheet_features("Filter").unwrap(), filter);
    assert_eq!(written.workbook_features().unwrap(), features);
}

#[test]
fn editor_replaces_features_and_parts() {
    use crabxl_core::features::Comment;
    use crabxl_xlsx::{SaveOptions, WorkbookEditor};
    let mut editor = WorkbookEditor::new(Cursor::new(FIXTURE.to_vec())).unwrap();
    let mut data = editor.sheet_features("Data").unwrap();
    data.tables[0].display_name = "Renamed".into();
    data.conditional_formats.truncate(1);
    data.data_validations.validations.truncate(1);
    data.comments
        .push(Comment::new("C5".parse().unwrap(), "Added note", "Carol"));
    data.header_footer = None;
    editor.set_sheet_features("Data", data.clone()).unwrap();
    let mut filter = editor.sheet_features("Filter").unwrap();
    filter.auto_filter = None;
    filter
        .comments
        .push(Comment::new("A1".parse().unwrap(), "New sheet note", "Dan"));
    editor.set_sheet_features("Filter", filter.clone()).unwrap();
    let mut workbook = editor.workbook_features().unwrap();
    workbook
        .defined_names
        .retain(|name| name.name.as_ref() != "Total");
    workbook.protection = None;
    workbook.properties.title = Some("Edited".into());
    editor.set_workbook_features(workbook.clone());
    let (bytes, _) = editor
        .save(Cursor::new(Vec::new()), SaveOptions::default())
        .map(|(output, stats)| (output.into_inner(), stats))
        .unwrap();
    std::fs::write(
        std::env::temp_dir().join("crabxl-features-edited.xlsx"),
        &bytes,
    )
    .unwrap();
    let mut edited = reader(bytes);
    let mut read = edited.sheet_features("Data").unwrap();
    // New table parts get fresh workbook-unique ids.
    assert!(read.tables[0].id > 1);
    read.tables[0].id = data.tables[0].id;
    assert_eq!(read, data);
    assert_eq!(edited.sheet_features("Filter").unwrap(), filter);
    assert_eq!(edited.workbook_features().unwrap(), workbook);
}

#[test]
fn editor_keeps_unchanged_features() {
    use crabxl_xlsx::{SaveOptions, WorkbookEditor};
    let mut editor = WorkbookEditor::new(Cursor::new(FIXTURE.to_vec())).unwrap();
    let mut data = editor.sheet_features("Data").unwrap();
    let source = data.clone();
    data.protection = None;
    editor.set_sheet_features("Data", data.clone()).unwrap();
    let bytes = editor
        .save(Cursor::new(Vec::new()), SaveOptions::default())
        .unwrap()
        .0
        .into_inner();
    let mut edited = reader(bytes);
    let read = edited.sheet_features("Data").unwrap();
    assert_eq!(read, data);
    assert_eq!(read.tables, source.tables);
}

#[test]
fn loaded_workbook_edits_features_without_materializing_cells() {
    use crabxl_xlsx::{LoadOptions, LoadedWorkbook, SaveOptions};
    let mut loaded =
        LoadedWorkbook::with_options(Cursor::new(FIXTURE.to_vec()), LoadOptions::default())
            .unwrap();
    let id = loaded.sheet_id("Data").unwrap();
    let mut data = loaded.sheet_features(id).unwrap().clone();
    assert!(!loaded.is_materialized(id));
    data.data_validations.validations.clear();
    data.comments.clear();
    loaded.set_sheet_features(id, data.clone()).unwrap();
    let mut workbook = loaded.workbook_features().unwrap().clone();
    workbook.properties.creator = Some("Loaded".into());
    loaded.set_workbook_features(workbook.clone()).unwrap();
    let bytes = loaded
        .save(Cursor::new(Vec::new()), SaveOptions::default())
        .unwrap()
        .0
        .into_inner();
    let mut saved = reader(bytes);
    assert_eq!(saved.sheet_features("Data").unwrap(), data);
    assert_eq!(saved.workbook_features().unwrap(), workbook);
}

#[test]
fn loaded_workbook_registers_differential_styles() {
    use crabxl_xlsx::{LoadOptions, LoadedWorkbook, SaveOptions, feature_xml};
    let mut loaded =
        LoadedWorkbook::with_options(Cursor::new(FIXTURE.to_vec()), LoadOptions::default())
            .unwrap();
    let before = loaded.differential_styles().len();
    let style = feature_xml::differential_style_from_xml(b"<dxf><font><i val=\"1\"/></font></dxf>")
        .unwrap();
    let id = loaded.register_differential_style(style.clone()).unwrap();
    assert_eq!(id as usize, before);
    assert_eq!(loaded.register_differential_style(style).unwrap(), id);
    let bytes = loaded
        .save(Cursor::new(Vec::new()), SaveOptions::default())
        .unwrap()
        .0
        .into_inner();
    let catalog = reader(bytes).into_style_catalog().unwrap().unwrap();
    assert_eq!(catalog.differential_styles.len(), before + 1);
}

#[test]
fn loaded_workbook_writes_features_on_created_sheets() {
    use crabxl_core::features::Comment;
    use crabxl_xlsx::{LoadOptions, LoadedWorkbook, SaveOptions};
    let mut loaded =
        LoadedWorkbook::with_options(Cursor::new(FIXTURE.to_vec()), LoadOptions::default())
            .unwrap();
    let source = loaded
        .sheet_features(loaded.sheet_id("Data").unwrap())
        .unwrap()
        .clone();
    let id = loaded.create_sheet("Created").unwrap();
    let mut features = source.clone();
    features.tables[0].display_name = "CreatedItems".into();
    features.tables[0].name = Some("CreatedItems".into());
    features.comments = vec![Comment::new("B2".parse().unwrap(), "On new sheet", "Eve")];
    loaded.set_sheet_features(id, features.clone()).unwrap();
    let bytes = loaded
        .save(Cursor::new(Vec::new()), SaveOptions::default())
        .unwrap()
        .0
        .into_inner();
    std::fs::write(
        std::env::temp_dir().join("crabxl-features-created.xlsx"),
        &bytes,
    )
    .unwrap();
    let mut saved = reader(bytes);
    let mut read = saved.sheet_features("Created").unwrap();
    read.tables[0].id = features.tables[0].id;
    assert_eq!(read, features);
    assert_eq!(saved.sheet_features("Data").unwrap(), source);
}

#[test]
fn loaded_workbook_keeps_local_names_on_their_sheets() {
    use crabxl_xlsx::{LoadOptions, LoadedWorkbook, SaveOptions};
    let mut loaded =
        LoadedWorkbook::with_options(Cursor::new(FIXTURE.to_vec()), LoadOptions::default())
            .unwrap();
    let filter = loaded.sheet_id("Filter").unwrap();
    loaded.move_sheet(filter, 0).unwrap();
    for _ in 0..2 {
        let bytes = loaded
            .save(Cursor::new(Vec::new()), SaveOptions::default())
            .unwrap()
            .0
            .into_inner();
        let mut saved = reader(bytes);
        assert_eq!(saved.sheets()[0].name(), "Filter");
        let names = saved.workbook_features().unwrap().defined_names;
        let local = names
            .iter()
            .find(|name| name.name.as_ref() == "LocalName")
            .unwrap();
        assert_eq!(local.local_sheet_id, Some(0));
        let area = names
            .iter()
            .find(|name| name.name.as_ref() == "_xlnm.Print_Area")
            .unwrap();
        assert_eq!(area.local_sheet_id, Some(1));
    }
}
