//! Aggregate owned-workbook allowances and stable identity behavior.
#![allow(clippy::unwrap_used)]
use openrsxl_core::{
    Cell, CellAddress, CellValue, DateEpoch, EditLimits, ErrorKind, RowIndex, StyleId, Workbook,
    WorkbookLimits,
};
fn cell(row: u32, value: i64) -> Cell {
    Cell {
        address: CellAddress::new(row, 0).unwrap(),
        value: CellValue::Integer(value),
        style: StyleId::new(0),
    }
}
#[test]
fn sheet_ids_survive_copy_reorder_rename_and_reject_removed_foreign_handles() {
    let mut book = Workbook::new(WorkbookLimits::default()).unwrap();
    let first = book.create_sheet("First").unwrap();
    let second = book.create_sheet("Second").unwrap();
    book.sheet_mut(first).unwrap().set(cell(0, 1)).unwrap();
    book.sheet_mut(first).unwrap().append(vec![]).unwrap();
    book.sheet_mut(first)
        .unwrap()
        .append(vec![CellValue::Integer(3)])
        .unwrap();
    let copied = book.copy_sheet(first, "Copy").unwrap();
    assert_eq!(book.sheet(copied).unwrap().row_extent(), 3);
    book.sheet_mut(copied).unwrap().set(cell(0, 7)).unwrap();
    assert_eq!(
        book.sheet(first)
            .unwrap()
            .get(CellAddress::new(0, 0).unwrap())
            .unwrap()
            .value,
        CellValue::Integer(1)
    );
    book.rename_sheet(first, "Renamed").unwrap();
    book.set_active_sheet(copied).unwrap();
    book.move_sheet(copied, 0).unwrap();
    assert_eq!(book.active_index(), Some(0));
    assert_eq!(book.sheet_id("Renamed"), Some(first));
    let transferred = book.remove_sheet(second).unwrap();
    assert_eq!(transferred.name(), "Second");
    assert_eq!(
        book.sheet(second).err().unwrap().kind(),
        ErrorKind::SheetNotFound
    );
    let new = book.create_sheet("Second").unwrap();
    assert_ne!(new, second);
    let other = Workbook::new(WorkbookLimits::default()).unwrap();
    assert_eq!(
        other.sheet(first).err().unwrap().kind(),
        ErrorKind::SheetNotFound
    );
    assert!(book.rename_sheet(new, "renamed").is_err());
    assert_eq!(book.sheet(new).unwrap().name(), "Second");
    book.set_epoch(DateEpoch::Mac1904);
    assert_eq!(book.epoch(), DateEpoch::Mac1904);
}
#[test]
fn aggregate_bytes_cells_and_work_budget_fail_without_changing_models() {
    let mut book = Workbook::new(WorkbookLimits {
        max_bytes: 1200,
        max_cells: 2,
        max_sheets: 4,
        sheet: EditLimits {
            max_bytes: 1000,
            max_cells: 2,
        },
    })
    .unwrap();
    let a = book.create_sheet("A").unwrap();
    let b = book.create_sheet("B").unwrap();
    book.sheet_mut(a).unwrap().set(cell(0, 1)).unwrap();
    book.sheet_mut(b).unwrap().set(cell(0, 2)).unwrap();
    let bytes = book.charged_bytes();
    assert!(bytes <= 1200);
    assert_eq!(
        book.sheet_mut(a)
            .unwrap()
            .set(cell(1, 3))
            .unwrap_err()
            .kind(),
        ErrorKind::MemoryBudgetExceeded
    );
    assert_eq!(book.cell_count(), 2);
    assert_eq!(book.charged_bytes(), bytes);
    assert!(
        book.sheet_mut(a)
            .unwrap()
            .insert_rows(RowIndex::new(0).unwrap(), 1)
            .is_err()
    );
    assert!(
        book.sheet(a)
            .unwrap()
            .get(CellAddress::new(0, 0).unwrap())
            .is_some()
    );
    assert!(book.copy_sheet(a, "Copy").is_err());
    assert_eq!(book.len(), 2);
    assert!(book.rename_sheet(a, "x".repeat(1000)).is_err());
    assert_eq!(book.sheet(a).unwrap().name(), "A");
    book.sheet_mut(b)
        .unwrap()
        .remove(CellAddress::new(0, 0).unwrap());
    book.sheet_mut(a).unwrap().set(cell(1, 3)).unwrap();
    assert_eq!(book.cell_count(), 2);
}
#[test]
fn freed_space_is_reusable_and_slot_capacity_remains_accounted() {
    let mut book = Workbook::new(WorkbookLimits {
        max_bytes: 1600,
        max_cells: 10,
        max_sheets: 2,
        ..WorkbookLimits::default()
    })
    .unwrap();
    let a = book.create_sheet("A").unwrap();
    book.sheet_mut(a)
        .unwrap()
        .append(vec![CellValue::Integer(1), CellValue::Integer(2)])
        .unwrap();
    let b = book.create_sheet("B").unwrap();
    assert_eq!(book.charged_bytes(), 1026);
    assert_eq!(
        book.create_sheet("C").unwrap_err().kind(),
        ErrorKind::LimitExceeded
    );
    book.remove_sheet(a).unwrap();
    assert_eq!(book.charged_bytes(), 513);
    book.sheet_mut(b)
        .unwrap()
        .append(vec![
            CellValue::Integer(1),
            CellValue::Integer(2),
            CellValue::Integer(3),
            CellValue::Integer(4),
        ])
        .unwrap();
    assert_eq!(book.charged_bytes(), 1537);
    assert_eq!(book.active_sheet(), Some(b));
}
