//! Print visible source expressions for public shared-index interoperability.
use crabxl::{CellValue, WorkbookReader};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("Missing input")?;
    let mut book = WorkbookReader::open(path)?;
    let mut rows = book.rows("Sheet")?;
    while let Some(row) = rows.next_row()? {
        for cell in row.cells {
            let CellValue::Formula(value) = cell.value else {
                return Err("Expected formula".into());
            };
            println!("={}", value.expression());
        }
    }
    Ok(())
}
