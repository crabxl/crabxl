//! Stream ISO calendar/clock values and numeric elapsed durations for public tests.
use crabxl::{
    Cell, CellAddress, CellValue, ExcelDateTime, Row, RowIndex, StyleId, WorkbookWriter,
    WriteOptions,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args().skip(1);
    let path = arguments
        .next()
        .ok_or("Usage: iso_fixture <output> [rows] [mac]")?;
    let count = arguments
        .next()
        .map(|value| value.parse::<u32>())
        .transpose()?
        .unwrap_or(1);
    let mac = arguments.next().as_deref() == Some("mac");
    if count == 0 {
        return Err("Row count must be positive".into());
    }
    let values = [
        ExcelDateTime::from_ymd(1899, 12, 31)?,
        ExcelDateTime::from_ymd_hms_micro(2024, 2, 29, 12, 3, 4, 123456)?,
        ExcelDateTime::from_hms_micro(12, 3, 4, 123456)?,
        ExcelDateTime::from_duration_parts(1, 21600, 0)?,
    ];
    let mut writer = WorkbookWriter::new(WriteOptions {
        iso_dates: true,
        date_1904: mac,
        ..Default::default()
    })?;
    writer.start_sheet("Sheet")?;
    let mut row = Row {
        index: RowIndex::new(0)?,
        cells: values
            .into_iter()
            .enumerate()
            .map(|(column, value)| {
                Ok(Cell {
                    address: CellAddress::new(0, column as u32)?,
                    value: CellValue::DateTime(Box::new(value)),
                    style: StyleId::new(0),
                })
            })
            .collect::<Result<Vec<_>, crabxl::Error>>()?,
    };
    for index in 0..count {
        row.index = RowIndex::new(index)?;
        for cell in &mut row.cells {
            cell.address = CellAddress::new(index, cell.address.column.get())?;
        }
        writer.write_row(&row)?;
    }
    writer.close_sheet()?;
    let stats = writer.stats();
    writer.finish(std::fs::File::create(path)?)?;
    eprintln!("TEMP_BYTES {}", stats.peak_temp_bytes);
    println!("{}", count as u64 * 4);
    Ok(())
}
