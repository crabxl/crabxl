//! Export imported source IDs and a raw number-format edit into a new package.
use crabxl::{ReadOptions, WorkbookReader, WorkbookWriter, WriteOptions};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = std::env::args()
        .nth(1)
        .ok_or("Usage: style_catalog_export <input> <output>")?;
    let output = std::env::args().nth(2).ok_or("Missing output")?;
    let mut reader = WorkbookReader::open(input)?;
    let date_1904 = reader.date_1904();
    let mut sheet = reader.read_sheet_with_options(
        "Sheet",
        ReadOptions {
            data_only: true,
            ..Default::default()
        },
    )?;
    let catalog = reader
        .into_style_catalog()?
        .ok_or("Missing source styles")?;
    let mut format = catalog
        .cell_style(sheet.rows[0].cells[0].style)?
        .format
        .clone();
    format.number_format_id = 14;
    let mut writer = WorkbookWriter::from_style_catalog(
        WriteOptions {
            date_1904,
            max_styles: 100000,
            ..Default::default()
        },
        catalog,
    )?;
    sheet.rows[0].cells[0].style = writer.register_format(format)?;
    writer.start_sheet("Sheet")?;
    let mut count = 0usize;
    for row in &sheet.rows {
        writer.write_row(row)?;
        count += row.cells.len();
    }
    let temporary = writer.temporary_bytes();
    writer.finish(std::fs::File::create(output)?)?;
    eprintln!("STYLE_EXPORT_TEMP {temporary}");
    println!("{count}");
    Ok(())
}
