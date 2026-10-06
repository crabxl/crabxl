//! Generated sparse-edit semantics and failure-atomicity fixtures.
#![allow(clippy::unwrap_used)]
use crabxl_core::{
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

    // Exercise multiple storage blocks, backwards interior insertion, block
    // boundary removal and sparse row traversal against an independent map.
    let mut sheet = Worksheet::new("Sparse", EditLimits::default()).unwrap();
    let mut expected = std::collections::BTreeMap::new();
    for index in 0..653u32 {
        let coordinate = (index / 32 * 3, index % 32 * 2);
        set(
            &mut sheet,
            coordinate.0,
            coordinate.1,
            Value::Integer(index.into()),
        );
        expected.insert(coordinate, i64::from(index));
    }
    assert_eq!(sheet.cells().count(), 653);
    assert!(sheet.charged_bytes() < sheet.len() * 64);
    assert_eq!(sheet.row_cells(RowIndex::new(60).unwrap()).count(), 13);
    let mut reversed = Worksheet::new("Reverse", EditLimits::default()).unwrap();
    for index in (0..653u32).rev() {
        set(
            &mut reversed,
            index / 32 * 3,
            index % 32 * 2,
            Value::Integer(index.into()),
        );
    }
    assert_eq!(values(&reversed), values(&sheet));
    assert_eq!(reversed.row_cells(RowIndex::new(60).unwrap()).count(), 13);
    let dense_bytes = reversed.charged_bytes();
    for index in 1..653u32 {
        if index % 128 != 0 {
            reversed
                .remove(address(index / 32 * 3, index % 32 * 2))
                .unwrap();
        }
    }
    assert!(reversed.charged_bytes() < dense_bytes / 2);
    assert_eq!(reversed.len(), 6);
    for index in (0..640u32).rev() {
        let coordinate = (index / 32 * 3, index % 32 * 2 + 1);
        set(
            &mut sheet,
            coordinate.0,
            coordinate.1,
            Value::Integer(-i64::from(index)),
        );
        expected.insert(coordinate, -i64::from(index));
    }
    let removed = expected.keys().copied().step_by(3).collect::<Vec<_>>();
    for coordinate in removed {
        let value = expected.remove(&coordinate).unwrap();
        assert_eq!(
            sheet
                .remove(address(coordinate.0, coordinate.1))
                .unwrap()
                .value,
            Value::Integer(value)
        );
    }
    assert_eq!(sheet.len(), expected.len());
    let actual = sheet
        .cells()
        .map(|cell| {
            (
                (cell.address.row.get(), cell.address.column.get()),
                cell.value.clone(),
            )
        })
        .collect::<Vec<_>>();
    let reference = expected
        .iter()
        .map(|(coordinate, value)| (*coordinate, Value::Integer(*value)))
        .collect::<Vec<_>>();
    assert_eq!(actual, reference);
    for row in 0..60 {
        let actual = sheet
            .row_cells(RowIndex::new(row).unwrap())
            .map(|cell| (cell.address.column.get(), cell.value.clone()))
            .collect::<Vec<_>>();
        let reference = expected
            .range((row, 0)..=(row, u32::MAX))
            .map(|((_, column), value)| (*column, Value::Integer(*value)))
            .collect::<Vec<_>>();
        assert_eq!(actual, reference);
    }
    for (coordinate, value) in expected.iter().rev() {
        assert_eq!(
            sheet
                .get(address(coordinate.0, coordinate.1))
                .unwrap()
                .value,
            Value::Integer(*value)
        );
        sheet.remove(address(coordinate.0, coordinate.1)).unwrap();
    }
    assert!(sheet.is_empty());
    assert!(sheet.cells().next().is_none());
    set(&mut sheet, 0, 0, Value::Integer(42));
    assert_eq!(sheet.get(address(0, 0)).unwrap().value, Value::Integer(42));
}
#[test]
fn row_column_shifts_preserve_styles_and_formulas_verbatim() {
    use crabxl_core::Formula;
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
    // Whole-axis geometry must remain compact even when it covers more cells
    // than a 32-bit process could allocate. Reuse the shared address validator.
    for (source, canonical, count, rows, columns) in [
        ("$1:$1048576", "1:1048576", 17_179_869_184u64, true, false),
        ("$A:$XFD", "A:XFD", 17_179_869_184, false, true),
        ("$B$2:$D$4", "B2:D4", 9, false, false),
        ("A1", "A1", 1, false, false),
    ] {
        let range: crabxl_core::WorksheetRange = source.parse().unwrap();
        assert_eq!(range.to_string(), canonical);
        assert_eq!(range.cell_count(), count);
        assert_eq!(range.is_rows(), rows);
        assert_eq!(range.is_columns(), columns);
        assert!(range.contains(range.bounds().start));
        assert!(range.contains(range.bounds().end));
        assert_eq!(
            canonical.parse::<crabxl_core::WorksheetRange>().unwrap(),
            range
        );
    }
    assert!(std::mem::size_of::<crabxl_core::WorksheetRange>() <= 24);
    for source in [
        "",
        ":",
        "A:",
        ":B",
        "$:$",
        "A:1",
        "2:1",
        "Z:A",
        "A0",
        "1:1048577",
        "A:XFE",
        "A1:B2:C3",
        "Sheet!A1",
        "1:1x",
        "A$$:B",
        "A1:A0",
    ] {
        assert_eq!(
            source
                .parse::<crabxl_core::WorksheetRange>()
                .unwrap_err()
                .kind(),
            ErrorKind::InvalidData
        );
    }
    let columns: crabxl_core::WorksheetRange = "B:D".parse().unwrap();
    assert!(!columns.contains(address(0, 0)));
    assert!(columns.contains(address(MAX_ROWS - 1, 3)));
    // A buffer expansion can exceed the operation allowance even when its
    // eventual retained size would fit. Both set and append stay atomic.
    let mut growing = Worksheet::new(
        "Growth",
        EditLimits {
            max_bytes: 650,
            max_cells: 10,
        },
    )
    .unwrap();
    set(&mut growing, 0, 0, Value::Integer(1));
    growing.mark_clean();
    let retained = growing.charged_bytes();
    assert!(
        growing
            .set(Cell {
                address: address(0, 1),
                value: Value::Integer(2),
                style: StyleId::new(0),
            })
            .is_err()
    );
    assert!(growing.append(vec![Value::Integer(2)]).is_err());
    assert_eq!(growing.len(), 1);
    assert_eq!(growing.charged_bytes(), retained);
    assert_eq!(growing.row_extent(), 1);
    assert!(!growing.is_dirty());
    growing.remove(address(0, 0)).unwrap();
    assert_eq!(growing.charged_bytes(), "Growth".len());
    growing.append(vec![Value::Integer(3)]).unwrap();

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

#[test]
fn model_rename_obeys_budget_and_preserves_cells_on_failure() {
    let mut sheet = Worksheet::new(
        "A",
        EditLimits {
            max_bytes: 600,
            max_cells: 1,
        },
    )
    .unwrap();
    set(&mut sheet, 0, 0, Value::Integer(5));
    sheet.mark_clean();
    let before = sheet.charged_bytes();
    assert!(sheet.rename("x".repeat(100)).is_err());
    assert_eq!(sheet.name(), "A");
    assert_eq!(sheet.charged_bytes(), before);
    assert!(!sheet.is_dirty());
    sheet.rename("Data").unwrap();
    assert_eq!(sheet.name(), "Data");
    assert_eq!(sheet.charged_bytes(), before + 3);
    assert!(sheet.is_dirty());
}
