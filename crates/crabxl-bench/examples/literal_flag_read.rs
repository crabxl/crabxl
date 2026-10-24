//! Stream and verify each retained public data-table flag without boolean assumptions.
use crabxl::{CellValue, WorkbookReader};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("Missing input")?;
    let mut book = WorkbookReader::open(path)?;
    let mut rows = book.rows("Sheet")?;
    let mut count = 0;
    let literals = ["", "false", "invalid", "0", " true "];
    while let Some(row) = rows.next_row()? {
        for cell in row.cells {
            let CellValue::Formula(formula) = cell.value else {
                return Err("Lost formula".into());
            };
            let metadata = formula.metadata().ok_or("Lost metadata")?;
            let table = metadata.data_table.as_ref().ok_or("Lost table")?;
            let literal = literals[cell.address.column.get() as usize];
            for flag in [
                &metadata.flags.calculate_cell,
                &table.two_dimensions,
                &table.row_table,
                &table.deleted1,
                &table.deleted2,
            ] {
                if flag.as_ref().and_then(|value| value.source())
                    != if literal.is_empty() {
                        None
                    } else {
                        Some(literal)
                    }
                {
                    return Err("Changed literal flag".into());
                }
            }
            count += 1;
        }
    }
    println!("{count}");
    Ok(())
}
