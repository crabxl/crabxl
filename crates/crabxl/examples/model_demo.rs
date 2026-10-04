//! Measure explicitly materialized sparse cells and structural mutation costs.
use crabxl::{Cell, CellAddress, CellValue, EditLimits, RowIndex, StyleId, Worksheet};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let count = std::env::args()
        .nth(1)
        .ok_or("Usage: model_demo <rows>")?
        .parse::<u32>()?;
    let mut sheet = Worksheet::new(
        "Sheet",
        EditLimits {
            max_bytes: 1024 * 1024 * 1024,
            max_cells: 2_000_000,
        },
    )?;
    for row in 0..count {
        for column in 0..10 {
            sheet.set(Cell {
                address: CellAddress::new(row, column)?,
                value: CellValue::Integer(i64::from(row) * 10 + i64::from(column)),
                style: StyleId::new(0),
            })?;
        }
    }
    sheet.insert_rows(RowIndex::new(0)?, 1)?;
    sheet.delete_rows(RowIndex::new(0)?, 1)?;
    let sum = sheet.cells().try_fold(0i64, |sum, cell| match cell.value {
        CellValue::Integer(value) => sum.checked_add(value).ok_or("Model checksum overflow"),
        _ => Err("Unexpected model cell type"),
    })?;
    println!("{} {} {}", sheet.len(), sum, sheet.charged_bytes());
    Ok(())
}
