//! Generate compatible blank serialization of nonfinite values and caches.
use crabxl::{
    Cell, CellAddress, CellValue, Formula, Row, RowIndex, StyleId, WorkbookWriter, WriteOptions,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("Usage: nonfinite_fixture <file>")?;
    let mut row = Row::new(RowIndex::new(0)?);
    for (column, value) in [
        CellValue::Number(f64::NAN),
        CellValue::Number(f64::INFINITY),
        CellValue::Number(f64::NEG_INFINITY),
        CellValue::Formula(Box::new(Formula::new(
            "=1",
            Some(CellValue::Number(f64::INFINITY)),
        )?)),
    ]
    .into_iter()
    .enumerate()
    {
        row.cells.push(Cell {
            address: CellAddress::new(0, column as u32)?,
            value,
            style: StyleId::new(0),
        });
    }
    let mut writer = WorkbookWriter::new(WriteOptions::default())?;
    writer.start_sheet("Sheet")?;
    writer.write_row(&row)?;
    writer.finish(std::fs::File::create(path)?)?;
    Ok(())
}
