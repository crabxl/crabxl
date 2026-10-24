//! Verify formula cache projections without retaining discarded expressions.
use crabxl::{CellValue, ReadOptions, WorkbookReader};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("Usage: formula_cache_read <file>")?;
    let mut book = WorkbookReader::open(path)?;
    let options = ReadOptions {
        data_only: true,
        ..ReadOptions::default()
    };
    let rows = book.rows_with_options("Sheet", options)?;
    let mut count = 0u64;
    for row in rows {
        let row = row?;
        if row.cells.len() != 1
            || row.cells[0].value != CellValue::Integer(i64::from(row.index.get()) + 1)
        {
            return Err("Incorrect formula cache projection".into());
        }
        count += 1;
    }
    println!("{count}");
    Ok(())
}
