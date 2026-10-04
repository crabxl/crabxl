//! Read and sum numeric cells while reusing one sparse row allocation.
use openrsxl::{CellValue, ResourceLimits, Row, RowIndex, WorkbookReader};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args().skip(1);
    let path = arguments
        .next()
        .ok_or("Usage: sum <workbook.xlsx> [sheet] [buffer bytes] [stream|materialized] [data budget bytes]")?;
    let requested_sheet = arguments.next();
    let mut limits = ResourceLimits::default();
    if let Some(bytes) = arguments.next() {
        limits.input_buffer_bytes = bytes.parse()?;
    }
    let mode = arguments.next().unwrap_or_else(|| "stream".into());
    if let Some(bytes) = arguments.next() {
        limits.max_materialized_bytes = bytes.parse()?;
    }
    let mut workbook = WorkbookReader::open_with_limits(path, limits)?;
    let sheet = requested_sheet
        .or_else(|| workbook.sheets().first().map(|s| s.name().to_owned()))
        .ok_or("Workbook has no sheets")?;
    let mut count = 0u64;
    let mut checksum = 0f64;
    let mut accumulate = |row: &Row| {
        for cell in &row.cells {
            if let CellValue::Number(value) = cell.value {
                count += 1;
                checksum += value;
            }
        }
    };
    match mode.as_str() {
        "stream" => {
            let mut rows = workbook.rows(&sheet)?;
            let mut row = Row::new(RowIndex::new(0)?);
            while rows.read_row_into(&mut row)? {
                accumulate(&row);
            }
        }
        "materialized" => {
            let data = workbook.read_sheet(&sheet)?;
            for row in &data.rows {
                accumulate(row);
            }
        }
        _ => return Err("Read mode must be stream or materialized".into()),
    }
    println!("{count} {checksum:.0}");
    Ok(())
}
