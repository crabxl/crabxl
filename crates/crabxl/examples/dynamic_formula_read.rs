//! Verify projected dynamic array records and every cache using bounded rows.
use crabxl::{CellValue, FormulaType, Row, RowIndex, WorkbookReader};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("Usage: dynamic_formula_read <file>")?;
    let mut book = WorkbookReader::open(path)?;
    let mut rows = book.rows("Sheet")?;
    let mut row = Row::new(RowIndex::new(0)?);
    let mut count = 0u64;
    while rows.read_row_into(&mut row)? {
        let index = row.index.get() + 1;
        if row.cells.len() != 2 {
            return Err("Wrong dynamic formula row width".into());
        }
        let CellValue::Formula(formula) = &row.cells[0].value else {
            return Err("Missing array formula".into());
        };
        if formula.formula_type() != FormulaType::Array
            || formula.expression() != "_xlfn.SEQUENCE(1)"
            || formula.cached() != Some(&CellValue::Integer(i64::from(index)))
            || formula
                .metadata()
                .and_then(|m| m.reference.as_ref())
                .map(|r| r.spelling())
                != Some(format!("A{index}").into())
            || row.cells[1].value != CellValue::Integer(i64::from(index))
        {
            return Err("Incorrect dynamic formula/value/cache".into());
        }
        count += 2;
    }
    if rows.projected_metadata_cells() != count {
        return Err("Incorrect metadata projection count".into());
    }
    println!("{count}");
    Ok(())
}
