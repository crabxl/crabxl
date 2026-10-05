//! Verify every value and shared style component from the registration workload.
use crabxl::{CellValue, ColorKind, Fill, FillPattern, Row, RowIndex, WorkbookReader};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("Usage: style_registry_read <file>")?;
    let mut book = WorkbookReader::open(path)?;
    let mut rows = book.rows("Sheet")?;
    let mut row = Row::new(RowIndex::new(0)?);
    let mut count = 0u32;
    while rows.read_row_into(&mut row)? {
        if row.cells.len() != 1 {
            return Err("Wrong style row width".into());
        }
        let cell = &row.cells[0];
        if cell.value != CellValue::Number(f64::from(count) + 0.25) {
            return Err("Wrong numeric value".into());
        }
        let catalog = rows.style_catalog().ok_or("Missing style catalog")?;
        let style = catalog.cell_style(cell.style)?;
        let Fill::Pattern(fill) = style.fill else {
            return Err("Expected pattern fill".into());
        };
        if style.number_format != Some("0.000")
            || style.font.name.as_deref() != Some("Calibri")
            || style.font.size != Some(11.0)
            || style.font.color.as_ref().map(|color| &color.kind)
                != Some(&ColorKind::Argb(0xFF000000 | (count % 16)))
            || fill.pattern != Some(FillPattern::Solid)
            || fill.foreground.as_ref().map(|color| &color.kind)
                != Some(&ColorKind::Argb(0xFF100000 | ((count / 16) % 16)))
            || style
                .alignment
                .map(|alignment| alignment.rotation.unwrap_or(0))
                != Some(((count / 256) % 32) as u8)
        {
            return Err(format!("Wrong style at {}", cell.address).into());
        }
        count += 1;
    }
    println!("{count}");
    Ok(())
}
