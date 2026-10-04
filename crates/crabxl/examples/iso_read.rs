//! Verify generated ISO values without materializing a worksheet.
use crabxl::{CellValue, DateKind, Row, RowIndex, WorkbookReader};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("Usage: iso_read <file>")?;
    let mut book = WorkbookReader::open(path)?;
    let mut rows = book.rows("Sheet")?;
    let mut row = Row::new(RowIndex::new(0)?);
    let mut count = 0u64;
    while rows.read_row_into(&mut row)? {
        if row.cells.len() != 4 {
            return Err("Wrong row width".into());
        }
        for (index, cell) in row.cells.iter().enumerate() {
            let CellValue::DateTime(value) = &cell.value else {
                return Err("Expected date/time/duration".into());
            };
            let correct = match index {
                0 => value.kind() == DateKind::Date && value.to_iso8601()? == "1899-12-31",
                1 => {
                    value.kind() == DateKind::DateTime
                        && value.to_iso8601()? == "2024-02-29T12:03:04.123"
                }
                2 => value.kind() == DateKind::Time && value.to_iso8601()? == "12:03:04.123",
                3 => {
                    value.kind() == DateKind::Duration
                        && value.to_duration()?.num_seconds() == 108000
                }
                _ => false,
            };
            if !correct {
                return Err(format!("Wrong ISO value at cell {count}").into());
            }
            count += 1;
        }
    }
    println!("{count}");
    Ok(())
}
