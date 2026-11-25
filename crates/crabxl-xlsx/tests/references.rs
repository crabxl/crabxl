//! Row and column edits update the references to moved cells.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use crabxl_core::features::{ConditionalFormatting, DefinedName};
use crabxl_core::{
    Cell, CellAddress, CellRange, CellValue, ColumnIndex, ErrorKind, Formula, Row, RowDimension,
    RowIndex, StyleId,
};
use crabxl_xlsx::{LoadedWorkbook, ReferenceUpdate, WorkbookWriter, WriteOptions};
use std::io::Cursor;

fn address(text: &str) -> CellAddress {
    text.parse().unwrap()
}
fn formula(text: &str) -> CellValue {
    CellValue::Formula(Box::new(Formula::new(text, None).unwrap()))
}
fn cell(text: &str, value: CellValue) -> Cell {
    Cell {
        address: address(text),
        value,
        style: StyleId::new(0),
    }
}
fn source() -> Vec<u8> {
    let mut writer = WorkbookWriter::new(WriteOptions::default()).unwrap();
    writer.start_sheet("Data").unwrap();
    for (index, cells) in [
        vec![
            cell("A1", CellValue::Integer(1)),
            cell("D1", formula("=A4*2")),
        ],
        vec![cell("A2", CellValue::Integer(2))],
        vec![cell("A3", CellValue::Integer(3))],
        vec![cell("A4", formula("=SUM(A1:A3)"))],
    ]
    .into_iter()
    .enumerate()
    {
        writer
            .write_row(&Row {
                index: RowIndex::new(index as u32).unwrap(),
                cells,
            })
            .unwrap();
    }
    writer.start_sheet("Summary").unwrap();
    writer
        .write_row(&Row {
            index: RowIndex::new(0).unwrap(),
            cells: vec![
                cell("A1", formula("=SUM(Data!A1:A3)+Data!A4")),
                cell("B1", formula("=A1")),
            ],
        })
        .unwrap();
    writer.finish(Cursor::new(Vec::new())).unwrap().into_inner()
}
fn expression(book: &mut LoadedWorkbook<Cursor<Vec<u8>>>, sheet: &str, at: &str) -> String {
    let id = book.sheet_id(sheet).unwrap();
    match &book.sheet(id).unwrap().get(address(at)).unwrap().value {
        CellValue::Formula(formula) => formula.expression().to_owned(),
        other => panic!("{sheet}!{at} holds {other:?}"),
    }
}
fn prepared() -> LoadedWorkbook<Cursor<Vec<u8>>> {
    let mut book = LoadedWorkbook::with_options(Cursor::new(source()), Default::default()).unwrap();
    let data = book.sheet_id("Data").unwrap();
    book.merge_cells(data, "B1:C2".parse().unwrap()).unwrap();
    let mut height = RowDimension::new(RowIndex::new(2).unwrap());
    height.height = Some(30.0);
    book.set_row_dimension(data, height).unwrap();
    let mut features = book.sheet_features(data).unwrap().clone();
    features.conditional_formats.push(ConditionalFormatting {
        ranges: vec!["A2:A4".parse().unwrap()],
        ..Default::default()
    });
    book.set_sheet_features(data, features).unwrap();
    let mut workbook = book.workbook_features().unwrap().clone();
    workbook.defined_names.push(DefinedName {
        name: "Total".into(),
        value: "Data!$A$4".into(),
        ..Default::default()
    });
    book.set_workbook_features(workbook).unwrap();
    book
}

#[test]
fn inserting_rows_updates_formulas_names_merges_rules_and_heights() {
    let mut book = prepared();
    let data = book.sheet_id("Data").unwrap();
    book.insert_rows(data, RowIndex::new(1).unwrap(), 2)
        .unwrap();

    assert_eq!(expression(&mut book, "Data", "A6"), "SUM(A1:A5)");
    assert_eq!(expression(&mut book, "Data", "D1"), "A6*2");
    assert_eq!(
        expression(&mut book, "Summary", "A1"),
        "SUM(Data!A1:A5)+Data!A6"
    );
    assert_eq!(expression(&mut book, "Summary", "B1"), "A1");
    let model = book.sheet(data).unwrap();
    let merges: Vec<CellRange> = model
        .merged_ranges()
        .ranges()
        .iter()
        .map(|m| m.range())
        .collect();
    assert_eq!(merges, ["B1:C4".parse().unwrap()]);
    assert_eq!(model.dimensions().rows()[0].index.get(), 4);
    assert_eq!(
        book.sheet_features(data).unwrap().conditional_formats[0].ranges[0].to_string(),
        "A4:A6"
    );
    let names = &book.workbook_features().unwrap().defined_names;
    assert_eq!(
        &*names.iter().find(|n| &*n.name == "Total").unwrap().value,
        "Data!$A$6"
    );

    // The updates survive a save and a reload.
    let (output, _) = book
        .save(Cursor::new(Vec::new()), Default::default())
        .unwrap();
    let mut again = LoadedWorkbook::with_options(output, Default::default()).unwrap();
    assert_eq!(
        expression(&mut again, "Summary", "A1"),
        "SUM(Data!A1:A5)+Data!A6"
    );
    assert_eq!(expression(&mut again, "Data", "A6"), "SUM(A1:A5)");
}

#[test]
fn deleting_rows_and_columns_shrinks_or_invalidates_references() {
    let mut book = prepared();
    let data = book.sheet_id("Data").unwrap();
    book.delete_rows(data, RowIndex::new(0).unwrap(), 1)
        .unwrap();
    assert_eq!(expression(&mut book, "Data", "A3"), "SUM(A1:A2)");
    assert_eq!(
        expression(&mut book, "Summary", "A1"),
        "SUM(Data!A1:A2)+Data!A3"
    );
    book.delete_rows(data, RowIndex::new(2).unwrap(), 1)
        .unwrap();
    // The SUM cell itself is gone; references to it become #REF!.
    assert_eq!(
        expression(&mut book, "Summary", "A1"),
        "SUM(Data!A1:A2)+Data!#REF!"
    );
    let names = &book.workbook_features().unwrap().defined_names;
    assert_eq!(
        &*names.iter().find(|n| &*n.name == "Total").unwrap().value,
        "Data!#REF!"
    );

    let mut book = prepared();
    let data = book.sheet_id("Data").unwrap();
    book.insert_columns(data, ColumnIndex::new(0).unwrap(), 1)
        .unwrap();
    assert_eq!(expression(&mut book, "Data", "B4"), "SUM(B1:B3)");
    assert_eq!(
        expression(&mut book, "Summary", "A1"),
        "SUM(Data!B1:B3)+Data!B4"
    );
}

#[test]
fn cells_only_edits_keep_reference_text_and_unsupported_content_rejects_first() {
    let mut book = prepared();
    let data = book.sheet_id("Data").unwrap();
    book.insert_rows_with(
        data,
        RowIndex::new(0).unwrap(),
        1,
        ReferenceUpdate::CellsOnly,
    )
    .unwrap();
    assert_eq!(expression(&mut book, "Data", "A5"), "SUM(A1:A3)");

    let mut book = prepared();
    let summary = book.sheet_id("Summary").unwrap();
    book.set_value(summary, address("C1"), formula("=SUM(Data:Summary!A1)"))
        .unwrap();
    let data = book.sheet_id("Data").unwrap();
    let error = book
        .insert_rows(data, RowIndex::new(0).unwrap(), 1)
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Unsupported);
    // Nothing moved.
    assert_eq!(expression(&mut book, "Data", "A4"), "SUM(A1:A3)");
}

#[test]
fn internal_hyperlinks_follow_their_target_cells() {
    let mut book = prepared();
    let summary = book.sheet_id("Summary").unwrap();
    let link = crabxl_core::Hyperlink {
        location: Some("Data!A4".into()),
        ..Default::default()
    };
    book.set_hyperlink(summary, address("C1"), Some(link))
        .unwrap();
    let data = book.sheet_id("Data").unwrap();
    book.insert_rows(data, RowIndex::new(0).unwrap(), 3)
        .unwrap();
    let location = book
        .hyperlinks(summary)
        .unwrap()
        .get(address("C1"))
        .unwrap()
        .location
        .clone();
    assert_eq!(location.as_deref(), Some("Data!A7"));
}
