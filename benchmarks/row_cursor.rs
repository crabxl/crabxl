//! Generated public-model probe; XLSX parsing and Python conversion are separate.
use crabxl_core::{Cell, CellAddress, CellValue, EditLimits, RowIndex, StyleId, Worksheet};
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let rows: u32 = args.next().ok_or("Specify row count")?.parse()?;
    if rows == 0 || rows > 500_000 {
        return Err("Row count must be between 1 and 500000".into());
    }
    let step = match args.next().as_deref() {
        Some("dense") => 1,
        Some("sparse") => 2,
        _ => return Err("Specify dense or sparse coordinates".into()),
    };
    let mode = args.next().ok_or("Specify row or cell traversal")?;
    let mut sheet = Worksheet::new(
        "Data",
        EditLimits {
            max_bytes: 1024 * 1024 * 1024,
            max_cells: rows as usize * 10,
        },
    )?;
    for row in 0..rows {
        for column in 0..10 {
            sheet.set(Cell {
                address: CellAddress::new(row * step, column * step)?,
                value: CellValue::Integer(i64::from(row) * 10 + i64::from(column)),
                style: StyleId::new(0),
            })?;
        }
    }
    let repetitions = 5usize;
    let started = Instant::now();
    let mut cells = 0usize;
    let mut checksum = 0i64;
    let mut verify = |cell: &Cell| -> Result<(), Box<dyn std::error::Error>> {
        let row = cell.address.row.get();
        let column = cell.address.column.get();
        if row % step != 0 || column % step != 0 {
            return Err("Unexpected sparse coordinate".into());
        }
        let expected = i64::from(row / step) * 10 + i64::from(column / step);
        if cell.value != CellValue::Integer(expected) {
            return Err("Traversal returned an incorrect value".into());
        }
        cells += 1;
        checksum += expected;
        Ok(())
    };
    for _ in 0..repetitions {
        for row in 0..rows * step {
            match mode.as_str() {
                "row" => {
                    for cell in sheet.row_cells(RowIndex::new(row)?) {
                        verify(cell)?;
                    }
                }
                "cell" => {
                    for column in 0..10 * step {
                        if let Some(cell) = sheet.get(CellAddress::new(row, column)?) {
                            verify(cell)?;
                        }
                    }
                }
                _ => return Err("Unknown traversal mode".into()),
            }
        }
    }
    let count = i64::from(rows) * 10;
    if cells != count as usize * repetitions
        || checksum != count * (count - 1) / 2 * repetitions as i64
    {
        return Err("Incomplete or duplicated traversal".into());
    }
    println!(
        "{{\"cells\":{cells},\"checksum\":{checksum},\"traverse_seconds\":{},\"model_bytes\":{}}}",
        started.elapsed().as_secs_f64(),
        sheet.charged_bytes(),
    );
    Ok(())
}
