//! Validate finite public style domains from reference-generated packages.
use crabxl::{ColorKind, Fill, WorkbookReader, WorkbookWriter, WriteOptions};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("Missing fixture path")?;
    let mut book = WorkbookReader::new(std::fs::File::open(path)?)?;
    let catalog = book.style_catalog()?.ok_or("Missing styles")?;
    for size in [-1.0, 410.0, 1_000_000.0] {
        if !catalog.fonts.iter().any(|font| font.size == Some(size)) {
            return Err("Missing finite font size".into());
        }
    }
    if !catalog.fills.iter().any(|fill| matches!(fill, Fill::Gradient(g) if g.edges[0] == Some(-1.0) && g.edges[1] == Some(2.0))) {
        return Err("Missing gradient edges".into());
    }
    if !catalog
        .number_formats
        .iter()
        .any(|format| format.code().is_empty())
    {
        return Err("Missing empty number format".into());
    }
    if !catalog.fonts.iter().any(|font| {
        font.color
            .as_ref()
            .is_some_and(|color| color.kind == ColorKind::Indexed(3.into()))
    }) {
        return Err("Missing selected indexed color".into());
    }
    if let Some(output) = std::env::args().nth(2) {
        let sheet = book.read_sheet("Sheet")?;
        let catalog = book.into_style_catalog()?.ok_or("Missing styles")?;
        let mut writer = WorkbookWriter::from_style_catalog(WriteOptions::default(), catalog)?;
        writer.start_sheet("Sheet")?;
        for row in &sheet.rows {
            writer.write_row(row)?;
        }
        writer.finish(std::fs::File::create(output)?)?;
    }
    println!("finite-style-domains-ok");
    Ok(())
}
