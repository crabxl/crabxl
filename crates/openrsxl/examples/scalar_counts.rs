//! Count supported scalar types without retaining a worksheet.
use openrsxl::{CellValue, Row, RowIndex, WorkbookReader};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("Usage: scalar_counts <workbook.xlsx>")?;
    let mut workbook = WorkbookReader::open(path)?;
    let sheet = workbook
        .sheets()
        .first()
        .ok_or("Workbook has no sheets")?
        .name()
        .to_owned();
    let mut rows = workbook.rows(&sheet)?;
    let mut row = Row::new(RowIndex::new(0)?);
    let (mut numbers, mut booleans, mut trues, mut empty) = (0u64, 0u64, 0u64, 0u64);
    while rows.read_row_into(&mut row)? {
        for cell in &row.cells {
            match cell.value {
                CellValue::Number(_) => numbers += 1,
                CellValue::Boolean(value) => {
                    booleans += 1;
                    trues += u64::from(value);
                }
                CellValue::Empty => empty += 1,
                _ => return Err("Unsupported scalar type in this example".into()),
            }
        }
    }
    println!("{numbers} {booleans} {trues} {empty}");
    Ok(())
}
