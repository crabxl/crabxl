//! Formula calculation before saving a loaded workbook.
#![cfg(feature = "calc")]
#![allow(clippy::unwrap_used)]
use crabxl_core::{
    Cell, CellAddress, CellValue, Formula, ReadOptions, Row, RowDimension, RowIndex, StyleId,
};
use crabxl_xlsx::{LoadOptions, LoadedWorkbook, SaveOptions, WorkbookReader, WorkbookWriter};
use std::{collections::HashMap, io::Cursor};

fn at(address: &str) -> CellAddress {
    address.parse().unwrap()
}

fn cell(address: &str, value: CellValue) -> Cell {
    Cell {
        address: at(address),
        value,
        style: StyleId::new(0),
    }
}

/// A formula with a wrong cached value, as an out-of-date file has.
fn stale(expression: &str) -> CellValue {
    CellValue::Formula(Box::new(
        Formula::new(expression, Some(CellValue::Integer(999))).unwrap(),
    ))
}

fn source() -> Vec<u8> {
    let mut writer = WorkbookWriter::new(Default::default()).unwrap();
    for name in ["Lazy", "Model"] {
        writer.start_sheet(name).unwrap();
        writer
            .write_row(&Row {
                index: RowIndex::new(0).unwrap(),
                cells: vec![
                    cell("A1", CellValue::Integer(2)),
                    cell("B1", stale("A1*3")),
                    cell("C1", stale("C1")),
                ],
            })
            .unwrap();
    }
    writer.finish(Cursor::new(Vec::new())).unwrap().into_inner()
}

/// Cached values of every cell, as readers that do not calculate see them.
fn cached(bytes: Vec<u8>) -> HashMap<String, CellValue> {
    let mut reader = WorkbookReader::new(Cursor::new(bytes)).unwrap();
    let names: Vec<String> = reader.sheets().iter().map(|s| s.name().into()).collect();
    let mut values = HashMap::new();
    for name in names {
        let options = ReadOptions {
            data_only: true,
            ..ReadOptions::default()
        };
        let mut rows = reader.rows_with_options(&name, options).unwrap();
        while let Some(row) = rows.next_row().unwrap() {
            for cell in row.cells {
                values.insert(format!("{name}!{}", cell.address), cell.value);
            }
        }
    }
    values
}

#[test]
fn saved_formulas_carry_their_calculated_results() {
    let mut book =
        LoadedWorkbook::with_options(Cursor::new(source()), LoadOptions::default()).unwrap();
    let lazy = book.sheet_id("Lazy").unwrap();
    let model = book.sheet_id("Model").unwrap();
    // A pending edit on a sheet that stays unloaded.
    book.set_value(lazy, at("A1"), CellValue::Integer(5))
        .unwrap();
    // A rewritten model, and a created sheet.
    book.set_row_dimension(
        model,
        RowDimension {
            height: Some(20.0),
            ..RowDimension::new(RowIndex::new(0).unwrap())
        },
    )
    .unwrap();
    book.set_value(model, at("A1"), CellValue::Integer(7))
        .unwrap();
    let created = book.create_sheet("Created").unwrap();
    book.upsert_value(created, at("A1"), stale("Lazy!B1+Model!B1"))
        .unwrap();

    let stats = book.calculate().unwrap();
    let (output, _) = book
        .save(Cursor::new(Vec::new()), SaveOptions::default())
        .unwrap();
    let values = cached(output.into_inner());

    assert_eq!((stats.calculated, stats.skipped), (3, 2));
    assert_eq!(values["Lazy!B1"], CellValue::Number(15.0));
    assert_eq!(values["Model!B1"], CellValue::Number(21.0));
    assert_eq!(values["Created!A1"], CellValue::Number(36.0));
    // A formula that refers to itself has no result to keep.
    assert_eq!(values["Lazy!C1"], CellValue::Empty);
    assert_eq!(values["Model!C1"], CellValue::Empty);
}
