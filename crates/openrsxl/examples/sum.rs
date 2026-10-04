//! Read and sum numeric cells while reusing one sparse row allocation.
use openrsxl::{
    AccessPattern, CellValue, MemoryPolicy, ReadData, ResourceLimits, Row, RowIndex, WorkbookReader,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args().skip(1);
    let path = arguments
        .next()
        .ok_or("Usage: sum <workbook.xlsx> [sheet] [buffer bytes] [stream|materialized|auto-scan|auto-repeat] [budget bytes|auto] [passes]")?;
    let requested_sheet = arguments.next();
    let mut limits = ResourceLimits::default();
    if let Some(bytes) = arguments.next() {
        limits.input_buffer_bytes = bytes.parse()?;
    }
    let mode = arguments.next().unwrap_or_else(|| "stream".into());
    let budget = arguments
        .next()
        .filter(|value| value != "auto")
        .map(|bytes| bytes.parse::<usize>())
        .transpose()?;
    if let Some(bytes) = budget {
        limits.max_materialized_bytes = bytes;
    }
    let passes = arguments
        .next()
        .map(|value| value.parse::<usize>())
        .transpose()?
        .unwrap_or(1);
    if passes == 0 {
        return Err("Pass count must be positive".into());
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
            for _ in 0..passes {
                let mut rows = workbook.rows(&sheet)?;
                let mut row = Row::new(RowIndex::new(0)?);
                while rows.read_row_into(&mut row)? {
                    accumulate(&row);
                }
            }
        }
        "materialized" => {
            let data = workbook.read_sheet(&sheet)?;
            for _ in 0..passes {
                for row in &data.rows {
                    accumulate(row);
                }
            }
        }
        "auto-scan" | "auto-repeat" => {
            let access = if mode == "auto-scan" {
                AccessPattern::Scan
            } else {
                AccessPattern::RepeatedAccess
            };
            let policy = budget.map(MemoryPolicy::Budget).unwrap_or_default();
            let stream_more = {
                let output = workbook.read_with_policy(&sheet, access, policy)?;
                eprintln!("DECISION {:?}", output.decision);
                match output.data {
                    ReadData::Streaming(mut rows) => {
                        let mut row = Row::new(RowIndex::new(0)?);
                        while rows.read_row_into(&mut row)? {
                            accumulate(&row);
                        }
                        true
                    }
                    ReadData::Materialized(data) => {
                        for _ in 0..passes {
                            for row in &data.rows {
                                accumulate(row);
                            }
                        }
                        false
                    }
                }
            };
            if stream_more {
                for _ in 1..passes {
                    let mut rows = workbook.rows(&sheet)?;
                    let mut row = Row::new(RowIndex::new(0)?);
                    while rows.read_row_into(&mut row)? {
                        accumulate(&row);
                    }
                }
            }
        }
        _ => return Err("Unknown read mode".into()),
    }
    println!("{count} {checksum:.0}");
    Ok(())
}
