//! Verify visible normal/shared formula text and cache without unused hint semantics.
use crabxl::{CellValue, WorkbookReader};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("Missing input")?;
    let mut reader = WorkbookReader::open(path)?;
    let mut rows = reader.rows("Sheet")?;
    let mut count = 0;
    while let Some(row) = rows.next_row()? {
        for cell in row.cells {
            let CellValue::Formula(formula) = cell.value else {
                return Err("Lost formula".into());
            };
            if formula.expression() != "A1+1" || formula.cached() != Some(&CellValue::Integer(2)) {
                return Err("Changed formula or cache".into());
            }
            count += 1;
        }
    }
    println!("{count}");
    Ok(())
}
