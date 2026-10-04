//! Transfer catalogs, validate typed values and edit a source format without remapping IDs.
use crabxl::{CellValue, DateKind, ReadOptions, StyleLimits, StyleRegistry, WorkbookReader};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("Usage: style_catalog_import <file> <formats>")?;
    let count: usize = std::env::args()
        .nth(2)
        .ok_or("Missing format count")?
        .parse()?;
    let mut reader = WorkbookReader::open(path)?;
    let mut sheet = reader.read_sheet_with_options(
        "Sheet",
        ReadOptions {
            data_only: true,
            ..Default::default()
        },
    )?;
    if sheet.rows.len() != 1 || sheet.rows[0].cells.len() != 10 {
        return Err("Incorrect source rows".into());
    }
    for cell in &sheet.rows[0].cells {
        let valid = match (cell.address.column.get(), &cell.value) {
            (0 | 9, CellValue::Number(value)) => *value == 1.25,
            (1 | 7, CellValue::DateTime(value)) => {
                value.kind() == DateKind::DateTime
                    && value.to_datetime()?.to_string() == "2024-01-01 06:00:00"
            }
            (2, CellValue::DateTime(value)) => {
                value.kind() == DateKind::Time && value.to_time()?.to_string() == "12:00:00"
            }
            (3, CellValue::DateTime(value)) => {
                value.kind() == DateKind::Duration && value.to_duration()?.num_seconds() == 0
            }
            (4, CellValue::Boolean(value)) => !value,
            (5, CellValue::Text(value)) => value.as_str() == "styled-00000000",
            (6, CellValue::Error(value)) => value.as_str() == "#DIV/0!",
            (8, CellValue::Integer(value)) => *value == 0,
            _ => false,
        };
        if !valid {
            return Err("Incorrect typed source value".into());
        }
    }
    let catalog = reader.into_style_catalog()?.ok_or("Missing catalog")?;
    if catalog.number_formats.len() != count {
        return Err("Wrong declared format count".into());
    }
    let mut styles = StyleRegistry::from_catalog(catalog, StyleLimits::default())?;
    let first = &mut sheet.rows[0].cells[0];
    let mut format = styles.catalog().cell_style(first.style)?.format.clone();
    format.number_format_id = 14;
    first.style = styles.register_format(format)?;
    if first.style.get() != 1
        || first.value != CellValue::Number(1.25)
        || styles.catalog().cell_style(first.style)?.number_format != Some("mm-dd-yy")
    {
        return Err("Incorrect source format edit".into());
    }
    eprintln!("STYLE_IMPORT_BYTES {}", styles.memory_bytes());
    println!("10");
    Ok(())
}
