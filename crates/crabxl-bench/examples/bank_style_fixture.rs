//! Build and export an explicitly owned bank with shared canonical style identities.
use crabxl::{
    Cell, CellAddress, CellValue, Workbook, WorkbookLimits, WorkbookWriter, WriteOptions,
};
#[path = "support/style_combinations.rs"]
mod combinations;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or("Missing output")?;
    let count: u32 = args.next().ok_or("Missing combinations")?.parse()?;
    if count == 0 || count > 8192 {
        return Err("Require 1..=8192 combinations".into());
    }
    let mut book = Workbook::new(WorkbookLimits::default())?;
    let sheet = book.create_sheet("Sheet")?;
    for index in 0..count {
        let id = book.register_style(combinations::style(index))?;
        if book.register_style(combinations::style(index))? != id {
            return Err("Unstable canonical identity".into());
        }
        book.sheet_mut(sheet)?.set(Cell {
            address: CellAddress::new(index, 0)?,
            value: CellValue::Number(f64::from(index) + 0.25),
            style: id,
        })?;
    }
    let charged = book.charged_bytes();
    let writer = WorkbookWriter::from_workbook(
        WriteOptions {
            max_styles: count as usize + 5,
            ..Default::default()
        },
        book,
    )?;
    eprintln!("BANK_BYTES {charged}");
    eprintln!("TEMP_BYTES {}", writer.stats().peak_temp_bytes);
    writer.finish(std::fs::File::create(path)?)?;
    println!("{count}");
    Ok(())
}
