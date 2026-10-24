//! Verify every expanded shared formula and cache without materializing a sheet.
use crabxl::{CellValue, Row, RowIndex, WorkbookReader};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("Usage: shared_formula_read <file>")?;
    let mut book = WorkbookReader::open(path)?;
    let mut rows = book.rows("Sheet")?;
    let mut row = Row::new(RowIndex::new(0)?);
    let mut count = 0u64;
    while rows.read_row_into(&mut row)? {
        if row.cells.len() != 2 {
            return Err("Wrong formula row width".into());
        }
        let index = row.index.get() + 1;
        for (column, cell) in row.cells.iter().enumerate() {
            let CellValue::Formula(formula) = &cell.value else {
                return Err("Missing formula".into());
            };
            let expected = if column == 0 {
                format!("A{index}+$Z$1")
            } else {
                format!("B{index}+1")
            };
            if formula.expression() != expected
                || formula.cached() != Some(&CellValue::Integer(i64::from(index)))
            {
                return Err(format!("Incorrect formula/cache at {}", cell.address).into());
            }
            count += 1;
        }
    }
    let stats = rows.shared_formula_stats();
    eprintln!(
        "FORMULA_STATS {} {} {} {}",
        stats.templates, stats.accounted_bytes, stats.expanded, stats.unresolved
    );
    println!("{count}");
    Ok(())
}
