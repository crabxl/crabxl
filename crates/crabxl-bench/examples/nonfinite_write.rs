//! Stream a measured finite/nonfinite workload with compatible formula caches.
use crabxl::{
    Cell, CellAddress, CellValue, Formula, Row, RowIndex, StyleId, WorkbookWriter, WriteOptions,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or("Usage: nonfinite_write <file> <rows>")?;
    let count: u32 = args.next().ok_or("Missing row count")?.parse()?;
    let mut writer = WorkbookWriter::new(WriteOptions::default())?;
    writer.start_sheet("Sheet")?;
    let mut row = Row::new(RowIndex::new(0)?);
    for index in 0..count {
        row.index = RowIndex::new(index)?;
        row.cells.clear();
        let values = [
            CellValue::Number(f64::from(index) + 0.25),
            CellValue::Number(f64::INFINITY),
            CellValue::Number(f64::NEG_INFINITY),
            CellValue::Number(f64::NAN),
            CellValue::Formula(Box::new(Formula::new(
                format!("=A{}+1", index + 1),
                Some(CellValue::Number(f64::INFINITY)),
            )?)),
        ];
        for (column, value) in values.into_iter().enumerate() {
            row.cells.push(Cell {
                address: CellAddress::new(index, column as u32)?,
                value,
                style: StyleId::new(0),
            });
        }
        writer.write_row(&row)?;
    }
    writer.close_sheet()?;
    let stats = writer.stats();
    writer.finish(std::fs::File::create(path)?)?;
    eprintln!("TEMP_BYTES {}", stats.peak_temp_bytes);
    println!("{}", u64::from(count) * 5);
    Ok(())
}
