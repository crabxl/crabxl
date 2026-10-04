//! Generated sparse-edit semantics and failure-atomicity fixtures.
#![allow(clippy::unwrap_used)]
use openrsxl_core::{
    Cell, CellAddress as Address, CellRange, CellValue as Value, ColumnIndex, EditLimits,
    ErrorKind, MAX_ROWS, RowIndex, StyleId, Worksheet,
};
fn address(row: u32, col: u32) -> Address {
    Address::new(row, col).unwrap()
}
fn set(sheet: &mut Worksheet, row: u32, col: u32, value: Value) {
    sheet
        .set(Cell {
            address: address(row, col),
            value,
            style: StyleId::new(0),
        })
        .unwrap();
}
fn values(sheet: &Worksheet) -> Vec<(String, Value)> {
    sheet
        .cells()
        .map(|cell| (cell.address.to_string(), cell.value.clone()))
        .collect()
}
#[test]
fn sparse_get_append_empty_rows_remove_and_dirty_tracking() {
    let mut sheet = Worksheet::new("Sheet", EditLimits::default()).unwrap();
    assert!(!sheet.is_dirty());
    assert!(sheet.is_empty());
    assert_eq!(sheet.append(vec![]).unwrap().get(), 0);
    assert_eq!(
        sheet
            .append(vec![Value::text("x"), Value::Integer(2)])
            .unwrap()
            .get(),
        1
    );
    assert_eq!(sheet.len(), 2);
    assert!(sheet.get(address(0, 0)).is_none());
    assert_eq!(sheet.get(address(1, 0)).unwrap().value, Value::text("x"));
    let bytes = sheet.charged_bytes();
    set(&mut sheet, 1, 0, Value::Integer(1));
    assert!(sheet.charged_bytes() < bytes);
    sheet.mark_clean();
    assert!(!sheet.is_dirty());
    assert!(sheet.remove(address(999, 99)).is_none());
    assert!(!sheet.is_dirty());
    assert!(sheet.remove(address(1, 1)).is_some());
    assert!(sheet.is_dirty());
    assert_eq!(sheet.append(vec![Value::Boolean(false)]).unwrap().get(), 2);
}
#[test]
fn row_column_shifts_preserve_styles_and_formulas_verbatim() {
    use openrsxl_core::Formula;
    let mut sheet = Worksheet::new("Sheet", EditLimits::default()).unwrap();
    let value = Value::Formula(Box::new(Formula::new("=A1", None).unwrap()));
    sheet
        .set(Cell {
            address: address(2, 2),
            value: value.clone(),
            style: StyleId::new(7),
        })
        .unwrap();
    set(&mut sheet, 0, 0, Value::Integer(1));
    sheet.insert_rows(RowIndex::new(1).unwrap(), 2).unwrap();
    sheet
        .insert_columns(ColumnIndex::new(1).unwrap(), 3)
        .unwrap();
    assert_eq!(sheet.get(address(4, 5)).unwrap().value, value);
    assert_eq!(sheet.get(address(4, 5)).unwrap().style, StyleId::new(7));
    sheet.delete_rows(RowIndex::new(0).unwrap(), 2).unwrap();
    sheet
        .delete_columns(ColumnIndex::new(0).unwrap(), 4)
        .unwrap();
    assert_eq!(sheet.len(), 1);
    assert_eq!(sheet.get(address(2, 1)).unwrap().value, value);
    assert_eq!(sheet.append(vec![Value::Integer(9)]).unwrap().get(), 3);
}
#[test]
fn overlapping_moves_and_copies_clear_destination_holes_without_dense_cells() {
    let mut sheet = Worksheet::new("Sheet", EditLimits::default()).unwrap();
    set(&mut sheet, 0, 0, Value::Integer(1));
    set(&mut sheet, 0, 2, Value::Integer(7));
    set(&mut sheet, 0, 3, Value::Integer(8));
    let range = CellRange::new(address(0, 0), address(0, 1)).unwrap();
    sheet.move_range(range, 0, 2).unwrap();
    assert_eq!(sheet.len(), 1);
    assert_eq!(sheet.get(address(0, 2)).unwrap().value, Value::Integer(1));
    assert!(sheet.get(address(0, 3)).is_none());
    set(&mut sheet, 0, 3, Value::Integer(8));
    set(&mut sheet, 0, 5, Value::Integer(9));
    sheet
        .copy_range(CellRange::new(address(0, 1), address(0, 3)).unwrap(), 0, 2)
        .unwrap();
    assert_eq!(
        values(&sheet),
        vec![
            ("C1".into(), Value::Integer(1)),
            ("E1".into(), Value::Integer(1)),
            ("F1".into(), Value::Integer(8))
        ]
    );
    sheet
        .move_range(CellRange::new(address(0, 2), address(0, 5)).unwrap(), 0, -1)
        .unwrap();
    assert_eq!(
        values(&sheet),
        vec![
            ("B1".into(), Value::Integer(1)),
            ("D1".into(), Value::Integer(1)),
            ("E1".into(), Value::Integer(8))
        ]
    );
}
#[test]
fn bounds_and_data_work_limits_fail_without_partial_mutation() {
    let mut sheet = Worksheet::new(
        "Sheet",
        EditLimits {
            max_bytes: 700,
            max_cells: 2,
        },
    )
    .unwrap();
    set(&mut sheet, MAX_ROWS - 1, 0, Value::Integer(1));
    sheet.mark_clean();
    let before = values(&sheet);
    assert_eq!(
        sheet
            .insert_rows(RowIndex::new(0).unwrap(), 1)
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidData
    );
    assert!(sheet.append(vec![]).is_err());
    assert_eq!(values(&sheet), before);
    assert!(!sheet.is_dirty());
    let cell = Cell {
        address: address(0, 0),
        value: Value::text("x".repeat(1024).into_boxed_str()),
        style: StyleId::new(0),
    };
    assert_eq!(
        sheet.set(cell).unwrap_err().kind(),
        ErrorKind::MemoryBudgetExceeded
    );
    assert_eq!(values(&sheet), before);
    let range = CellRange::new(address(MAX_ROWS - 1, 0), address(MAX_ROWS - 1, 0)).unwrap();
    assert!(sheet.move_range(range, 1, 0).is_err());
    assert_eq!(values(&sheet), before);
    set(&mut sheet, 0, 0, Value::Integer(2));
    sheet.mark_clean();
    let before = values(&sheet);
    assert_eq!(
        sheet
            .insert_columns(ColumnIndex::new(0).unwrap(), 1)
            .unwrap_err()
            .kind(),
        ErrorKind::MemoryBudgetExceeded
    );
    assert_eq!(values(&sheet), before);
    assert!(!sheet.is_dirty());
}
