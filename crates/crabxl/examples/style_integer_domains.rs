//! Exact large style identities with native and public-reference readback.
use crabxl::*;
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("Missing output path")?;
    let mut writer = WorkbookWriter::new(WriteOptions::default())?;
    let mut ids = Vec::new();
    for literal in [
        "9223372036854775808",
        "-9223372036854775809",
        "10000000000000000000000000000000000000000",
    ] {
        let integer = StyleInteger::parse(literal)?;
        for kind in [
            ColorKind::Theme(integer.clone()),
            ColorKind::Indexed(integer.clone()),
        ] {
            let color = Color { kind, tint: None };
            let mut style = CellStyle::default();
            style.font.charset = Some(integer.clone());
            style.font.color = Some(color.clone());
            style.fill = Fill::solid(color.clone());
            style.borders.sides[0]
                .as_mut()
                .ok_or("Missing default edge")?
                .color = Some(color);
            let id = writer.register_style(style.clone())?;
            if writer.register_style(style)? != id {
                return Err("Unstable style identity".into());
            }
            ids.push(id);
        }
    }
    writer.start_sheet("Sheet")?;
    for (index, id) in ids.iter().enumerate() {
        writer.write_row(&Row {
            index: RowIndex::new(index as u32)?,
            cells: vec![Cell {
                address: CellAddress::new(index as u32, 0)?,
                value: CellValue::Integer(1),
                style: *id,
            }],
        })?;
    }
    writer.finish(std::fs::File::create(&path)?)?;
    let mut reader = WorkbookReader::open(path)?;
    let source = reader.read_sheet("Sheet")?;
    let catalog = reader.into_style_catalog()?.ok_or("Missing styles")?;
    for (index, cell) in source.rows.iter().flat_map(|row| &row.cells).enumerate() {
        if cell.style != ids[index] {
            return Err("Style identity changed".into());
        }
        let font = catalog.cell_style(cell.style)?.font;
        if font
            .charset
            .as_ref()
            .and_then(StyleInteger::as_i64)
            .is_some()
        {
            return Err("Truncated style integer".into());
        }
    }
    println!(
        "{} {} {} {}",
        size_of::<StyleInteger>(),
        size_of::<Color>(),
        size_of::<Font>(),
        catalog.memory_bytes()
    );
    Ok(())
}
