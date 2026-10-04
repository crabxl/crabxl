//! Public-compatible font domains and compact color spelling interoperability.
use crabxl::*;
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("Usage: style_domains <output.xlsx>")?;
    let mut writer = WorkbookWriter::new(WriteOptions::default())?;
    let mut ids = Vec::new();
    for (charset, kind) in [
        (-1, ColorKind::Theme(-1)),
        (256, ColorKind::Indexed(-1)),
        (4096, ArgbLiteral::parse("aaBbCcDd")?.into_kind()),
        (0, ArgbLiteral::parse("aAbBcC")?.into_kind()),
    ] {
        let mut style = CellStyle::default();
        style.font.family = Some(2.5);
        style.font.charset = Some(charset);
        style.font.color = Some(Color { kind, tint: None });
        ids.push(writer.register_style(style)?);
    }
    writer.start_sheet("Sheet")?;
    for (index, style) in ids.into_iter().enumerate() {
        writer.write_row(&Row {
            index: RowIndex::new(index as u32)?,
            cells: vec![Cell {
                address: CellAddress::new(index as u32, 0)?,
                value: CellValue::Integer(1),
                style,
            }],
        })?;
    }
    writer.finish(std::fs::File::create(path)?)?;
    Ok(())
}
