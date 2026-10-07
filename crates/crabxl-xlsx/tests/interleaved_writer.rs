//! Independently appendable spool ownership and bounds; generated fixtures.
#![allow(clippy::unwrap_used)]
use crabxl_core::{Cell, CellAddress, CellValue, Row, RowIndex, StyleId};
use crabxl_xlsx::{WorkbookReader, WorkbookWriter, WriteOptions};
use std::io::Cursor;

fn row(index: u32, value: i64) -> Row {
    Row {
        index: RowIndex::new(index).unwrap(),
        cells: vec![Cell {
            address: CellAddress::new(index, 0).unwrap(),
            value: CellValue::Integer(value),
            style: StyleId::new(0),
        }],
    }
}

#[test]
fn interleaved_spools_keep_creation_order_rows_names_and_active_sheet() {
    let directory = tempfile::tempdir().unwrap();
    let mut writer = WorkbookWriter::new(WriteOptions {
        temp_directory: Some(directory.path().into()),
        ..Default::default()
    })
    .unwrap();
    let first = writer.start_interleaved_sheet("First").unwrap();
    writer
        .group_interleaved_rows(first, RowIndex::FIRST, RowIndex::new(2).unwrap(), 2, true)
        .unwrap();
    let mut trailing = crabxl_core::RowDimension::new(RowIndex::new(10).unwrap());
    trailing.height = Some(31.0);
    writer
        .set_interleaved_row_dimension(first, trailing)
        .unwrap();
    let mut column = crabxl_core::ColumnDimension::new(
        crabxl_core::ColumnIndex::new(1).unwrap(),
        crabxl_core::ColumnIndex::new(4).unwrap(),
    )
    .unwrap();
    column.width = Some(22.0);
    writer
        .set_interleaved_column_dimension(first, column.clone())
        .unwrap();
    writer.write_row(&row(0, 1)).unwrap();
    assert!(
        writer
            .set_interleaved_column_dimension(first, column)
            .is_err()
    );
    let second = writer.start_interleaved_sheet("Second").unwrap();
    writer.write_row(&row(0, 2)).unwrap();
    writer.activate_sheet(first).unwrap();
    writer.write_row(&row(1, 3)).unwrap();
    assert!(writer.start_interleaved_sheet("SECOND").is_err());
    writer.write_row(&row(2, 4)).unwrap();
    writer.rename_interleaved_sheet(first, "Renamed").unwrap();
    writer.close_interleaved_sheet(first).unwrap();
    assert!(writer.activate_sheet(first).is_err());
    writer.activate_sheet(second).unwrap();
    writer.write_row(&row(1, 5)).unwrap();
    writer.set_active_sheet(1).unwrap();
    let output = writer.finish(Cursor::new(Vec::new())).unwrap();
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    let mut reader = WorkbookReader::new(output).unwrap();
    assert_eq!(
        reader
            .sheets()
            .iter()
            .map(|sheet| sheet.name())
            .collect::<Vec<_>>(),
        vec!["Renamed", "Second"]
    );
    assert_eq!(reader.active_index(), Some(1));
    assert_eq!(
        reader
            .read_sheet("Renamed")
            .unwrap()
            .rows
            .iter()
            .filter(|row| !row.cells.is_empty())
            .map(|row| &row.cells[0].value)
            .collect::<Vec<_>>(),
        vec![
            &CellValue::Integer(1),
            &CellValue::Integer(3),
            &CellValue::Integer(4)
        ]
    );
    assert_eq!(reader.read_sheet("Second").unwrap().rows.len(), 2);
    assert_eq!(reader.worksheet_dimension("Renamed").unwrap(), None);
    let columns = reader.column_dimensions("Renamed").unwrap();
    assert_eq!(columns.columns()[0].width, Some(22.0));
    let mut rows = reader.rows("Renamed").unwrap();
    rows.capture_dimensions();
    let mut dimensions = Vec::new();
    while rows.next_row().unwrap().is_some() {
        dimensions.push(rows.row_dimension().cloned());
    }
    assert_eq!(dimensions.len(), 4);
    assert_eq!(dimensions[0].as_ref().unwrap().outline_level, Some(2));
    assert_eq!(dimensions[3].as_ref().unwrap().height, Some(31.0));
}

#[test]
fn abort_releases_paused_spools_and_sheet_limit_rejection_retains_active() {
    let directory = tempfile::tempdir().unwrap();
    let mut writer = WorkbookWriter::new(WriteOptions {
        max_sheets: 2,
        temp_directory: Some(directory.path().into()),
        ..Default::default()
    })
    .unwrap();
    writer.start_interleaved_sheet("First").unwrap();
    writer.start_interleaved_sheet("Second").unwrap();
    assert!(writer.start_interleaved_sheet("Third").is_err());
    writer.write_row(&row(0, 7)).unwrap();
    writer.abort().unwrap();
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}
