//! Register combinations and re-register them to verify stable deduplicated IDs.
use crabxl::{Cell, CellAddress, CellValue, Row, RowIndex, StyleId, WorkbookWriter, WriteOptions};
#[path = "support/style_combinations.rs"]
mod combinations;
use combinations::style;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or("Usage: style_registry_fixture <file> <styles>")?;
    let count: u32 = args.next().ok_or("Missing style count")?.parse()?;
    if count == 0 || count > 8192 {
        return Err("Require 1..=8192 distinct combinations".into());
    }
    let mut writer = WorkbookWriter::new(WriteOptions {
        max_styles: count as usize + 5,
        ..Default::default()
    })?;
    let mut identities = Vec::<StyleId>::new();
    identities.try_reserve_exact(count as usize)?;
    for index in 0..count {
        identities.push(writer.register_style(style(index))?);
    }
    for index in 0..count {
        if writer.register_style(style(index))? != identities[index as usize] {
            return Err("Unstable style identity".into());
        }
    }
    if args.next().as_deref() == Some("stats") {
        let catalog = writer.style_catalog().ok_or("Missing catalog")?;
        println!(
            "{} {} {} {} {} {}",
            catalog.fonts.len(),
            catalog.fills.len(),
            catalog.borders.len(),
            catalog.number_formats.len(),
            catalog.cell_formats.len(),
            writer.style_memory_bytes()
        );
        writer.abort()?;
        return Ok(());
    }
    writer.start_sheet("Sheet")?;
    let mut row = Row::new(RowIndex::new(0)?);
    for index in 0..count {
        row.index = RowIndex::new(index)?;
        row.cells.clear();
        row.cells.push(Cell {
            address: CellAddress::new(index, 0)?,
            value: CellValue::Number(f64::from(index) + 0.25),
            style: identities[index as usize],
        });
        writer.write_row(&row)?;
    }
    writer.close_sheet()?;
    let stats = writer.stats();
    writer.finish(std::fs::File::create(path)?)?;
    eprintln!("TEMP_BYTES {}", stats.peak_temp_bytes);
    println!("{count}");
    Ok(())
}
