//! Verify streamed numeric dates, durations and formula caches against a generated fixture.
use crabxl::{CellValue, DateKind, ReadOptions, Row, RowIndex, WorkbookReader};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("Usage: styled_read <file>")?;
    let mut book = WorkbookReader::open(path)?;
    let catalog = book.style_catalog()?.ok_or("Missing style catalog")?;
    if catalog.cell_formats.len() != 3 || catalog.fonts.len() != 1 {
        return Err("Unexpected style catalog".into());
    }
    let bytes = book.style_memory_bytes();
    let mut rows = book.rows_with_options(
        "Sheet",
        ReadOptions {
            data_only: true,
            ..Default::default()
        },
    )?;
    let mut row = Row::new(RowIndex::new(0)?);
    let mut count = 0u64;
    while rows.read_row_into(&mut row)? {
        let index = count / 10;
        if row.cells.len() != 10 {
            return Err("Wrong row width".into());
        }
        let mut day = index % 365 + 1;
        let mut month = 1;
        for days in [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31] {
            if day <= days {
                break;
            }
            day -= days;
            month += 1;
        }
        let expected_date = format!("2024-{month:02}-{day:02} 06:00:00");
        for (column, cell) in row.cells.iter().enumerate() {
            let matches = match (column, &cell.value) {
                (0 | 9, CellValue::Number(value)) => *value == 1.25,
                (1 | 7, CellValue::DateTime(value)) => {
                    value.kind() == DateKind::DateTime
                        && value.serial() == 45292.25 + (index % 365) as f64
                        && value.to_datetime()?.to_string() == expected_date
                }
                (2, CellValue::DateTime(value)) => {
                    value.kind() == DateKind::Time && value.to_time()?.to_string() == "12:00:00"
                }
                (3, CellValue::DateTime(value)) => {
                    value.kind() == DateKind::Duration
                        && value.to_duration()?.num_seconds() == (index % 101) as i64 * 21600
                }
                (4, CellValue::Boolean(value)) => *value == (index % 2 != 0),
                (5, CellValue::Text(value)) => value.as_str() == format!("styled-{index:08}"),
                (6, CellValue::Error(value)) => value.as_str() == "#DIV/0!",
                (8, CellValue::Integer(value)) => *value == index as i64,
                _ => false,
            };
            if !matches {
                return Err(format!(
                    "Wrong value at row {index}, column {column}: {:?}",
                    cell.value
                )
                .into());
            }
            count += 1;
        }
    }
    eprintln!("STYLE_BYTES {bytes}");
    println!("{count}");
    Ok(())
}
