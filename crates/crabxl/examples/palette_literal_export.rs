//! Transfer a source indexed palette through the canonical style catalog.
use crabxl::{WorkbookReader, WorkbookWriter, WriteOptions};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let input = args.next().ok_or("Missing input")?;
    let output = args.next().ok_or("Missing output")?;
    let mut reader = WorkbookReader::open(input)?;
    let sheet = reader.read_sheet("Sheet")?;
    let catalog = reader.into_style_catalog()?.ok_or("Missing styles")?;
    let mut writer = WorkbookWriter::from_style_catalog(WriteOptions::default(), catalog)?;
    writer.start_sheet("Sheet")?;
    for row in &sheet.rows {
        writer.write_row(row)?;
    }
    writer.finish(std::fs::File::create(output)?)?;
    Ok(())
}
